//! WASAPI shared-mode capture: no Stereo Mix driver or system-wide changes.
use std::{fs::File, io::{BufWriter, Write}, path::{Path, PathBuf}, sync::{Arc, Mutex, mpsc, atomic::{AtomicBool, AtomicU64, Ordering}}, thread::{self, JoinHandle}, time::Duration};
use windows::{core::GUID, Win32::{Media::Audio::*, System::{Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED}, Performance::{QueryPerformanceCounter, QueryPerformanceFrequency}}}};
use windows::{core::w, Win32::{Foundation::HANDLE, System::Threading::{AvSetMmThreadCharacteristicsW, AvRevertMmThreadCharacteristics}}};
#[derive(Clone)]
pub struct Track { pub path: PathBuf, pub rate: u32, pub channels: u16, pub format: &'static str, frame_bytes: usize, silence: u8 }
#[cfg(test)] impl Track { pub fn float_stereo(path: PathBuf) -> Self { Self { path, rate: 48_000, channels: 2, format: "f32le", frame_bytes: 8, silence: 0 } } }
pub fn clock() -> u64 { unsafe { let mut ticks = 0; let mut frequency = 0; if QueryPerformanceCounter(&mut ticks).is_err() || QueryPerformanceFrequency(&mut frequency).is_err() || frequency <= 0 { return 0; } (ticks as u128 * 10_000_000 / frequency as u128) as u64 } }
pub struct AudioGroup {
    stop: Arc<AtomicBool>, start: Arc<AtomicU64>, end: Arc<AtomicU64>,
    workers: Vec<JoinHandle<()>>, pub tracks: Vec<Track>, errors: Arc<Mutex<Vec<String>>>,
}
pub struct AudioResult { pub tracks: Vec<Track>, pub errors: Vec<String> }
impl AudioGroup {
    pub fn prepare(folder: &Path, segment: usize, system: bool, microphone: bool) -> Result<Self, String> {
        let mut group = Self { stop: Arc::new(AtomicBool::new(false)), start: Arc::new(AtomicU64::new(0)), end: Arc::new(AtomicU64::new(0)), workers: vec![], tracks: vec![], errors: Arc::new(Mutex::new(vec![])) };
        for (enabled, loopback, label) in [(system, true, "system sound"), (microphone, false, "microphone")] {
            if !enabled { continue; }
            let path = folder.join(format!("audio-{segment}-{}.pcm", if loopback { "system" } else { "mic" }));
            let stop = group.stop.clone(); let start = group.start.clone(); let end = group.end.clone(); let errors = group.errors.clone();
            let (ready, wait) = mpsc::channel();
            group.workers.push(thread::spawn(move || {
                if let Err(e) = capture(&path, loopback, &stop, &start, &end, &ready) {
                    let message = format!("Could not record {label}: {e}. Check the default audio device and Windows microphone privacy permission.");
                    let _ = ready.send(Err(message.clone())); if let Ok(mut list) = errors.lock() { list.push(message); }
                }
            }));
            match wait.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(track)) => group.tracks.push(track), Ok(Err(e)) => return Err(e), Err(_) => return Err(format!("The {label} device did not initialize in time")),
            }
        }
        Ok(group)
    }
    pub fn begin(&self) { self.start.store(clock(), Ordering::SeqCst); }
    pub fn errors(&self) -> Vec<String> { self.errors.lock().map(|e| e.clone()).unwrap_or_default() }
    fn stop_workers(&mut self) {
        self.end.store(clock(), Ordering::SeqCst); self.stop.store(true, Ordering::SeqCst);
        for worker in self.workers.drain(..) { let _ = worker.join(); }
    }
    pub fn finish(mut self) -> AudioResult { self.stop_workers(); AudioResult { tracks: self.tracks.clone(), errors: self.errors() } }
}
impl Drop for AudioGroup { fn drop(&mut self) { self.stop_workers(); } }
struct Com;
impl Drop for Com { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
struct AudioPriority(Option<HANDLE>);
impl AudioPriority { fn enter() -> Self { let mut index = 0; Self(unsafe { AvSetMmThreadCharacteristicsW(w!("Audio"), &mut index).ok() }) } }
impl Drop for AudioPriority { fn drop(&mut self) { if let Some(handle) = self.0 { unsafe { let _ = AvRevertMmThreadCharacteristics(handle); } } } }
fn silence(output: &mut impl Write, frames: u64, track: &Track) -> Result<(), String> {
    let zeros = [track.silence; 8192]; let mut bytes = frames * track.frame_bytes as u64;
    while bytes > 0 { let chunk = bytes.min(zeros.len() as u64) as usize; output.write_all(&zeros[..chunk]).map_err(|e| e.to_string())?; bytes -= chunk as u64; } Ok(())
}
fn pad(output: &mut impl Write, written: &mut u64, target: u64, track: &Track) -> Result<(), String> { if target > *written { silence(output, target - *written, track)?; *written = target; } Ok(()) }
fn frames(ticks: u64, rate: u32) -> u64 { (ticks as u128 * rate as u128 / 10_000_000) as u64 }
#[derive(Default)]
struct Timeline { written: u64, device_end: Option<u64> }
struct Packet { position: u64, timestamp: u64, count: u32, discontinuity: bool, timestamp_valid: bool }
impl Timeline {
    fn append(&mut self, output: &mut impl Write, track: &Track, packet: Packet, bytes: Option<&[u8]>, base: u64, now: u64, stop: Option<u64>) -> Result<(), String> {
        if base == 0 { return Ok(()); }
        let wall = frames(now.saturating_sub(base), track.rate);
        let anchored = frames(packet.timestamp.saturating_sub(base), track.rate).min(wall.saturating_add(track.rate as u64));
        let (target, skip) = match self.device_end {
            // Align once at the beginning. Do NOT repeatedly cut samples to fit
            // packet timestamps: driver jitter and queued packets would stutter.
            None => (anchored, frames(base.saturating_sub(packet.timestamp), track.rate).min(packet.count as u64)),
            Some(expected) if expected == packet.position && !packet.discontinuity => (self.written, 0),
            Some(expected) => {
                let gap = packet.position.saturating_sub(expected);
                let bounded_gap = gap.min(wall.saturating_sub(self.written).saturating_add(track.rate as u64));
                let target = if packet.timestamp_valid && (packet.discontinuity || packet.position < expected) {
                    // Ignore sub-packet clock jitter; re-anchor only real gaps.
                    if anchored > self.written.saturating_add(track.rate as u64 / 50) { anchored } else { self.written.saturating_add(bounded_gap) }
                } else { self.written.saturating_add(bounded_gap) };
                (target, 0)
            },
        };
        let mut count = packet.count as u64 - skip;
        if let Some(stop) = stop { count = count.min(frames(stop.saturating_sub(base), track.rate).saturating_sub(target.max(self.written))); }
        if count == 0 { return Ok(()); }
        pad(output, &mut self.written, target, track)?;
        if let Some(bytes) = bytes {
            let begin = skip as usize * track.frame_bytes; let end = begin + count as usize * track.frame_bytes;
            output.write_all(bytes.get(begin..end).ok_or("Invalid audio packet size")?).map_err(|e| e.to_string())?;
        } else { silence(output, count, track)?; }
        self.written += count;
        self.device_end = Some(packet.position.saturating_add(packet.count as u64));
        Ok(())
    }
}
struct MixFormat(*mut WAVEFORMATEX);
impl Drop for MixFormat { fn drop(&mut self) { unsafe { CoTaskMemFree(Some(self.0 as _)); } } }
fn describe(path: &Path, format: WAVEFORMATEX, subformat: Option<GUID>) -> Result<Track, String> {
    let tag = if format.wFormatTag == 0xfffe { match subformat {
        Some(id) if id == GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71) => 3,
        Some(id) if id == GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71) => 1,
        _ => return Err("Unsupported audio device subformat".into()),
    } } else { format.wFormatTag };
    let encoding = match (tag, format.wBitsPerSample) { (3, 32) => "f32le", (3, 64) => "f64le", (1, 8) => "u8", (1, 16) => "s16le", (1, 24) => "s24le", (1, 32) => "s32le", _ => return Err("Unsupported audio sample format".into()) };
    let frame_bytes = format.nChannels as usize * format.wBitsPerSample as usize / 8;
    if !(8000..=384000).contains(&{format.nSamplesPerSec}) || !(1..=8).contains(&{format.nChannels}) || frame_bytes != format.nBlockAlign as usize { return Err("Unsupported audio device layout".into()); }
    Ok(Track { path: path.into(), rate: format.nSamplesPerSec, channels: format.nChannels, format: encoding, frame_bytes, silence: if encoding == "u8" { 128 } else { 0 } })
}
fn capture(path: &Path, loopback: bool, stop: &AtomicBool, start: &AtomicU64, end: &AtomicU64, ready: &mpsc::Sender<Result<Track, String>>) -> Result<(), String> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().map_err(|e| e.to_string())?; let _com = Com;
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| format!("Audio device service: {e}"))?;
        let device = enumerator.GetDefaultAudioEndpoint(if loopback { eRender } else { eCapture }, eConsole).map_err(|e| format!("Default audio endpoint: {e}"))?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|e| format!("Activate audio endpoint: {e}"))?;
        // Capture in the endpoint's native mix format. FFmpeg resamples/downmixes
        // later, so 44.1 kHz, mono microphones, and surround devices remain valid.
        let format = MixFormat(client.GetMixFormat().map_err(|e| format!("Read device format: {e}"))?);
        if format.0.is_null() { return Err("Audio device returned no format".into()); }
        let base_format = *format.0;
        let subformat = if base_format.wFormatTag == 0xfffe && base_format.cbSize >= 22 { Some((*(format.0 as *const WAVEFORMATEXTENSIBLE)).SubFormat) } else { None };
        let track = describe(path, base_format, subformat)?;
        let rate = track.rate;
        let flags = if loopback { AUDCLNT_STREAMFLAGS_LOOPBACK } else { 0 };
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 5_000_000, 0, format.0, None).map_err(|e| format!("Initialize audio stream: {e}"))?;
        let source: IAudioCaptureClient = client.GetService().map_err(|e| e.to_string())?;
        let mut output = BufWriter::with_capacity(256 * 1024, File::create(path).map_err(|e| e.to_string())?);
        let _priority = AudioPriority::enter();
        client.Start().map_err(|e| format!("Start audio stream: {e}"))?; let _ = ready.send(Ok(track.clone()));
        let result = (|| -> Result<(), String> {
            let mut timeline = Timeline::default();
            let mut buffer = Vec::with_capacity(64 * 1024);
            loop {
                let stopping = stop.load(Ordering::SeqCst);
                let mut packets = 0;
                while source.GetNextPacketSize().map_err(|e| e.to_string())? > 0 && packets < 200 {
                    packets += 1;
                    let mut data = std::ptr::null_mut(); let mut count = 0; let mut flags = 0; let mut timestamp = 0; let mut position = 0;
                    source.GetBuffer(&mut data, &mut count, &mut flags, Some(&mut position), Some(&mut timestamp)).map_err(|e| e.to_string())?;
                    let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null();
                    buffer.clear();
                    if !silent { buffer.extend_from_slice(std::slice::from_raw_parts(data, count as usize * track.frame_bytes)); }
                    // Release WASAPI's buffer before any potentially blocking disk I/O.
                    source.ReleaseBuffer(count).map_err(|e| e.to_string())?;
                    let base = start.load(Ordering::SeqCst);
                    let now = clock();
                    let timestamp_valid = flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 == 0 && timestamp != 0;
                    if !timestamp_valid { timestamp = now.saturating_sub(count as u64 * 10_000_000 / rate as u64); }
                    let packet = Packet { position, timestamp, count, discontinuity: flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0, timestamp_valid };
                    timeline.append(&mut output, &track, packet, if silent { None } else { Some(&buffer) }, base, now, if stopping { Some(end.load(Ordering::SeqCst)) } else { None })?;
                }
                if stopping {
                    let base = start.load(Ordering::SeqCst);
                    if base != 0 { pad(&mut output, &mut timeline.written, frames(end.load(Ordering::SeqCst).saturating_sub(base), rate), &track)?; }
                    return Ok(());
                }
                // No live wall-clock padding: queued audio must never be replaced
                // with silence simply because the capture thread was scheduled late.
                thread::sleep(Duration::from_millis(5));
            }
        })();
        let _ = client.Stop(); output.flush().map_err(|e| e.to_string())?; result
    }
}

