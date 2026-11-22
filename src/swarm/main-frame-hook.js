// Template serialized into dist/main.js during static ASAR patching.
// Must remain completely self-contained with zero repository imports at Electron runtime.
export function installSwarmFrames(rendererScript, passkeyScript) {
  const { app, webContents, webFrameMain } = require('electron');
  const seen = new WeakSet();

  function inject(frame) {
    try {
      const url = new URL(frame.url);
      if (
        url.protocol !== 'plugin:' ||
        !/^swarm--swarm_board-[a-z0-9]+$/i.test(url.hostname) ||
        url.port ||
        url.username ||
        url.password ||
        (url.pathname !== '/' && url.pathname !== '')
      ) {
        return;
      }
      // Never log plugin URLs: query strings can contain session authentication tokens.
      Promise.resolve(frame.executeJavaScript(rendererScript)).catch(() => {});
    } catch (_) {
      /* Frame navigation or destruction can race completion. */
    }
  }

  // The native sheet has its own sandboxed renderer without the desktop preload.
  app.on('agy-zh-passkey-sheet', (win, parent) => {
    if (!parent || parent.isDestroyed() || win.isDestroyed()) return;
    const contents = win.webContents;
    let lastState;

    let active = true;
    let timer;
    const cancel = () => { active = false; clearTimeout(timer); };
    contents.once('destroyed', cancel);
    contents.once('did-start-navigation', cancel);
    async function sync() {
      if (!active || parent.isDestroyed()) return;
      try {
        const url = new URL(parent.webContents.getURL());
        if (['http:', 'https:'].includes(url.protocol) && ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname)) {
          const state = await parent.webContents.executeJavaScript(
            "document.querySelector('[data-agy-zh-control]')?.getAttribute('aria-pressed')"
          );
          if (active && state !== lastState && (state === 'true' || state === 'false')) {
            await contents.executeJavaScript(passkeyScript + '(' + state + ')');
            lastState = state;
          }
        }
      } catch (_) { /* The parent or sheet can close during a read. */ }
      if (active) timer = setTimeout(sync, 500);
    }
    sync();
  });

  function watch(contents) {
    if (seen.has(contents)) return;
    seen.add(contents);
    contents.on('did-frame-finish-load', (_event, _isMainFrame, frameProcessId, frameRoutingId) => {
      try {
        const frame = webFrameMain.fromId(frameProcessId, frameRoutingId);
        if (frame) inject(frame);
      } catch (_) {
        /* Frame may already be destroyed. */
      }
    });
  }

  app.on('web-contents-created', (_event, contents) => watch(contents));
  for (const contents of webContents.getAllWebContents()) watch(contents);
}
