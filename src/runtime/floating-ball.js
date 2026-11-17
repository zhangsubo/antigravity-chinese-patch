import {
  VERSION,
  CONTROL_ATTR,
  STORAGE_LANG,
  STORAGE_POS,
  STORAGE_KEEP_EN,
  LANG_CHANNEL,
  QUERY_LANG_CHANNEL,
} from '../shared/constants.js';

function loadPref(key) {
  try {
    return window.localStorage?.getItem(key) ?? null;
  } catch (_) {
    return null;
  }
}

function savePref(key, value) {
  try {
    window.localStorage?.setItem(key, value);
  } catch (_) {
    /* ignore unavailable storage (e.g. data: URLs) */
  }
}

function clampPosition(left, top, elWidth, elHeight) {
  const vw = window.innerWidth || document.documentElement?.clientWidth || 1024;
  const vh = window.innerHeight || document.documentElement?.clientHeight || 768;
  const w = elWidth > 0 ? elWidth : 84;
  const h = elHeight > 0 ? elHeight : 28;
  const margin = 8;
  const maxLeft = Math.max(margin, vw - w - margin);
  const maxTop = Math.max(margin, vh - h - margin);
  return {
    left: Math.min(Math.max(margin, Math.round(left)), maxLeft),
    top: Math.min(Math.max(margin, Math.round(top)), maxTop),
  };
}

