import {
  VERSION,
  GLOBAL_KEY,
  LEGACY_KEY,
  CUSTOM_DICT_KEY,
  MODE_KEY,
  RUNTIME_MARKER,
} from '../shared/constants.js';
import { createTranslator } from '../shared/translator.js';
import { createScanner } from './scanner.js';
import { createMenuTranslator } from './menu.js';
import { createFloatingBall } from './floating-ball.js';

export function mountDesktopRuntime(bundleData, buildOptions = {}) {
  if (typeof window === 'undefined' || typeof document === 'undefined') return null;

  // Never touch sign-in sites or external origins; only localhost / loopback / data: pages.
  const isAllowedOrigin =
    ['127.0.0.1', 'localhost', '[::1]'].includes(window.location.hostname) ||
    window.location.protocol === 'data:';
  if (!isAllowedOrigin) return null;

  const fingerprint =
    window.__AGY_ZH_REQUESTED_FINGERPRINT__ ||
    buildOptions.fingerprint ||
    `${VERSION}-dev`;

  // Clean up legacy v2 global instance if present
  const hadPrevious = Boolean(
    (window[LEGACY_KEY] && typeof window[LEGACY_KEY].dispose === 'function') ||
    (window[GLOBAL_KEY] && typeof window[GLOBAL_KEY].dispose === 'function')
  );
  if (window[LEGACY_KEY] && typeof window[LEGACY_KEY].dispose === 'function') {
    try { window[LEGACY_KEY].dispose(); } catch (_) { /* ignore */ }
  }

  // Idempotency & hot-reload protection
  const existing = window[GLOBAL_KEY];
  if (existing && existing.document === document && existing.fingerprint === fingerprint) {
    return existing;
  }
  if (existing && typeof existing.dispose === 'function') {
    try { existing.dispose(); } catch (_) { /* ignore */ }
  }

  let disposed = false;
  let enabled = true;
  let lastCustomRef = window[CUSTOM_DICT_KEY];

  const translator = createTranslator({
    dictionary: bundleData.exact || {},
    patterns: bundleData.patterns || [],
    customEntries: Array.isArray(lastCustomRef) ? lastCustomRef : [],
  });

  const scanner = createScanner(translator, {
    isEnabled: () => enabled,
    isDisposed: () => disposed,
  });

  const translateMenu = createMenuTranslator(translator, () => enabled && !disposed);

  const mode = window[MODE_KEY] === 'cdp' ? 'cdp' : 'static';

  function syncCustomTranslationsIfChanged() {
    const currentCustom = window[CUSTOM_DICT_KEY];
    if (currentCustom !== lastCustomRef) {
      lastCustomRef = currentCustom;
      translator.setCustomTranslations(Array.isArray(currentCustom) ? currentCustom : []);
      scanner.restore();
      return true;
    }
    return false;
  }

  function setEnabled(value, opts = {}) {
    if (disposed) return;
    enabled = Boolean(value);
    if (enabled) {
      syncCustomTranslationsIfChanged();
      scanner.scan();
      scanner.observe();
    } else {
      scanner.restore();
    }
    floatingBall.syncState({
      persistLang: opts.persist === false ? undefined : enabled,
    });
  }

  function setKeepEnglishLabels(value, opts = {}) {
    if (disposed) return;
    const changed = translator.setKeepEnglishLabels(value);
    if (changed) {
      scanner.restore();
      if (enabled) {
        scanner.scan();
        scanner.observe();
      }
    }
    floatingBall.syncState({
      persistKeepEn: opts.persist === false ? undefined : Boolean(value),
    });
  }

  const floatingBall = createFloatingBall({
    mode,
    getEnabled: () => enabled,
    onToggleEnabled: (next, opts) => setEnabled(next, opts),
    getKeepEnglishLabels: () => translator.getKeepEnglishLabels(),
    onToggleKeepEnglishLabels: (next, opts) => setKeepEnglishLabels(next, opts),
  });

  function refresh() {
    if (disposed) return;
    syncCustomTranslationsIfChanged();
    if (enabled) {
      scanner.scan();
      scanner.observe();
    }
  }

  function init() {
    if (disposed || !document.body) return;
    const prefs = floatingBall.readInitialPrefs();
    enabled = prefs.enabled;
    translator.setKeepEnglishLabels(prefs.keepEnglishLabels);
    scanner.startObserver();
    floatingBall.mount();
    setEnabled(enabled, { persist: false });
  }

  function dispose() {
    if (disposed) return;
    disposed = true;
    scanner.dispose();
    floatingBall.unmount();
    document.removeEventListener('DOMContentLoaded', init);
    if (window[GLOBAL_KEY] === api) {
      delete window[GLOBAL_KEY];
    }
  }

  const api = Object.freeze({
    marker: RUNTIME_MARKER,
    version: VERSION,
    fingerprint,
    mode,
    document,
    isEnabled: () => enabled,
    setEnabled,
    getKeepEnglishLabels: () => translator.getKeepEnglishLabels(),
    setKeepEnglishLabels,
    refresh,
    translateMenu,
    dispose,
  });

  window[GLOBAL_KEY] = api;

  if (document.readyState === 'loading' && !hadPrevious) {
    document.addEventListener('DOMContentLoaded', init, { once: true });
  } else {
    init();
  }

  return api;
}
