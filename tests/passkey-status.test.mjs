import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { JSDOM } from 'jsdom';

const render = fs.readFileSync('dist/passkey-status.js', 'utf8');
const html = '<title>Passkeys</title><div id="sheet-card"><div class="sheet-content"><div class="mechanism-text"><p id="status" class="mechanism-desc">Scan for example.invalid</p></div><div class="qr-centered-view"><svg aria-label="Passkey QR code"></svg></div></div></div>';
const en = 'Connecting to your device…';
const zh = '正在连接到你的设备…';

test('Passkeys sheet follows language and late status updates without changing adjacent content', async () => {
  const dom = new JSDOM(html, { url: 'data:text/html,fixture', runScripts: 'outside-only' });
  try {
    const win = dom.window;
    const setLanguage = win.eval(render);
    const status = win.document.querySelector('#status');
    const qr = win.document.querySelector('svg');
    setLanguage(true);
    assert.equal(status.textContent, 'Scan for example.invalid');
    status.textContent = en;
    await Promise.resolve();
    assert.equal(status.textContent, zh);
    setLanguage(false);
    assert.equal(status.textContent, en);
    setLanguage(true);
    assert.equal(status.textContent, zh);
    status.firstChild.data = 'Verify on your device to continue…';
    await Promise.resolve();
    setLanguage(false);
    assert.equal(status.textContent, 'Verify on your device to continue…');
    assert.equal(win.document.querySelector('svg'), qr);
  } finally { dom.window.close(); }
});

test('same text in an unrelated document remains unchanged', () => {
  const dom = new JSDOM(html.replace('Scan for example.invalid', en), {
    url: 'https://example.invalid/', runScripts: 'outside-only',
  });
  try {
    dom.window.eval(render)(true);
    assert.equal(dom.window.document.querySelector('#status').textContent, en);
  } finally { dom.window.close(); }
});