export function createFloatingBall(options) {
  const {
    mode = 'static',
    getEnabled,
    onToggleEnabled,
    getKeepEnglishLabels,
    onToggleKeepEnglishLabels,
  } = options;

  let control = null;
  let menu = null;
  let menuOpen = false;
  let keepEnCheckbox = null;
  let zhItemBtn = null;
  let enItemBtn = null;
  let dragging = false;
  let moved = false;
  let suppressClick = false;
  let startX = 0;
  let startY = 0;
  let startLeft = 0;
  let startTop = 0;

  function broadcastToFrames() {
    const payload = {
      channel: LANG_CHANNEL,
      enabled: getEnabled(),
      keepEnglishLabels: getKeepEnglishLabels(),
    };
    try {
      for (const frame of document.querySelectorAll('iframe')) {
        frame.contentWindow?.postMessage(payload, '*');
      }
    } catch (_) { /* cross-origin or detached */ }
  }

  function applyStoredPosition() {
    if (!control) return;
    const raw = loadPref(STORAGE_POS);
    if (!raw) {
      control.style.left = '';
      control.style.top = '';
      control.style.right = '14px';
      control.style.bottom = '14px';
      return;
    }
    try {
      const parsed = JSON.parse(raw);
      if (typeof parsed?.left === 'number' && typeof parsed?.top === 'number') {
        const rect = control.getBoundingClientRect();
        const clamped = clampPosition(parsed.left, parsed.top, rect.width, rect.height);
        control.style.right = 'auto';
        control.style.bottom = 'auto';
        control.style.left = `${clamped.left}px`;
        control.style.top = `${clamped.top}px`;
      }
    } catch (_) { /* ignore malformed pos */ }
  }

  function updateControlUI() {
    if (!control) return;
    const enabled = getEnabled();
    control.textContent = enabled ? '中文 · 开' : '中文 · 关';
    control.setAttribute('aria-pressed', String(enabled));
    if (keepEnCheckbox) {
      keepEnCheckbox.checked = getKeepEnglishLabels();
    }
    if (zhItemBtn && enItemBtn) {
      zhItemBtn.style.fontWeight = enabled ? '600' : '400';
      enItemBtn.style.fontWeight = !enabled ? '600' : '400';
      zhItemBtn.textContent = `${enabled ? '✓ ' : '  '}简体中文`;
      enItemBtn.textContent = `${!enabled ? '✓ ' : '  '}English (原文)`;
    }
  }

  function positionMenu() {
    if (!menu || !control) return;
    const vw = window.innerWidth || 1024;
    const vh = window.innerHeight || 768;
    const rect = control.getBoundingClientRect();
    const ctrlLeft = rect.width > 0 ? rect.left : (parseFloat(control.style.left) || (vw - 98));
    const ctrlTop = rect.height > 0 ? rect.top : (parseFloat(control.style.top) || (vh - 42));
    const menuWidth = 228;
    const menuHeight = 148;
    const left = Math.min(Math.max(8, ctrlLeft - menuWidth + (rect.width || 84)), vw - menuWidth - 8);
    const top = ctrlTop > menuHeight + 16 ? ctrlTop - menuHeight - 8 : Math.min(ctrlTop + (rect.height || 28) + 8, vh - menuHeight - 8);
    menu.style.left = `${Math.round(left)}px`;
    menu.style.top = `${Math.round(top)}px`;
  }

  function setMenuOpen(next) {
    menuOpen = Boolean(next);
    if (!menu) return;
    if (menuOpen) {
      updateControlUI();
      positionMenu();
      menu.style.display = 'block';
    } else {
      menu.style.display = 'none';
    }
  }

  function buildMenu() {
    menu = document.createElement('div');
    menu.setAttribute('data-agy-zh-menu', 'true');
    menu.setAttribute('data-no-translate', 'true');
    menu.setAttribute('role', 'menu');
    menu.style.cssText =
      'display:none;position:fixed;z-index:2147483647;width:228px;padding:6px;' +
      'background:#fff;color:#1f2937;border:1px solid #d1d5db;border-radius:10px;' +
      'box-shadow:0 8px 24px rgba(0,0,0,.16);font:12px/1.4 system-ui,-apple-system,sans-serif;';

    const makeAction = (label, onClick) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.textContent = label;
      btn.style.cssText =
        'display:block;width:100%;text-align:left;padding:6px 8px;border:0;border-radius:6px;' +
        'background:transparent;color:inherit;cursor:pointer;font:inherit;';
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        onClick();
        setMenuOpen(false);
      });
      return btn;
    };

    zhItemBtn = makeAction('✓ 简体中文', () => {
      savePref(STORAGE_LANG, 'zh');
      onToggleEnabled(true);
      updateControlUI();
      broadcastToFrames();
    });
    enItemBtn = makeAction('  English (原文)', () => {
      savePref(STORAGE_LANG, 'en');
      onToggleEnabled(false);
      updateControlUI();
      broadcastToFrames();
    });

    const divider = document.createElement('div');
    divider.style.cssText = 'height:1px;background:#e5e7eb;margin:5px 2px;';

    const keepEnRow = document.createElement('label');
    keepEnRow.style.cssText =
      'display:flex;align-items:center;gap:6px;padding:6px 8px;border-radius:6px;cursor:pointer;user-select:none;';
    keepEnCheckbox = document.createElement('input');
    keepEnCheckbox.type = 'checkbox';
    keepEnCheckbox.setAttribute('data-agy-zh-keep-en', 'true');
    keepEnCheckbox.checked = getKeepEnglishLabels();
    keepEnCheckbox.addEventListener('change', (e) => {
      const checked = Boolean(e.target.checked);
      savePref(STORAGE_KEEP_EN, checked ? '1' : '0');
      onToggleKeepEnglishLabels(checked);
      updateControlUI();
      broadcastToFrames();
    });
    const keepEnLabel = document.createElement('span');
    keepEnLabel.textContent = '选项名保留英文';
    keepEnRow.append(keepEnCheckbox, keepEnLabel);

    const footer = document.createElement('div');
    footer.style.cssText = 'padding:4px 8px 2px;font-size:11px;color:#6b7280;';
    footer.textContent = `v${VERSION} · ${mode === 'cdp' ? 'CDP 动态' : '静态补丁'}`;

    menu.append(zhItemBtn, enItemBtn, divider, keepEnRow, footer);
    document.body.appendChild(menu);
  }

  function onMouseDown(e) {
    if (e.button !== 0 || !control) return;
    dragging = true;
    moved = false;
    startX = e.clientX;
    startY = e.clientY;
    const rect = control.getBoundingClientRect();
    const vw = window.innerWidth || 1024;
    const vh = window.innerHeight || 768;
    startLeft = rect.width > 0 ? rect.left : (parseFloat(control.style.left) || (vw - 98));
    startTop = rect.height > 0 ? rect.top : (parseFloat(control.style.top) || (vh - 42));
  }

  function onMouseMove(e) {
    if (!dragging || !control) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (!moved && Math.abs(dx) + Math.abs(dy) <= 4) return;
    moved = true;
    const rect = control.getBoundingClientRect();
    const clamped = clampPosition(startLeft + dx, startTop + dy, rect.width, rect.height);
    control.style.right = 'auto';
    control.style.bottom = 'auto';
    control.style.left = `${clamped.left}px`;
    control.style.top = `${clamped.top}px`;
    if (menuOpen) positionMenu();
  }

  function onMouseUp() {
    if (!dragging || !control) return;
    dragging = false;
    if (moved) {
      suppressClick = true;
      const left = parseFloat(control.style.left);
      const top = parseFloat(control.style.top);
      if (Number.isFinite(left) && Number.isFinite(top)) {
        savePref(STORAGE_POS, JSON.stringify({ left, top }));
      }
      setTimeout(() => { suppressClick = false; }, 0);
    }
  }

  function onResize() {
    if (!control || !control.style.left || !control.style.top) return;
    const left = parseFloat(control.style.left);
    const top = parseFloat(control.style.top);
    if (!Number.isFinite(left) || !Number.isFinite(top)) return;
    const rect = control.getBoundingClientRect();
    const clamped = clampPosition(left, top, rect.width, rect.height);
    control.style.left = `${clamped.left}px`;
    control.style.top = `${clamped.top}px`;
    if (menuOpen) positionMenu();
  }

  function onDocumentClick(e) {
    if (!menuOpen) return;
    if (menu?.contains(e.target) || control?.contains(e.target)) return;
    setMenuOpen(false);
  }

  function onStorage(e) {
    if (!e || e.storageArea !== window.localStorage) return;
    if (e.key === STORAGE_LANG) {
      onToggleEnabled(e.newValue !== 'en', { persist: false });
      updateControlUI();
      broadcastToFrames();
    } else if (e.key === STORAGE_KEEP_EN) {
      onToggleKeepEnglishLabels(e.newValue === '1', { persist: false });
      updateControlUI();
      broadcastToFrames();
    } else if (e.key === STORAGE_POS) {
      applyStoredPosition();
    }
  }

  function onMessage(e) {
    if (e?.data?.channel === QUERY_LANG_CHANNEL && e.source && typeof e.source.postMessage === 'function') {
      try {
        e.source.postMessage({
          channel: LANG_CHANNEL,
          enabled: getEnabled(),
          keepEnglishLabels: getKeepEnglishLabels(),
        }, '*');
      } catch (_) { /* ignore */ }
    }
  }

  function mount() {
    if (!document.body) return;
    control = document.createElement('button');
    control.type = 'button';
    control.setAttribute(CONTROL_ATTR, VERSION);
    control.setAttribute('data-no-translate', 'true');
    control.title = 'Antigravity 中文补丁：单击切换中英文，拖拽调整位置，右键或双击打开选项';
    control.setAttribute('aria-label', '切换界面汉化');
    control.style.cssText =
      'position:fixed;right:14px;bottom:14px;z-index:2147483647;' +
      'font:12px system-ui,-apple-system,sans-serif;padding:6px 10px;' +
      'border:1px solid rgba(170,170,170,.75);border-radius:20px;' +
      'background:rgba(255,255,255,.9);color:#222;cursor:pointer;' +
      'backdrop-filter:blur(6px);box-shadow:0 2px 8px rgba(0,0,0,.14);user-select:none;';

    control.addEventListener('click', () => {
      if (suppressClick) {
        suppressClick = false;
        return;
      }
      const next = !getEnabled();
      savePref(STORAGE_LANG, next ? 'zh' : 'en');
      onToggleEnabled(next);
      updateControlUI();
      broadcastToFrames();
    });
    control.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      setMenuOpen(!menuOpen);
    });
    control.addEventListener('dblclick', (e) => {
      e.preventDefault();
      setMenuOpen(!menuOpen);
    });
    control.addEventListener('mousedown', onMouseDown);

    document.body.appendChild(control);
    buildMenu();
    applyStoredPosition();
    updateControlUI();

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    window.addEventListener('resize', onResize);
    window.addEventListener('storage', onStorage);
    window.addEventListener('message', onMessage);
    document.addEventListener('click', onDocumentClick);
  }

  function readInitialPrefs() {
    const langPref = loadPref(STORAGE_LANG);
    const keepEnPref = loadPref(STORAGE_KEEP_EN);
    return {
      enabled: langPref !== 'en',
      keepEnglishLabels: keepEnPref === '1',
    };
  }

  function syncState(opts = {}) {
    if (opts.persistLang !== undefined) savePref(STORAGE_LANG, opts.persistLang ? 'zh' : 'en');
    if (opts.persistKeepEn !== undefined) savePref(STORAGE_KEEP_EN, opts.persistKeepEn ? '1' : '0');
    updateControlUI();
    broadcastToFrames();
  }

  function unmount() {
    window.removeEventListener('mousemove', onMouseMove);
    window.removeEventListener('mouseup', onMouseUp);
    window.removeEventListener('resize', onResize);
    window.removeEventListener('storage', onStorage);
    window.removeEventListener('message', onMessage);
    document.removeEventListener('click', onDocumentClick);
    control?.remove();
    menu?.remove();
    control = null;
    menu = null;
    keepEnCheckbox = null;
    zhItemBtn = null;
    enItemBtn = null;
  }

  return {
    mount,
    unmount,
    readInitialPrefs,
    syncState,
    broadcastToFrames,
  };
}
