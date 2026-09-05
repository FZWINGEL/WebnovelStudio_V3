// Actual Tauri/WebView2 integration checks. No Chromium browser is launched.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:net';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const output = resolve(root, '.local/native-results');
await mkdir(output, { recursive: true });
const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-native-'));
const server = createServer();
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const port = server.address().port;
await new Promise(resolve => server.close(resolve));
const app = spawn(resolve(root, 'target/debug/webnovel-desktop.exe'), [], {
  cwd: root, windowsHide: true, stdio: 'pipe',
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`, WNS_V3_TRIAL_WEBVIEW_DIR: data },
});
let appLog = '';
app.stdout.on('data', chunk => { appLog += chunk; });
app.stderr.on('data', chunk => { appLog += chunk; });
app.on('error', error => { appLog += error.stack; });
let browser;
const checks = [];
try {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (app.exitCode !== null) throw new Error(`Native app exited: ${appLog}`);
    try { const response = await fetch(`http://127.0.0.1:${port}/json/version`); if (response.ok) break; } catch {}
    await new Promise(resolve => setTimeout(resolve, 200));
  }
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 10000 });
  const context = browser.contexts()[0];
  let page = context.pages()[0];
  if (!page) page = await context.waitForEvent('page', { timeout: 10000 });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.getByRole('textbox', { name: 'Chapter manuscript' }).waitFor();
  await page.getByText(/Tauri · WebView2/).waitFor();
  assert.equal(new URL(page.url()).hostname, 'tauri.localhost');
  const runtime = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('runtime_info'));
  assert.equal(runtime.host, 'Tauri');
  assert.equal(runtime.persistence, false);
  checks.push(`Real Tauri IPC, WebView2 ${runtime.webviewVersion}`);
  await page.getByRole('button', { name: 'Check with Rust', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Fingerprints match' }).waitFor();
  checks.push('Editor/Rust canonical JSON and SHA-256 match over real IPC');
  await page.screenshot({ path: resolve(output, 'desktop.png') });

  // An immutable Tiptap instance is exposed on its DOM element by Tiptap itself.
  const before = await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    window.nativeTrialEditor = editor;
    return editor.getJSON();
  });
  await page.evaluate(() => document.querySelector('.tiptap').editor.commands.setTextSelection({ from: 1, to: 8 }));
  await page.getByRole('button', { name: 'Selection feedback', exact: true }).click();
  await page.getByRole('textbox', { name: 'Your feedback on this passage' }).waitFor();
  assert.equal(await page.locator('.quoted-scope blockquote').textContent(), 'By dusk');
  assert.equal(await page.evaluate(() => document.activeElement.id), 'feedback-input');
  await page.getByRole('textbox', { name: 'Your feedback on this passage' }).fill('Make this opening more immediate.');
  await page.getByRole('button', { name: 'Keep feedback', exact: true }).click();
  assert(await page.evaluate(() => document.querySelector('.tiptap').editor === window.nativeTrialEditor));
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), before);
  checks.push('Selection quote and composer focus survive chat updates without replacing the editor');

  await page.getByRole('button', { name: 'Try your own replacement' }).click();
  await page.getByRole('textbox', { name: 'Replacement text' }).fill('At nightfall');
  await page.getByRole('button', { name: 'Preview replacement' }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), before);
  await page.getByRole('button', { name: 'Reject', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), before);
  await page.getByRole('button', { name: 'Preview replacement' }).click();
  await page.locator('.replacement-preview').scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(output, 'selection-preview.png') });
  await page.getByRole('button', { name: 'Apply replacement' }).click();
  await page.getByRole('status').filter({ hasText: 'Replacement applied' }).waitFor();
  const after = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  assert.equal(after.content[0].content[0].text, 'At nightfall, every lantern in the harbour had gone dark. All but one.');
  assert.deepEqual(after.content.slice(1), before.content.slice(1));
  assert.equal(after.content[0].attrs.id, before.content[0].attrs.id);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), before);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON()), after);
  checks.push('Preview/Reject do not mutate; strict local Apply, undo and redo preserve surrounding nodes');

  const manuscript = page.getByRole('textbox', { name: 'Chapter manuscript' });
  await manuscript.click();
  await page.keyboard.press('ArrowRight');
  const focusStyle = await manuscript.evaluate(element => ({ style: getComputedStyle(element).outlineStyle, width: getComputedStyle(element).outlineWidth }));
  assert.equal(focusStyle.style, 'solid');
  assert.equal(focusStyle.width, '2px');
  await page.screenshot({ path: resolve(output, 'editor-focused.png') });
  checks.push('Manuscript keyboard focus is visibly indicated in the native window');
  await page.keyboard.press('Control+End');
  await page.keyboard.press('Enter');
  await page.keyboard.insertText('灯火🙂 灯火🙂 e\u0301 👩‍🚀');
  assert((await manuscript.textContent()).includes('灯火🙂 灯火🙂 e\u0301 👩‍🚀'));
  await page.getByRole('button', { name: 'Check with Rust', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Fingerprints match' }).waitFor();
  checks.push('Synthetic Unicode edge cases survive native input: non-Latin text, emoji, combining marks and ZWJ');
  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setTextSelection({ from: 1, to: 8 });
    editor.commands.focus();
  });
  await page.waitForFunction(() => document.activeElement.classList.contains('tiptap'));
  await page.keyboard.press('Control+Shift+f');
  await page.getByRole('textbox', { name: 'Your feedback on this passage' }).waitFor();
  assert.equal(await page.evaluate(() => document.activeElement.id), 'feedback-input');
  checks.push('Ctrl+Shift+F captures selection and transfers focus');

  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.insertContentAt(1, 'Changed ');
  });
  await page.getByText('The chapter changed. This quotation is kept as a reference.', { exact: false }).waitFor();
  assert(await page.getByRole('button', { name: 'Try your own replacement' }).isDisabled());
  checks.push('Intervening manuscript edits make captured replacement scope stale');

  await page.getByRole('button', { name: 'Whole chapter', exact: true }).click();
  const clipboardSource = await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setContent({ type: 'doc', content: [
      { type: 'paragraph', attrs: { id: 'clipboard-left' }, content: [{ type: 'text', text: 'Clipboard 灯火🙂', marks: [{ type: 'bold' }] }] },
      { type: 'paragraph', attrs: { id: 'clipboard-right' }, content: [{ type: 'text', text: '尾声', marks: [{ type: 'italic' }] }] },
      { type: 'paragraph', attrs: { id: 'clipboard-destination' }, content: [{ type: 'text', text: 'Destination ' }] },
    ] });
    editor.commands.setTextSelection({ from: 1, to: editor.state.doc.child(0).nodeSize + 1 + editor.state.doc.child(1).content.size });
    editor.commands.focus();
    return editor.getJSON();
  });
  await page.waitForFunction(() => document.activeElement.classList.contains('tiptap'));
  await page.keyboard.press('Control+c');
  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
  });
  await page.keyboard.press('Control+v');
  await page.waitForFunction(() => document.querySelector('.tiptap').editor.state.doc.textContent.split('Clipboard 灯火🙂').length === 3);
  const pasted = await page.evaluate(() => document.querySelector('.tiptap').editor.getJSON());
  assert.deepEqual(pasted.content.slice(0, 2), clipboardSource.content.slice(0, 2));
  assert.equal(new Set(pasted.content.map(block => block.attrs.id)).size, pasted.content.length);
  assert(pasted.content.slice(2).some(block => block.content?.some(node => node.text?.includes('Clipboard 灯火🙂') && node.marks?.some(mark => mark.type === 'bold'))));
  assert(pasted.content.slice(2).some(block => block.content?.some(node => node.text?.includes('尾声') && node.marks?.some(mark => mark.type === 'italic'))));
  await page.getByRole('button', { name: 'Check with Rust', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Fingerprints match' }).waitFor();
  checks.push('Native WebView2 Ctrl+C/Ctrl+V preserves formatted Unicode paragraphs and unique IDs');
  await page.evaluate(() => {
    const editor = document.querySelector('.tiptap').editor;
    editor.commands.setTextSelection({ from: 1, to: 10 });
    editor.commands.focus();
  });
  await page.waitForFunction(() => document.activeElement.classList.contains('tiptap'));
  await page.locator('.tiptap p').first().click({ button: 'right', position: { x: 20, y: 10 } });
  await page.getByRole('menuitem', { name: 'Give feedback on selection' }).click();
  await page.getByRole('textbox', { name: 'Your feedback on this passage' }).waitFor();
  assert.equal(await page.locator('.quoted-scope blockquote').textContent(), 'Clipboard');
  checks.push('Right-click selection menu captures the intended passage');
  assert.deepEqual(errors, []);
  await writeFile(resolve(output, 'report.json'), JSON.stringify({ date: new Date().toISOString(), runtime, url: page.url(), authoringLanguage: 'English', checks, errors, executable: 'target/debug/webnovel-desktop.exe', limitations: ['No disk-backed manuscript persistence', 'No physical keyboard/dead-key author trial', 'No screen-reader user trial', 'No minimum-window-size or multi-DPI qualification', 'No provider or durable Apply'], dataDirectory: data }, null, 2));
  console.log(JSON.stringify({ passed: checks.length, checks, output }, null, 2));
} catch (error) {
  await writeFile(resolve(output, 'failure.txt'), `${error.stack}\n${appLog}`);
  throw error;
} finally {
  await browser?.close();
  if (app.exitCode === null) app.kill();
}
