// Template serialized into dist/main.js during static ASAR patching.
// Must remain completely self-contained with zero repository imports at Electron runtime.
export function installSwarmFrames(rendererScript) {
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
