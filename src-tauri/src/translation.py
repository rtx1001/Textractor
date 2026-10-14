"""App-private CPU translation runtime. Networking is limited to explicit pack setup."""
import hashlib
import io
import importlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tarfile
import urllib.request
import zipfile

INDEX_URL = 'https://raw.githubusercontent.com/argosopentech/argospm-index/main/index.json'

def download(url):
    if not url.startswith(('https://pypi.org/', 'https://files.pythonhosted.org/',
                           'https://raw.githubusercontent.com/argosopentech/', 'https://argos-net.com/')):
        raise ValueError('Untrusted download location')
    request = urllib.request.Request(url, headers={'User-Agent': 'Textractor/0.3.1'})
    with urllib.request.urlopen(request, timeout=180) as response:
        return response.read()

def unzip(data, destination):
    destination = destination.resolve()
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for entry in archive.infolist():
            path = (destination / entry.filename).resolve()
            if not path.is_relative_to(destination) or (entry.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError('Unsafe archive entry')
        archive.extractall(destination)

def setup(root):
    site = root / 'runtime' / 'Lib' / 'site-packages'
    site.mkdir(parents=True, exist_ok=True)
    # Fixed compatible wheels; no pip, external Python, PyTorch, GPU, or system changes.
    versions = {'numpy': '2.2.6', 'PyYAML': '6.0.2', 'ctranslate2': '4.6.0',
                'sentencepiece': '0.2.1', 'sacremoses': '0.1.1', 'subword-nmt': '0.3.8',
                'regex': '2024.11.6', 'click': '8.1.8', 'joblib': '1.4.2',
                'tqdm': '4.67.1', 'colorama': '0.4.6', 'langid': '1.1.6', 'setuptools': '75.8.0'}
    for name, version in versions.items():
        stamp = site / ('.' + name + '-' + version)
        if stamp.exists():
            continue
        info = json.loads(download(f'https://pypi.org/pypi/{name}/{version}/json'))
        files = info['urls']
        wheel = next((f for f in files if f['filename'].endswith(
            ('cp312-cp312-win_amd64.whl', 'cp312-none-win_amd64.whl', 'py3-none-any.whl', 'py2.py3-none-any.whl'))), None)
        if name == 'langid':
            wheel = next(f for f in files if f['packagetype'] == 'sdist')
        if not wheel:
            raise ValueError(f'No compatible Windows runtime for {name}')
        data = download(wheel['url'])
        if hashlib.sha256(data).hexdigest() != wheel['digests']['sha256']:
            raise ValueError('Download integrity check failed')
        if name == 'langid':
            with tarfile.open(fileobj=io.BytesIO(data)) as archive:
                # Copy only Python package resources, never setup scripts.
                for entry in archive.getmembers():
                    parts = Path(entry.name).parts
                    if len(parts) >= 3 and parts[1] == 'langid' and entry.isfile():
                        target = site.joinpath(*parts[1:]).resolve()
                        if not target.is_relative_to(site.resolve()):
                            raise ValueError('Unsafe package path')
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(archive.extractfile(entry).read())
        else:
            unzip(data, site)
        stamp.touch()
    importlib.invalidate_caches()
    if str(site) not in sys.path:
        sys.path.insert(0, str(site))
    import ctranslate2, sentencepiece, langid
    if not (root / 'runtime' / 'msvcp140.dll').exists():
        raise ValueError('Private Visual C++ runtime missing. Repair engine in Settings → Translate.')
    (root / 'runtime-ready').write_text('2', encoding='utf-8')
    return {'ready': True}

def catalogue(root, refresh=False):
    cache = root / 'index.json'
    if refresh:
        data = download(INDEX_URL)
        json.loads(data)
        cache.write_bytes(data)
    if not cache.exists():
        return []
    return json.loads(cache.read_text(encoding='utf-8'))

def pack_id(source, target):
    if not re.fullmatch('[a-z]{2,3}', source) or not re.fullmatch('[a-z]{2,3}', target):
        raise ValueError('Invalid language code')
    return source + '_' + target

def status(root, refresh=False):
    packs = root / 'packs'
    result = []
    for item in catalogue(root, refresh):
        key = pack_id(item['from_code'], item['to_code'])
        result.append({'id': key, 'source': item['from_code'], 'target': item['to_code'],
                       'label': item['from_name'] + ' → ' + item['to_name'],
                       'installed': (packs / key / 'ready').exists()})
    marker = root / 'runtime-ready'
    return {'ready': marker.exists() and marker.read_text(encoding='utf-8').strip() == '2' and (root / 'runtime' / 'msvcp140.dll').exists(), 'packs': result}

def install(root, source, target):
    key = pack_id(source, target)
    item = next((p for p in catalogue(root, True) if p['from_code'] == source and p['to_code'] == target), None)
    if not item:
        raise ValueError('No direct pack for this language pair')
    data = download(next(url for url in item['links'] if url.startswith('https://argos-net.com/')))
    stage = root / 'packs' / (key + '.install')
    stage.mkdir(parents=True, exist_ok=True)
    try:
        unzip(data, stage)
        metadata = list(stage.rglob('metadata.json'))
        if len(metadata) != 1:
            raise ValueError('Invalid language package')
        model = metadata[0].parent
        meta = json.loads(metadata[0].read_text(encoding='utf-8'))
        if meta['from_code'] != source or meta['to_code'] != target or not (model / 'model' / 'model.bin').exists():
            raise ValueError('Language package does not match requested pair')
        if not (model / 'sentencepiece.model').exists() and not (model / 'bpe.model').exists():
            raise ValueError('Unsupported package tokenizer')
        destination = root / 'packs' / key
        if destination.exists():
            raise ValueError('Pack already installed; remove it before reinstalling')
        shutil.move(str(model), destination)
        (destination / 'ready').touch()
    finally:
        shutil.rmtree(stage)
    return status(root)

def translate_pair(root, source, target, text):
    import ctranslate2
    path = root / 'packs' / pack_id(source, target)
    if not (path / 'ready').exists():
        raise ValueError(f'Install {source} → {target} in Settings → Translate first.')
    if (path / 'sentencepiece.model').exists():
        import sentencepiece
        tokenizer = sentencepiece.SentencePieceProcessor(model_file=str(path / 'sentencepiece.model'))
        encode = lambda s: tokenizer.encode(s, out_type=str)
        decode = lambda tokens: tokenizer.decode_pieces(tokens).replace('▁', ' ').strip()
    else:
        from sacremoses import MosesTokenizer, MosesDetokenizer, MosesPunctNormalizer
        from subword_nmt.apply_bpe import BPE
        with (path / 'bpe.model').open(encoding='utf-8') as f:
            bpe = BPE(f)
        normalizer, tokenizer, detokenizer = MosesPunctNormalizer(source), MosesTokenizer(source), MosesDetokenizer(target)
        encode = lambda s: bpe.segment_tokens(tokenizer.tokenize(normalizer.normalize(s)))
        decode = lambda tokens: detokenizer.detokenize(' '.join(tokens).replace('@@ ', '').split())
    translator = ctranslate2.Translator(str(path / 'model'), device='cpu', compute_type='int8', intra_threads=max(1, min(4, os.cpu_count() or 1)))
    output = []
    for line in text.split('\n'):
        if not line.strip():
            output.append('')
            continue
        # Bound long lines without dropping content at the model's token limit.
        chunks = re.split(r'(?<=[.!?。！？])\s+', line.strip())
        tokens = [encode(chunk) for chunk in chunks if chunk]
        tokens = [seq[i:i+180] for seq in tokens for i in range(0, len(seq), 180)]
        results = translator.translate_batch(tokens, beam_size=4, max_decoding_length=512)
        output.append(' '.join(decode(r.hypotheses[0]) for r in results))
    return '\n'.join(output)

def translate(root, request):
    text, source, target = request['text'], request['source'], request['target']
    if not text.strip():
        raise ValueError('No text detected to translate.')
    if len(text) > 20000:
        raise ValueError('Select a smaller region (maximum 20,000 characters).')
    if source == 'auto':
        import langid
        source = langid.classify(text)[0]
        if re.search('[\u3040-\u30ff]', text):
            source = 'ja'
        elif source == 'zh' and (root / 'packs' / ('zt_' + target) / 'ready').exists() and not (root / 'packs' / ('zh_' + target) / 'ready').exists():
            source = 'zt'
    if source == target:
        return {'text': text, 'source': source, 'target': target}
    if (root / 'packs' / pack_id(source, target) / 'ready').exists():
        result = translate_pair(root, source, target, text)
    elif source != 'en' and target != 'en':
        result = translate_pair(root, 'en', target, translate_pair(root, source, 'en', text))
    else:
        result = translate_pair(root, source, target, text)
    return {'text': result, 'source': source, 'target': target}

def main():
    command, root = sys.argv[1], Path(sys.argv[2]).resolve()
    root.mkdir(parents=True, exist_ok=True)
    if command == 'setup':
        return setup(root)
    if command in ('status', 'catalogue'):
        return status(root, command == 'catalogue')
    request = json.load(sys.stdin)
    if command == 'install':
        return install(root, request['source'], request['target'])
    if command == 'remove':
        path = root / 'packs' / pack_id(request['source'], request['target'])
        if path.exists():
            shutil.rmtree(path)
        return status(root)
    if command == 'translate':
        return translate(root, request)
    raise ValueError('Unknown translation command')

if __name__ == '__main__':
    try:
        print(json.dumps(main(), ensure_ascii=True))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