#[cfg(test)] mod tests {
    use super::*;
    fn packet(position: u64, timestamp: u64, count: u32) -> Packet { Packet { position, timestamp, count, discontinuity: false, timestamp_valid: true } }
    #[test] fn queued_packets_and_jitter_preserve_every_sample() {
        let track = Track::float_stereo(PathBuf::new()); let base = 10_000_000;
        let mut timeline = Timeline::default(); let mut output = vec![]; let mut expected = vec![];
        for index in 0..100u64 {
            let bytes = vec![index as u8 + 1; 480 * 8]; expected.extend_from_slice(&bytes);
            // Simulate a delayed batch, then jittering and invalid timestamps.
            let mut p = packet(index * 480, if index == 0 { base } else { base + index * 100_000 + if index % 2 == 0 { 80_000 } else { 0 } }, 480);
            if index % 3 == 1 { p.timestamp_valid = false; p.timestamp = base; }
            timeline.append(&mut output, &track, p, Some(&bytes), base, base + 20_000_000, None).unwrap();
        }
        assert_eq!(output, expected); assert_eq!(timeline.written, 48_000);
    }
    #[test] fn genuine_device_gap_inserts_only_missing_frames() {
        let track = Track::float_stereo(PathBuf::new()); let base = 10_000_000;
        let mut timeline = Timeline::default(); let mut output = vec![];
        timeline.append(&mut output, &track, packet(0, base, 480), Some(&vec![1; 3840]), base, base + 1_000_000, None).unwrap();
        timeline.append(&mut output, &track, packet(960, base + 200_000, 480), Some(&vec![2; 3840]), base, base + 1_000_000, None).unwrap();
        assert_eq!(&output[..3840], &[1; 3840]); assert_eq!(&output[3840..7680], &[0; 3840]); assert_eq!(&output[7680..], &[2; 3840]);
    }
    #[test] fn capture_boundaries_trim_only_start_and_end() {
        let track = Track::float_stereo(PathBuf::new()); let base = 10_000_000;
        let mut timeline = Timeline::default(); let mut output = vec![];
        let bytes: Vec<u8> = (0..480).flat_map(|i| vec![(i / 240 + 1) as u8; 8]).collect();
        timeline.append(&mut output, &track, packet(0, base - 50_000, 480), Some(&bytes), base, base + 100_000, None).unwrap();
        assert_eq!(output, vec![2; 240 * 8]);
        timeline.append(&mut output, &track, packet(480, base + 50_000, 480), Some(&vec![3; 3840]), base, base + 200_000, Some(base + 100_000)).unwrap();
        assert_eq!(timeline.written, 480); assert_eq!(&output[1920..], &[3; 1920]);
    }
    #[test] fn silent_packet_and_counter_reset_keep_timeline() {
        let track = Track::float_stereo(PathBuf::new()); let base = 10_000_000;
        let mut timeline = Timeline::default(); let mut output = vec![];
        timeline.append(&mut output, &track, packet(1000, base, 480), None, base, base + 1_000_000, None).unwrap();
        let mut reset = packet(0, base + 500_000, 480); reset.discontinuity = true;
        timeline.append(&mut output, &track, reset, Some(&vec![4; 3840]), base, base + 1_000_000, None).unwrap();
        assert_eq!(timeline.written, 2880); assert!(output[..2400 * 8].iter().all(|b| *b == 0)); assert_eq!(&output[2400 * 8..], &[4; 3840]);
    }
    #[test] fn silence_and_padding_are_exact_stereo_frames() { let track = Track::float_stereo(PathBuf::new()); let mut bytes = vec![]; let mut written = 0; pad(&mut bytes, &mut written, 100, &track).unwrap(); pad(&mut bytes, &mut written, 20, &track).unwrap(); assert_eq!(bytes.len(), 800); assert!(bytes.iter().all(|b| *b == 0)); assert_eq!(frames(10_000_000, 44100), 44100); }
    #[test] fn audio_options_off_never_open_a_device() { let group = AudioGroup::prepare(Path::new("not-created"), 0, false, false).unwrap(); assert!(group.tracks.is_empty()); assert!(group.finish().errors.is_empty()); }
    #[test] fn mono_native_format_is_preserved() { let f = WAVEFORMATEX { wFormatTag: 1, nChannels: 1, nSamplesPerSec: 44100, nBlockAlign: 2, wBitsPerSample: 16, ..Default::default() }; let t = describe(Path::new("mic.pcm"), f, None).unwrap(); assert_eq!(t.channels, 1); assert_eq!(t.rate, 44100); assert_eq!(t.format, "s16le"); }
    #[test] #[ignore = "Requires an interactive audio endpoint and explicit local testing"] fn live_system_audio() { let folder = std::env::temp_dir(); let group = AudioGroup::prepare(&folder, std::process::id() as usize, true, false).unwrap(); group.begin(); thread::sleep(Duration::from_millis(500)); let result = group.finish(); assert!(result.errors.is_empty()); assert!(std::fs::metadata(&result.tracks[0].path).unwrap().len() > 0); std::fs::remove_file(&result.tracks[0].path).unwrap(); }
}
