import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { createRequire } from 'node:module';
const result = await build({
  stdin: { contents: `import React from 'react'; import { renderToStaticMarkup } from 'react-dom/server'; import { Settings } from './src/screens/Settings'; import { RecorderSettings } from './src/screens/RecorderSettings'; import { RecordingControls } from './src/screens/RecordingControls'; export function render() { const calls = []; const api = async command => { calls.push(command); }; return {settings: renderToStaticMarkup(<Settings api={api}/>), recorder: renderToStaticMarkup(<RecorderSettings api={api}/>), controls: renderToStaticMarkup(<RecordingControls api={api}/>), calls}; }`, resolveDir: process.cwd(), loader: 'tsx' },
  bundle: true, write: false, platform: 'node', format: 'cjs', packages: 'external', jsx: 'automatic',
});
const compiled = { exports: {} };
new Function('require', 'module', 'exports', result.outputFiles[0].text)(createRequire(import.meta.url), compiled, compiled.exports);
const { render } = compiled.exports;
const { settings, recorder, controls, calls } = render();
assert.equal((settings.match(/role="tab"/g) || []).length, 5);
assert.match(settings, /id="recorder-tab"/);
assert.match(recorder, /Download FFmpeg/);
assert.match(recorder, /115 MB/);
assert.match(recorder, /no download at startup/);
assert.match(recorder, /optional system sound and microphone/);
assert.match(controls, /aria-label="Save recording"/);
assert.match(controls, /aria-label="Cancel recording"/);
assert.match(controls, /aria-label="Hide recording controls"/);
assert.equal((controls.match(/<button /g) || []).length, 3);
assert.doesNotMatch(controls, /Keep recording locally|Archive/);
assert.doesNotMatch(settings + recorder + controls, /<select|type="color"| title=/);
assert.deepEqual(calls, [], 'Rendering must not download FFmpeg or start recording');
console.log('11 recorder UI markup and no-auto-download assertions passed.');
