import {
  VERSION,
  SWARM_GLOBAL_KEY,
  SWARM_MARKER,
  LANG_CHANNEL,
  QUERY_LANG_CHANNEL,
  STORAGE_LANG,
} from '../shared/constants.js';

export function mountSwarmRuntime(swarmData, buildOptions = {}) {
  if (typeof window === 'undefined' || typeof document === 'undefined') return null;

  if (
    window.location?.protocol !== 'plugin:' ||
    !/^swarm--swarm_board-[a-z0-9]+$/i.test(window.location.hostname || '') ||
    (window.location.pathname !== '/' && window.location.pathname !== '')
  ) {
    return null;
  }

  const fingerprint =
    window.__AGY_ZH_SWARM_REQUESTED_FINGERPRINT__ ||
    buildOptions.fingerprint ||
    `${VERSION}-swarm-dev`;

  const existing = window[SWARM_GLOBAL_KEY];
  if (existing && existing.document === document && existing.fingerprint === fingerprint) {
    return existing;
  }
  if (existing && typeof existing.dispose === 'function') {
    try { existing.dispose(); } catch (_) { /* ignore */ }
  }
  if (typeof window.__antigravityZhPatchCleanup === 'function') {
    try { window.__antigravityZhPatchCleanup(); } catch (_) { /* ignore */ }
  }

  const exact = swarmData.exact || {};
  const patterns = (swarmData.patterns || []).map((p) => ({
    id: p.id || '',
    parentSelector: p.parentSelector || '',
    regex: new RegExp(p.regex, (p.flags || '').replace(/g/g, '')),
    replacement: p.replacement,
  }));
  const selectors = (swarmData.selectors || []).join(',');
  const attributeSelectors = (swarmData.attributeSelectors || []).join(',');
  const allSelectors = [selectors, attributeSelectors].filter(Boolean).join(',');
  const protectedRoots = (swarmData.protectedRoots || []).join(',');

  const textEdits = new Map();
  const attrEdits = new Map();
  const attrs = ['title', 'aria-label', 'placeholder'];
  const observeOptions = {
    subtree: true,
    childList: true,
    characterData: true,
    attributes: true,
    attributeFilter: attrs.concat(['id', 'class', 'value', 'data-testid']),
  };

  let enabled = true;
  let disposed = false;
  let observer = null;
  let booted = false;

  try {
    if (window.localStorage?.getItem(STORAGE_LANG) === 'en') {
      enabled = false;
    }
  } catch (_) { /* storage unavailable in sandboxed plugin frame */ }

  function eligible(el, isAttribute = false) {
    const selectorGroup = isAttribute ? allSelectors : selectors;
    if (!el || !selectorGroup || !el.matches?.(selectorGroup)) return false;
    if (protectedRoots && el.closest?.(protectedRoots)) return false;
    if (
      el.closest?.('.msg') &&
      !el.matches(
        '#feed > article.msg > button.msg-more[data-testid="swarm-message-more"], #feed > article.msg > .msg-head .msg-note' +
          (isAttribute ? ', #feed > article.msg > .msg-head .msg-author' : '')
      )
    ) {
      return false;
    }
    if (el.closest?.('.o2')) {
      const custom = window.SwarmHive?.viewCopy;
      if (custom?.ui && Object.values(custom.ui).some((v) => v === el.textContent.trim())) {
        return false;
      }
    }
    return true;
  }

  function translateValue(raw, el, isAttribute = false) {
    if (typeof raw !== 'string' || !raw) return raw;
    const key = raw.replace(/\s+/g, ' ').trim();
    if (!key) return raw;
    if (key === '.' && el?.id !== 'empty-body') return raw;

    let target = null;
    if (Object.hasOwn(exact, key)) {
      target = exact[key];
    } else {
      for (let i = 0; i < patterns.length; i++) {
        const p = patterns[i];
        if (p.parentSelector && (isAttribute || !el?.matches?.(p.parentSelector))) continue;
        if (p.regex.test(key)) {
          target = key.replace(p.regex, p.replacement);
          break;
        }
      }
    }
    if (!target) return raw;
    return raw.replace(/\S[\s\S]*\S|\S/, () => target);
  }

  function restoreAll() {
    observer?.disconnect();
    for (const [node, edit] of textEdits) {
      if (node.isConnected && node.nodeValue === edit.localized) {
        node.nodeValue = edit.source;
      }
      textEdits.delete(node);
    }
    for (const [el, edits] of attrEdits) {
      if (el.isConnected) {
        for (const [name, edit] of edits) {
          if (el.getAttribute(name) === edit.localized) {
            el.setAttribute(name, edit.source);
          }
        }
      }
      attrEdits.delete(el);
    }
  }

  function scan() {
    if (disposed || !booted || !document.body) return;
    observer?.disconnect();
    try {
      for (const [node, edit] of textEdits) {
        if (!node.isConnected) {
          textEdits.delete(node);
        } else if (!eligible(node.parentElement, false) || node.nodeValue !== edit.localized) {
          if (node.nodeValue === edit.localized) node.nodeValue = edit.source;
          textEdits.delete(node);
        }
      }
      for (const [el, edits] of attrEdits) {
        if (!el.isConnected) {
          attrEdits.delete(el);
          continue;
        }
        for (const [name, edit] of edits) {
          if (!eligible(el, true) || el.getAttribute(name) !== edit.localized) {
            if (el.getAttribute(name) === edit.localized) el.setAttribute(name, edit.source);
            edits.delete(name);
          }
        }
        if (!edits.size) attrEdits.delete(el);
      }

      if (!enabled || !allSelectors) return;

      for (const el of document.querySelectorAll(allSelectors)) {
        if (!eligible(el, true)) continue;
        if (eligible(el, false)) {
          for (const node of el.childNodes) {
            if (node.nodeType !== 3 || textEdits.has(node)) continue;
            const source = node.nodeValue;
            const localized = translateValue(source, el, false);
            if (localized !== source) {
              textEdits.set(node, { source, localized });
              node.nodeValue = localized;
            }
          }
        }
        for (const name of attrs) {
          const source = el.getAttribute(name);
          if (!source || attrEdits.get(el)?.has(name)) continue;
          const localized = translateValue(source, el, true);
          if (localized !== source) {
            let map = attrEdits.get(el);
            if (!map) attrEdits.set(el, (map = new Map()));
            map.set(name, { source, localized });
            el.setAttribute(name, localized);
          }
        }
      }
    } finally {
      if (!disposed && enabled && observer && document.body) {
        observer.observe(document.body, observeOptions);
      }
    }
  }

  function setEnabled(next) {
    if (disposed) return;
    const boolNext = Boolean(next);
    if (enabled === boolNext && booted) return;
    enabled = boolNext;
    if (!booted) return;
    if (enabled) {
      scan();
    } else {
      restoreAll();
    }
  }

  function onMessage(event) {
    let parentWin = null;
    let topWin = null;
    try { parentWin = window.parent; } catch (_) { /* ignore */ }
    try { topWin = window.top; } catch (_) { /* ignore */ }
    if (event.source !== parentWin && event.source !== topWin) return;
    if (!event.data || event.data.channel !== LANG_CHANNEL) return;
    const origin = event.origin || '';
    const allowedOrigin =
      origin === 'null' ||
      origin === 'file://' ||
      origin === 'vscode-file://vscode-app' ||
      /^https?:\/\/(?:127\.0\.0\.1|localhost|\[::1\])(?::\d{1,5})?$/.test(origin);
    if (!allowedOrigin) return;
    setEnabled(Boolean(event.data.enabled));
  }

  function requestParentLang() {
    const query = { channel: QUERY_LANG_CHANNEL };
    try {
      if (window.parent && window.parent !== window) {
        window.parent.postMessage(query, '*');
      }
      if (window.top && window.top !== window && window.top !== window.parent) {
        window.top.postMessage(query, '*');
      }
    } catch (_) { /* parent unavailable */ }
  }

  function boot() {
    if (disposed || booted) return;
    if (
      !document.querySelector('[data-testid="swarm-tabs"]') ||
      !document.querySelector('[data-testid="swarm-empty-state"]')
    ) {
      return;
    }
    booted = true;
    if (typeof MutationObserver !== 'undefined') {
      observer = new MutationObserver(scan);
    }
    window.addEventListener('message', onMessage);
    if (enabled) scan();
    requestParentLang();
  }

  function dispose() {
    if (disposed) return;
    disposed = true;
    restoreAll();
    observer = null;
    document.removeEventListener('DOMContentLoaded', boot);
    window.removeEventListener('message', onMessage);
    if (window[SWARM_GLOBAL_KEY] === api) {
      delete window[SWARM_GLOBAL_KEY];
    }
  }

  const api = Object.freeze({
    marker: SWARM_MARKER,
    version: VERSION,
    fingerprint,
    document,
    isEnabled: () => enabled,
    setEnabled,
    refresh: scan,
    dispose,
  });

  window[SWARM_GLOBAL_KEY] = api;

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot, { once: true });
  } else {
    boot();
  }

  return api;
}
