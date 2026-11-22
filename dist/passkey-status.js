(enabled => (function updatePasskeyStatus(enabled, original, translation) {
  if (location.protocol !== 'data:' || document.title !== 'Passkeys') return;
  const status = document.querySelector('#sheet-card > .sheet-content .mechanism-text > p#status.mechanism-desc');
  if (!status || !document.querySelector('#sheet-card .qr-centered-view > svg[aria-label="Passkey QR code"]')) return;

  const key = Symbol.for('agy-zh.passkey-status');
  const prior = status[key];
  if (prior?.enabled === enabled) return;
  prior?.observer?.disconnect();
  function render() {
    const node = status.firstChild;
    if (node?.nodeType !== 3 || node.nextSibling) return;
    if (node.data === (enabled ? original : translation)) node.data = enabled ? translation : original;
  }
  const observer = enabled ? new MutationObserver(render) : null;
  observer?.observe(status, { childList: true, characterData: true, subtree: true });
  status[key] = { enabled, observer };
  render();
})(enabled, "Connecting to your device…", "正在连接到你的设备…"))