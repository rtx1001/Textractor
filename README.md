# Textractor

Textractor is a lightweight Windows tray app that turns text from images, video frames, and otherwise unselectable interfaces into selectable text.

## Download

Get the portable `textractor.exe` from [GitHub Releases](https://github.com/rtx1001/Textractor/releases/latest). No installer is provided. See the Windows and WebView2 requirements below before running it.

## Features

### Capture and tray

- Runs in the background with a system tray icon; closing Settings keeps it running. Exit from the tray menu to quit.
- Customizable global shortcut (`Ctrl+Shift+2` by default), including PrintScreen combinations such as `Ctrl+Shift+PrintScreen`; shortcut availability depends on other apps and Windows reservations.
- Capture from Settings or the tray, with optional Start with Windows.
- Region selection on the monitor under the cursor, with multi-monitor and display-scaling support.
- Freezes the desktop before selection, including ordinary video frames. Capture mode blocks clicks from reaching underlying windows, including at screen edges.
- The selected area stays bright while the surrounding screen is dimmed. A thin pale-orange border and an outside top-left pixel-dimensions badge identify the capture during and after selection.
- A fresh capture clears the previous selection. Selecting a region does not immediately run OCR.
- Compact icon-only toolbar near the selection's bottom edge; text results expand beneath it. Click outside the selected region/panel to dismiss capture.

### Text recognition and results

- Extract text from images, videos, scanned content, and otherwise unselectable interfaces using the scan-text button.
- Windows English OCR plus optional, individually downloadable offline recognition languages. Choose languages in Settings → OCR instead of downloading every model at startup.
- Image preprocessing improves small-screen text recognition; layout reconstruction preserves visual rows, spacing, and columns where possible, with contextual correction for some digit/letter ambiguities.
- Result display grows to fit the available screen space. A custom themed scrollbar appears only when content exceeds that space.
- Adjustable extracted-text/translation font size in Settings, independent of annotation size or the captured image.
- Copy all displayed text in one click, or enable automatic copying after recognition.
- OCR remains an approximation: punctuation, numbers, fonts, mixed languages, and complex layouts may still require review. Protected video content cannot be guaranteed to capture.

### Drawing and typed annotations

- Straight line, freehand pen, rectangle, circle/oval, arrow, and typed-text tools.
- Tools toggle on/off; selecting another tool turns the previous one off.
- Mouse wheel adjusts line/shape thickness, arrow size (including its shaft thickness), or typed-text size. A circle and pixel-size label preview the current size.
- Rounded triangular arrowheads.
- Compact color wheel with brightness and alpha/opacity sliders; the color button shows the selected color.
- Screen-color eyedropper beside the color button: hover to preview the original frozen-screen pixel, then click to select its color and automatically copy its `#RRGGBB` HEX code. The sampled-code strip lets you copy it again. `Esc` cancels picking without closing the capture.
- Typed text wraps onto new lines and the field expands downward without scrollbars. If available space runs out, text progressively shrinks to fit.
- Drag a text-field edge to move it; drag its bottom-right square to resize it. Click outside the field to commit its contents.
- Undo button and `Ctrl+Z` remove the latest annotation.
- `Esc` first commits/cancels the current editing interaction and turns off an active tool; when no tool is active, `Esc` closes capture.
- Tool color, opacity, thickness, arrow size, and preferred text size persist between sessions.

### Clipboard and PNG export

- Copy the captured region as an image without needing OCR.
- Save the captured image as PNG to a chosen location.
- Image copy and PNG export include drawing and text annotations.
- The retained region outline makes it clear which area is being copied or saved.

### Offline translation

- Always-available Translate button after selection: extracts text first if needed, then translates inside the same result panel. No Google Translate browser window.
- Source and target language settings, including automatic source detection for installed languages.
- User-selected directional language packs; the Install language pack button appears when the selected route is missing. No redundant pack dropdown.
- Direct translation or a two-pack route through English, reusing already-installed packs.
- Shared CPU translation engine downloads with the first pack; fresh startup does not download models. Missing packs produce a setup message instead of a silent download.
- Original/Translation switch retains the OCR result; Copy text copies the currently displayed view.
- Captured text is processed locally, never sent to an online translation service. Internet is needed only for catalogue updates and optional downloads.

### Settings and appearance

- Compact General, Tools, OCR, and Translate tabs with persistent per-user configuration.
- Pale-orange accent (`#F2BE8D`), dark warm-brown surfaces, and matching executable/tray icon.
- Readable setting labels, custom searchable language menus, themed controls, and explanatory tooltips.
- Keyboard navigation and Escape dismissal for custom menus. App-owned controls avoid native dropdowns, color pickers, default tooltips, and Windows scrollbars.
- Standard Windows tray menus, file-save dialogs, and emergency startup messages still use system UI.

## Development

Development requirements: Windows 10/11 x64, Node.js, Rust, and WebView2.

```powershell
npm install
npm run tauri dev
```

Create the portable executable only (no installer) with:

```powershell
npm run tauri build -- --no-bundle
```

Build artifacts are written under `src-tauri/target/release`.

## Portable PC requirements

Copy just `textractor.exe` to a Windows 10/11 x64 (Intel/AMD) PC and run it. There is no Textractor installer and no dependency on Node, Rust, Visual Studio, or a system Python installation. The main executable statically links its C runtime. Optional translation uses a private Python runtime and app-local Microsoft Visual C++ DLLs, not a system-wide runtime installation.

Microsoft Edge WebView2 Runtime is required. It is normally present on Windows 11 and most Windows 10 PCs, but cannot be assumed on every clean or managed machine. If absent, Textractor shows a native startup message explaining how to install Microsoft's Evergreen Runtime. This dependency cannot be eliminated by shrinking the EXE; bundling a full browser runtime would make the portable download much larger.

If Windows OCR is unavailable, select **English · offline pack** (or your language) in Settings → OCR and install it once. Downloads require internet; installed OCR/translation packs then work offline. A previous translation engine installation may show **Repair engine** to add the private C++ libraries without removing existing packs.

Configuration and packs are per-user in AppData, so running from USB/read-only folders does not require writing alongside the EXE. They are not automatically carried to another PC with the EXE. Startup registration is opt-in; keep the EXE at a stable location if using Start with Windows. Capture shortcuts that conflict on another PC no longer prevent the app from starting.

The executable is unsigned. Windows may display an unknown-publisher/SmartScreen warning depending on its policies. No security settings are changed by Textractor.

Run `textractor.exe --portable-check` with stdout redirected to inspect its version, WebView2/OCR availability, and configuration location without opening the UI. This does not install anything or download models. `--portable-smoke --hidden` also creates WebView2, waits for the embedded React interface to call the backend, emits a JSON success report, and exits; it does not register a capture shortcut or change Windows startup registration.

## How to use

1. Start Textractor and close its settings window if you want it to stay in the tray.
2. Press the configured shortcut.
3. Drag around text on the screen. Press `Esc` to cancel.
4. Click Extract text, or Translate to extract and translate in one step. Image copy/save does not require OCR.
5. The copy button copies the currently displayed original or translation. Use Original/Translation to switch views.

## Offline language downloads

In Settings → OCR, choose and install recognition languages. These are separate from translation packs.

In Settings → Translate:

1. The Translate tab loads the small language catalogue as needed (no models download). The refresh icon updates it.
2. Choose the source language, or automatic detection, and the target language.
3. If the selected pair needs a pack, click **Install language pack**. The app chooses the direct pack or both legs through English and downloads only missing packs. Already-ready pairs show **Ready offline** instead of an install button. There is no separate pack picker.

Automatic source detection uses installed source languages. With a non-English target, the install action prepares English → target; choose a specific source to add its own missing route. Same-language pairs need no translation engine. Settings use custom searchable menus, app-styled tooltips/color controls, and custom scrollbars only on overflowing content; the window has its own draggable header and tray-preserving close button.

The first translation install also downloads a private CPU engine. No system Python, Python installer, GPU, or administrative privileges are needed. The engine occupies approximately 150 MB installed. Example installed packs: Vietnamese → English ~80 MB; Japanese → English ~135 MB. Download and installed sizes differ. Nothing is bundled into the portable executable or downloaded at fresh startup.

Downloads come from Python.org, PyPI, and the official Argos model index/server. Wheel downloads are checked against PyPI SHA-256 digests; archives reject paths outside their destination. Runtime and package licenses remain in their installed directories. Microsoft's release-mode C++ redistributable libraries are carried in a compressed private runtime archive; see `src-tauri/VC-RUNTIME-NOTICE.txt`.

Runtime and translation packs are stored under `%LOCALAPPDATA%\Textractor\translation`. Settings persist under `%APPDATA%\Textractor\config.json`. OCR packs use the WebView's local model cache.

Translation never sends captured text online and never opens a browser. Once installed, packs work offline. A missing pack shows a setup message instead of silently downloading it. If no direct pair exists, translation can route through English using both installed legs (for example Japanese → English plus English → Vietnamese).

Automatic detection can be unreliable for short or mixed-language captures; choose a source explicitly when necessary. Translation is CPU-based and takes a short processing delay. OCR layout/newlines remain available in the Original view; translated sentences may naturally change length and spacing.

## Verification

Run `node testdata/translation-plan-tests.mjs` after `npm install` to verify direct/bridged translation installation plans.

`cargo test --lib` from `src-tauri` verifies settings compatibility and translation behavior. Native OCR tests run when the local `testdata/ocr-sample.png`, `ocr-small-sample.png`, and `ocr-explorer.png` fixtures are present; these private development screenshots are intentionally not published, and the corresponding tests skip without them.
`testdata/translation-tests.py` checks archive safety, missing-pack behavior, and real Japanese/Vietnamese offline output using the isolated developer fixtures. Developer model/runtime downloads under `testdata` are not included in release builds.

`testdata/inspect-portable.ps1` inspects PE DLL imports without executing binaries. The release uses size optimization, whole-program link-time optimization, symbol stripping, a static MSVC runtime, and only PNG/BMP image support. Keep PNG screenshot/clipboard and capture/translation regression tests passing when changing these settings.
