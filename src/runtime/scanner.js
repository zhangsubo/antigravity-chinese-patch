import { normalizeWhitespace } from '../shared/translator.js';
import {
  SKIP,
  CONTENT,
  POPUPS,
  UI_TARGETS,
  supportedSettingsHeadings,
  fallbackSettingsTexts,
  allowedFragments,
  navigationLabels,
  sidebarTestIds,
  sidebarPlaceholders,
  projectTriggerLabels,
  chatInputPlaceholders,
  historySafeTexts,
  overviewSafeTexts,
  permissionOptionSafeTexts,
  permissionDownloadLabels,
  approvalButtonSafeTexts,
  automationSafeTexts,
  createGuards,
} from './guards.js';

export function createScanner(translator, options = {}) {
  const records = new Map();
  const placeholderRecords = new Map();
  const attrRecords = new Map();
  let settingsRoot = null;
  let observer = null;
  let scheduled = false;

  const isEnabled = options.isEnabled || (() => true);
  const isDisposed = options.isDisposed || (() => false);

  const english = (node) => {
    const record = records.get(node);
    return record && node.nodeValue === record.translated ? record.original : node.nodeValue;
  };

  function originalText(element) {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    let result = '';
    let node;
    while ((node = walker.nextNode())) result += english(node);
    return normalizeWhitespace(result);
  }

  const state = { records, placeholderRecords, attrRecords, english, originalText };
  const guards = createGuards(state);
  const { protectedContent, protectedNode, skippedControl, buttonLeadLabel, isPermissionCard } = guards;

  function translateNode(node, allowPermissionOption = false) {
    if (protectedNode(node, allowPermissionOption)) return;
    const current = node.nodeValue;
    const previous = records.get(node);
    if (previous && current === previous.translated) return;
    records.delete(node);
    const key = normalizeWhitespace(current);
    const prevText = node.previousSibling?.nodeType === 3 ? english(node.previousSibling) : '';
    const nextText = node.nextSibling?.nodeType === 3 ? english(node.nextSibling) : '';
    const inlineConcat = (/\s$/.test(current) && /\S/.test(nextText)) ||
      (/^\s/.test(current) && /\S/.test(prevText)) ||
      (/\S/.test(prevText) && /\s$/.test(prevText)) ||
      (/\S/.test(nextText) && /^\s/.test(nextText)) ||
      (/^[A-Z][a-z]+\s+$/.test(current) && node.nextSibling?.tagName === 'STRONG');
    if (inlineConcat && !key.endsWith(':') && !allowedFragments.has(key)) return;
    const isSecondaryPill = Boolean(
      node.parentElement?.closest('.rounded-full.bg-secondary,[data-testid="customization-new-badge"]')
    );
    const target = translator.translate(key, { normalized: true, isSecondaryPill });
    if (!target) return;
    const translated = current.replace(/\S[\s\S]*\S|\S/, target);
    records.set(node, { original: current, translated });
    node.nodeValue = translated;
  }

  function setRecordedText(node, next) {
    const raw = english(node);
    if (node.nodeValue === next && records.get(node)?.original === raw) return;
    records.set(node, { original: raw, translated: next });
    node.nodeValue = next;
  }

  function translateMultiTextElement(el, allowPermissionOption = false) {
    if (el.children.length > 0 || el.childNodes.length < 2 || protectedNode(el, allowPermissionOption)) return false;
    const textNodes = [...el.childNodes].filter((n) => n.nodeType === 3);
    if (textNodes.length < 2) return false;
    const originals = textNodes.map(english);
    const raw = originals.join('');
    const key = normalizeWhitespace(raw);
    const target = translator.translate(key, { normalized: true });
    if (!target) {
      for (const n of textNodes) {
        const rec = records.get(n);
        if (rec?.multi && n.nodeValue === rec.translated) {
          n.nodeValue = rec.original;
          records.delete(n);
        }
      }
      return false;
    }
    const translated = raw.replace(/\S[\s\S]*\S|\S/, target);
    textNodes.forEach((n, i) => {
      const next = i === 0 ? translated : '';
      records.set(n, { original: originals[i], translated: next, multi: true });
      if (n.nodeValue !== next) n.nodeValue = next;
    });
    return true;
  }

  function translatePlaceholder(el) {
    if (!el || el.parentElement?.closest(SKIP) || protectedContent(el)) return;
    const current = el.getAttribute('placeholder');
    if (!current) return;
    const previous = placeholderRecords.get(el);
    if (previous && current === previous.translated) return;
    placeholderRecords.delete(el);
    const key = normalizeWhitespace(current);
    const target = translator.translate(key, { normalized: true });
    if (!target) return;
    const translated = current.replace(/\S[\s\S]*\S|\S/, target);
    placeholderRecords.set(el, { original: current, translated });
    el.setAttribute('placeholder', translated);
  }

  function translateAttrs(el) {
    if (!el || protectedContent(el) || skippedControl(el) ||
      (el.matches('[data-testid="title-menu-bar-item"]') && !['Workspace', 'View', 'Window'].includes(originalText(el))) ||
      el.matches('[data-testid="ssh-config-file-name"],[data-testid="ssh-connect-host-option"],[data-testid="ssh-host-label"],[data-testid="branch-pill-trigger"],[data-testid="external-link-confirm-uri"]')) return;
    let entry = attrRecords.get(el);
    for (const attr of ['title', 'aria-label']) {
      const current = el.getAttribute(attr);
      if (!current) continue;
      const prev = entry?.[attr];
      if (prev && current === prev.translated) continue;
      if (entry) delete entry[attr];
      const key = normalizeWhitespace(current);
      if (key === 'Actions') continue;
      const target = translator.translate(key, { normalized: true });
      if (!target) continue;
      const translated = current.replace(/\S[\s\S]*\S|\S/, target);
      if (!entry) { entry = {}; attrRecords.set(el, entry); }
      entry[attr] = { original: current, translated };
      el.setAttribute(attr, translated);
    }
  }

  function translateTree(root, allowPermissionOption = false) {
    if (!root || (root.nodeType === 1 && (protectedContent(root, allowPermissionOption) ||
      (!root.matches('input,textarea') && root.closest(SKIP))))) return;
    if (root.nodeType === 1 && translateMultiTextElement(root, allowPermissionOption)) return;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        if (node.nodeType === 1) {
          if (node.matches(SKIP + ',' + CONTENT) || translateMultiTextElement(node, allowPermissionOption)) {
            return NodeFilter.FILTER_REJECT;
          }
          return NodeFilter.FILTER_SKIP;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    let node;
    while ((node = walker.nextNode())) translateNode(node, allowPermissionOption);
    if (root.nodeType === 1) {
      if (root.matches('input[placeholder],textarea[placeholder]')) translatePlaceholder(root);
      for (const el of root.querySelectorAll('input[placeholder],textarea[placeholder]')) {
        translatePlaceholder(el);
      }
    }
  }

  function findSettings() {
    if (settingsRoot?.isConnected) return settingsRoot;
    const roots = [...document.querySelectorAll('[aria-label="Settings"], [role="dialog"]')];
    for (const heading of document.querySelectorAll('h1')) {
      if (originalText(heading) !== 'Settings') continue;
      let ancestor = heading.parentElement;
      for (let i = 0; ancestor && i < 6; i++, ancestor = ancestor.parentElement) {
        if (ancestor === document.body) break;
        roots.push(ancestor);
      }
    }
    settingsRoot = roots.find((root) => {
      if (root.matches?.('[aria-label="Settings"].settings-modal-container')) return true;
      const names = [...root.querySelectorAll('button')].map(originalText);
      return ['General', 'Application', 'Appearance'].every((name) => names.includes(name)) &&
        [...root.querySelectorAll('h1,h2')].some((h) => originalText(h) === 'Settings');
    });
    return settingsRoot;
  }

  function scanSettings() {
    const root = findSettings();
    if (!root) return;
    const headings = [...root.querySelectorAll('h2')].map(originalText);
    if (headings.some((name) => supportedSettingsHeadings.includes(name))) {
      translateTree(root);
      for (const el of root.querySelectorAll(
        'button[aria-label],input[aria-label],textarea[aria-label],[role="radio"][aria-label],' +
        '[role="radiogroup"][aria-label],[role="switch"][aria-label],[role="region"][aria-label],' +
        '[role="listbox"][aria-label],button[title]'
      )) {
        translateAttrs(el);
      }
    } else {
      for (const el of root.querySelectorAll('button,h1,h2,div')) {
        if ((el.tagName !== 'DIV' || el.children.length === 0) &&
          fallbackSettingsTexts.has(originalText(el))) {
          translateTree(el);
        }
      }
    }
  }

  function scanVoicePicker() {
    for (const vp of document.querySelectorAll('[data-testid="voice-picker"]')) {
      translateTree(vp);
      for (const el of vp.querySelectorAll('[aria-label],[title]')) translateAttrs(el);
    }
  }

  function scanSidebar() {
    const sidebar = document.querySelector('[aria-label="Sidebar"]');
    if (!sidebar) return;
    for (const el of sidebar.querySelectorAll('button,a')) {
      const label = el.getAttribute('aria-label');
      const href = el.getAttribute('href');
      const testId = el.getAttribute('data-testid');
      const text = originalText(el);
      const knownLink = el.tagName === 'A' && ['/', '/history', '/customizations'].includes(href);
      const rowChild = el.closest('[data-sidebar-row-id],[data-testid^="conversation-row-"]');
      if ((label && navigationLabels.has(label)) || (knownLink && navigationLabels.has(text)) ||
        (testId && sidebarTestIds.has(testId)) ||
        text === 'Provide Feedback' || text === 'Give Feedback' ||
        (!rowChild && (/^See all \(\d+\)$/.test(text) || text === 'See all' || text === 'See less' || text === 'Show less'))) {
        translateTree(el);
      }
    }
    for (const el of sidebar.querySelectorAll(
      '[data-testid="conversation-archive-button"],[data-testid="conversation-pin-button"],' +
      '[data-testid="conversation-resolve-button"],[data-testid="conversation-kebab"],' +
      '[data-testid="new-coordinator-button"],[data-testid="coordinators-more-button"],' +
      '[data-testid="plugins-visibility-trigger"],[data-testid="sidebar-add-project-button"],' +
      '[data-testid="customize-sidebar-button"],[data-testid="filter-sort-button"],' +
      '[data-testid="plugins-nav-more-button"],[data-testid="plugin-sidebar-more-button"],' +
      '[data-testid="conversation-row-draft"]'
    )) {
      translateAttrs(el);
    }
    for (const el of sidebar.querySelectorAll(
      '[data-testid="section-header"] h2,[data-testid="coordinator-section-header"] h2,' +
      '[data-testid="plugins-section-header"] h2'
    )) {
      translateTree(el);
    }
    for (const el of sidebar.querySelectorAll('span')) {
      const text = !el.closest('[data-sidebar-row-id],[data-testid^="conversation-row-"]') ? originalText(el) : '';
      if (text && (sidebarPlaceholders.has(text) || /^See all \(\d+\)$/.test(text))) {
        translateTree(el);
      }
    }
  }

  function translateDownloadDetailTerm(dt) {
    if (!dt || dt.firstChild?.nodeType !== 3 || dt.childNodes.length !== 1 || protectedNode(dt.firstChild)) return;
    const label = permissionDownloadLabels[originalText(dt)];
    const n = dt.firstChild;
    const cur = n.nodeValue;
    if (label && records.get(n)?.translated !== cur) {
      const tr = cur.replace(/\S[\s\S]*\S|\S/, label);
      records.set(n, { original: cur, translated: tr });
      n.nodeValue = tr;
    }
  }

  function scanChatChromeAndApprovals() {
    for (const el of document.querySelectorAll('[data-testid="project-selector-trigger"]')) {
      if (projectTriggerLabels.has(originalText(el))) translateTree(el);
    }
    for (const el of document.querySelectorAll('[data-testid="agent-input-box"]')) {
      translateTree(el);
    }
    for (const el of document.querySelectorAll('p.pointer-events-none')) {
      if (chatInputPlaceholders.has(originalText(el))) translateTree(el);
    }
    for (const el of document.querySelectorAll(
      '.files-changed-header,[data-testid="cleared-step-notice"],' +
      '[data-testid="load-more-status-pill"],[data-testid="suggested-action-row-cloud-env-setup"],' +
      '[data-testid="load-older-messages-button"],[data-testid="load-newer-status"],[data-testid="hidden-slice-loading"]'
    )) {
      translateTree(el);
    }
    for (const card of document.querySelectorAll('[data-testid="queued-messages-card"]')) {
      for (const span of card.querySelectorAll('span')) {
        if (span.children.length === 0 && originalText(span) === 'Queued Messages') translateTree(span);
      }
      for (const st of card.querySelectorAll('[data-testid="queued-messages-status"]')) {
        translateTree(st);
        translateAttrs(st);
      }
      for (const btn of card.querySelectorAll('[data-testid="queued-messages-expand-button"]')) {
        translateAttrs(btn);
      }
    }
    for (const el of document.querySelectorAll('[data-testid^="suggested-action-dismiss-"]')) {
      translateAttrs(el);
    }
    for (const el of document.querySelectorAll(
      '[data-testid="interaction-skip-button"],[data-testid="interaction-continue-button"],' +
      '[data-testid="permission-details-toggle"],.proceed-button,' +
      '[data-testid="artifact-card-split-button"],[data-testid="open-in-new-window-action"]'
    )) {
      translateTree(el);
    }
    for (const span of document.querySelectorAll('button > span.whitespace-nowrap')) {
      if (span.firstChild?.nodeType === 3 && normalizeWhitespace(english(span.firstChild)) === 'Proceed') {
        translateNode(span.firstChild);
      }
    }
    for (const el of document.querySelectorAll(
      '[data-testid="ask-question-collapse-button"],[data-testid="ask-question-dismiss-button"]'
    )) {
      translateAttrs(el);
    }
    for (const el of document.querySelectorAll('[data-testid="ask-question-progress"]')) {
      for (const btn of el.parentElement?.querySelectorAll('button') ?? []) translateAttrs(btn);
    }
    for (const el of document.querySelectorAll('[data-testid="ask-question-header-text"]')) {
      const badge = el.nextElementSibling;
      if (badge && originalText(badge) === 'Multi-select') translateTree(badge);
    }
    for (const el of document.querySelectorAll('[data-testid="ask-question-writein"]')) {
      const rawPh = placeholderRecords.get(el)?.original ?? el.getAttribute('placeholder');
      translatePlaceholder(el);
      if (rawPh && normalizeWhitespace(rawPh) === '(tell the agent what to do instead)') {
        const card = el.closest('[tabindex="-1"]');
        if (isPermissionCard(card)) {
          const prev = el.previousElementSibling;
          if (prev?.firstChild?.nodeType === 3 && prev.childNodes.length === 1 &&
            !protectedNode(prev.firstChild) && originalText(prev) === 'No') {
            const n = prev.firstChild;
            const cur = n.nodeValue;
            if (records.get(n)?.translated !== cur) {
              const tr = cur.replace(/\S+/, '否');
              records.set(n, { original: cur, translated: tr });
              n.nodeValue = tr;
            }
          }
          for (const opt of card.querySelectorAll('[data-testid="ask-question-option-text"]')) {
            if (opt.closest('[tabindex="-1"]') === card && permissionOptionSafeTexts.has(originalText(opt))) {
              translateTree(opt, true);
            }
          }
          for (const ed of card.querySelectorAll('[data-testid="permission-target-editor"]')) {
            if (ed.closest('[tabindex="-1"]') === card) translateAttrs(ed);
          }
          for (const dt of card.querySelectorAll('[data-testid="permission-download-details"] dt')) {
            if (dt.closest('[tabindex="-1"]') === card) translateDownloadDetailTerm(dt);
          }
          for (const span of card.querySelectorAll('span')) {
            if (span.closest('[tabindex="-1"]') === card && span.children.length === 0 &&
              originalText(span) === 'Confirm the command is safe to run outside of the sandbox with full network and disk access.') {
              translateTree(span);
            }
          }
        }
      }
    }
    for (const bar of document.querySelectorAll('.flex-wrap')) {
      if (protectedNode(bar)) continue;
      const prompt = bar.querySelector(':scope > p,:scope > span > p');
      if (!prompt || protectedNode(prompt)) continue;
      const text = originalText(prompt);
      const buttons = [...bar.querySelectorAll('button')].filter((btn) => !protectedNode(btn));
      const labels = new Set(buttons.map(buttonLeadLabel));
      const commandInput = text === 'Send command input?' && labels.has('Reject') && labels.has('Accept');
      const browserPermission = /^Agent needs permission to act on .+$/.test(text) &&
        ['Configure', 'Deny', 'Always Allow'].every((label) => labels.has(label));
      if (!commandInput && !browserPermission) continue;
      translateTree(prompt);
      for (const button of buttons) {
        if (approvalButtonSafeTexts.has(buttonLeadLabel(button))) translateTree(button);
      }
    }
    for (const btn of document.querySelectorAll('button[aria-label]')) {
      const rawAria = attrRecords.get(btn)?.['aria-label']?.original ?? btn.getAttribute('aria-label');
      if (rawAria !== 'Confirm Browser Interaction' || protectedNode(btn)) continue;
      const bar = btn.parentElement?.parentElement;
      const prompt = bar?.querySelector(':scope > span > span');
      const deny = [...(btn.parentElement?.querySelectorAll('button') ?? [])]
        .find((button) => buttonLeadLabel(button) === 'Deny');
      if (prompt && originalText(prompt) === 'Confirmation required to execute this step' && deny) {
        translateTree(prompt);
        translateTree(deny);
        translateTree(btn);
        translateAttrs(btn);
      }
    }
    for (const el of document.querySelectorAll('[data-testid="thinking-collapsible-trigger"]')) {
      if (originalText(el) !== 'Thinking') {
        translateTree(el);
        translateAttrs(el);
      }
    }
    for (const card of document.querySelectorAll('[data-testid="inline-permission-card"]')) {
      if (protectedNode(card)) continue;
      for (const el of card.querySelectorAll(
        '[data-testid="inline-permission-badge"],[data-testid="inline-permission-unsandboxed-warning"],' +
        '[data-testid="inline-permission-add-note"],[data-testid="inline-permission-in-chat"],' +
        '[data-testid="inline-permission-deny"],[data-testid="inline-permission-allow"],' +
        '[data-testid="permission-trigger-command"]'
      )) {
        translateTree(el);
      }
      for (const el of card.querySelectorAll('[data-testid="permission-target-editor"],[role="radiogroup"]')) {
        translateAttrs(el);
      }
      for (const dt of card.querySelectorAll('[data-testid="permission-download-details"] dt')) {
        translateDownloadDetailTerm(dt);
      }
      for (const opt of card.querySelectorAll('[data-testid^="inline-permission-scope-"]')) {
        const spans = [...opt.querySelectorAll('span')].filter(
          (s) => s.children.length === 0 && permissionOptionSafeTexts.has(originalText(s))
        );
        if (spans.length > 0) {
          for (const s of spans) translateTree(s, true);
        } else {
          const labelSpan = opt.querySelector('span:last-child') || opt;
          if (permissionOptionSafeTexts.has(originalText(labelSpan))) translateTree(labelSpan, true);
        }
      }
      for (const inp of card.querySelectorAll('[data-testid="inline-permission-deny-note"]')) {
        translatePlaceholder(inp);
      }
    }
    for (const card of document.querySelectorAll('[data-testid="plugin-grant-card"]')) {
      if (protectedNode(card)) continue;
      for (const btn of card.querySelectorAll('button')) {
        if (originalText(btn) === 'Manage permissions') translateTree(btn);
      }
      for (const span of card.querySelectorAll('span.text-xs')) {
        const txt = originalText(span);
        if (txt === 'Allowed in this conversation' || txt === 'Always allowed in this project' || txt === 'Always allowed') {
          translateTree(span);
        }
      }
    }
    for (const card of document.querySelectorAll('[data-testid="inline-card"]')) {
      if (protectedNode(card)) continue;
      for (const el of card.querySelectorAll(
        '[data-testid="inline-card-add-note"],[data-testid="inline-card-in-chat"],' +
        '[data-testid="inline-card-skip"],[data-testid="inline-card-next"],' +
        '[data-testid="inline-card-submit"],[data-testid="inline-card-retry"],' +
        '[data-testid="inline-card-settled-label"],[data-testid="inline-card-freeform-submit"]'
      )) {
        translateTree(el);
      }
      for (const inp of card.querySelectorAll('[data-testid="inline-card-approval-note"],[data-testid="inline-card-freeform-input"]')) {
        translatePlaceholder(inp);
        translateAttrs(inp);
      }
      for (const ta of card.querySelectorAll('[data-testid="inline-card-write-in"]')) {
        translatePlaceholder(ta);
        translateAttrs(ta);
      }
      const stateVal = card.getAttribute('data-inline-card-state');
      if (stateVal === 'superseded' || stateVal === 'expired') {
        for (const span of card.querySelectorAll('span')) {
          if (span.children.length === 0 &&
            ['Skipped', 'Replaced by newer request', 'Expired · Action not taken'].includes(originalText(span))) {
            translateTree(span);
          }
        }
      } else if (stateVal === 'answered') {
        const lead = card.querySelector(':scope > span:first-of-type');
        if (lead && originalText(lead) === 'Answered:') translateTree(lead);
      }
    }
  }

  function scanPopupsAndOverview() {
    for (const el of document.querySelectorAll(POPUPS)) {
      translateTree(el);
      for (const sub of el.querySelectorAll(
        '[data-testid="permissions-override-info-icon"],[data-testid="model-selector-effort-info"]'
      )) {
        translateAttrs(sub);
      }
    }
    for (const panel of document.querySelectorAll('[data-testid="overview-panel"]')) {
      for (const el of panel.querySelectorAll('span,button,div')) {
        if (el.children.length === 0 && overviewSafeTexts.has(originalText(el))) {
          translateTree(el);
        }
      }
    }
  }

  function scanAutomationsAndSidecars() {
    for (const el of document.querySelectorAll(
      '[data-testid="cloud-triggers-view"],[data-testid="trigger-InlineCreator"],[data-testid="automation-detail-view"],' +
      '[data-testid="new-sidecar-modal"]'
    )) {
      const rootEl = el.matches('[data-testid="new-sidecar-modal"]') ? (el.closest('[role="dialog"]') || el) : el;
      translateTree(rootEl);
      for (const b of rootEl.querySelectorAll('[aria-label],[title]')) translateAttrs(b);
    }
    for (const view of document.querySelectorAll(
      '[data-testid="sidecars-view"],[data-testid="automation-detail-page"],[data-testid="sidecar-detail"]'
    )) {
      for (const sub of view.querySelectorAll(
        '[data-testid="sidecar-list-empty"],[data-testid="automation-group"],' +
        '[data-testid="cloud-trigger-renew-all-credentials"],[data-testid="sidecar-create-with-prompt"],' +
        '[data-testid="sidecar-new-button"],[data-testid="sidecar-detail-run-now"],' +
        '[data-testid="cloud-trigger-detail-run-now"],[data-testid="automation-instructions-edit"],' +
        '[data-testid="automation-settings-edit"],[data-testid="automation-settings-save"],' +
        '[data-testid="automation-settings-cancel"],[data-testid="automation-runs-all"],' +
        '[data-testid="schedule-editor-panel"],[data-testid="cloud-trigger-editor-panel"]'
      )) {
        translateTree(sub);
      }
      for (const inp of view.querySelectorAll('input[placeholder],textarea[placeholder]')) {
        translatePlaceholder(inp);
      }
      for (const b of view.querySelectorAll(
        'button[aria-label],[data-testid="automation-untrusted-warning"],[data-testid="automation-error"],' +
        '[data-testid="automation-next-run"]'
      )) {
        translateAttrs(b);
      }
      for (const leaf of view.querySelectorAll('h1,h2,p,span,button,label,div')) {
        const text = leaf.children.length === 0 ? originalText(leaf) : '';
        if (text && (automationSafeTexts.has(text) || /^Task "[\s\S]+" not found\.$/.test(text))) {
          translateTree(leaf);
        }
      }
    }
  }

  function scanPanelsAndBrowser() {
    for (const el of document.querySelectorAll(
      '[data-testid="bottom-terminal-panel-title"],[data-testid="bottom-terminal-panel-starting"],' +
      '[data-testid="bottom-terminal-panel-empty"],[data-testid="terminal-tab-connecting"],' +
      '[data-testid="terminal-password-warning"]'
    )) {
      translateTree(el);
      translateAttrs(el);
    }
    for (const el of document.querySelectorAll(
      '[data-testid="bottom-terminal-panel-new"],[data-testid="bottom-terminal-panel-close"],' +
      '[data-testid="terminal-add-button"],[data-testid^="bottom-terminal-panel-split-"],' +
      '[data-testid^="bottom-terminal-panel-kill-"]'
    )) {
      translateAttrs(el);
    }
    for (const span of document.querySelectorAll('span.font-medium.text-sm')) {
      if (originalText(span) === 'Terminals') {
        translateTree(span);
        const scopeBtn = span.parentElement?.querySelector('button');
        if (scopeBtn && ['Conversation', 'Project'].includes(originalText(scopeBtn))) translateTree(scopeBtn);
      }
    }
    for (const el of document.querySelectorAll(
      '[data-testid="diff-view-mode-trigger"],[data-testid="diff-view-mode-option"]'
    )) {
      translateTree(el);
      const header = el.closest('.flex.items-center.text-sm');
      if (header) {
        for (const s of header.querySelectorAll(':scope > span.font-medium')) {
          if (originalText(s) === 'Review') translateTree(s);
        }
      }
    }
    for (const btn of document.querySelectorAll('button')) {
      const rawAria = attrRecords.get(btn)?.['aria-label']?.original ?? btn.getAttribute('aria-label');
      if (rawAria === 'Clear filter') {
        translateAttrs(btn);
        const turnChip = btn.parentElement;
        const header = turnChip?.parentElement;
        if (turnChip?.firstElementChild && originalText(turnChip.firstElementChild) === 'For Turn') {
          translateTree(turnChip.firstElementChild);
        }
        if (header?.firstElementChild && originalText(header.firstElementChild) === 'Review') {
          translateTree(header.firstElementChild);
        }
      }
    }
    for (const reloadBtn of document.querySelectorAll('[data-testid="builtin-browser-reload"]')) {
      const bar = reloadBtn.parentElement;
      for (const btn of bar ? bar.querySelectorAll('button') : [reloadBtn]) translateAttrs(btn);
    }
    for (const inp of document.querySelectorAll('[data-testid="browser-url-input"]')) {
      translatePlaceholder(inp);
      translateAttrs(inp);
    }
    for (const btn of document.querySelectorAll(
      '[data-testid="webview-download-show-in-folder"],[data-testid="webview-download-cancel"],' +
      '[data-testid="webview-download-remove"],[data-testid="webview-downloads-close"],' +
      '[data-testid="webview-url-bar-downloads"],' +
      '[data-testid="conversation-telemetry-icon-button"],[data-testid="conversation-telemetry-indicator"],' +
      '[data-testid="toolbar-text-selection-button"],[data-testid="toolbar-fit-mode-button"],[data-testid="toolbar-overflow-trigger"]'
    )) {
      translateAttrs(btn);
    }
    for (const el of document.querySelectorAll('[data-testid="browser-loopback-loading-state"]')) {
      translateTree(el);
    }
  }

  function scanCustomizationsAndHistory() {
    for (const search of document.querySelectorAll('[data-testid="customizations-search"]')) {
      const panel = search.closest('.overflow-y-auto') || search.closest('.max-w-4xl')?.parentElement?.parentElement || search.parentElement;
      if (panel) {
        translateTree(panel);
        for (const el of panel.querySelectorAll('[aria-label],[title]')) {
          if (!el.matches('[data-testid="featured-carousel-member-tile"],[data-testid="featured-carousel-description"]')) {
            translateAttrs(el);
          }
        }
      }
    }
    for (const el of document.querySelectorAll(
      '[data-testid="bundles-page"],[data-testid="bundle-detail"],[data-testid="marketplace-bundles-banner"],' +
      '[data-testid="marketplace-filter-button"],[data-testid="marketplace-filter-menu"],' +
      '[data-testid="marketplace-ranking-label"],[data-testid="customization-new-badge"]'
    )) {
      translateTree(el);
      for (const attrEl of el.querySelectorAll('[aria-label],[title]')) translateAttrs(attrEl);
      translateAttrs(el);
      for (const inp of el.querySelectorAll('input[placeholder]')) translatePlaceholder(inp);
    }
    if (location.pathname.startsWith('/customizations')) {
      for (const h2 of document.querySelectorAll('h2')) {
        if (originalText(h2) === 'Customizations') {
          const panel = h2.closest('.max-w-2xl')?.parentElement || h2.parentElement;
          if (panel) translateTree(panel);
        }
      }
    }
    for (const input of document.querySelectorAll('input[placeholder]')) {
      const record = placeholderRecords.get(input);
      const rawPlaceholder = record ? record.original : input.getAttribute('placeholder');
      if (rawPlaceholder && normalizeWhitespace(rawPlaceholder) === 'Search conversations...') {
        translatePlaceholder(input);
        const container = input.closest('.max-w-3xl,.max-w-2xl') || input.parentElement?.parentElement?.parentElement;
        if (container) {
          for (const el of container.querySelectorAll('h1,p,button,span')) {
            if (!el.closest('[role="button"]') && historySafeTexts.has(originalText(el))) {
              translateTree(el);
            }
          }
        }
      }
    }
  }

  function scanDialogsAndWizards() {
    for (const marker of document.querySelectorAll(
      '[data-testid="create-coordinator-submit"],[data-testid="coordinator-settings-save"],[data-testid="confirm-delete-side-question"],' +
      '[data-testid="browser-site-data-dialog"],[data-testid="browser-clear-data-confirm"],[data-testid="external-link-confirm-modal"],' +
      '[data-testid="plugin-source-caution-modal"],[data-testid="command-hook-warning"],' +
      '[data-testid="post-onboarding-terms-body"],[data-testid="post-onboarding-welcome-modal"],' +
      '[data-testid="customize-sidebar-modal"],[data-testid="vcs-commit-message"],' +
      '[data-testid="confirm-move-conversation-project"],[data-testid="plugin-permission-revoke-all-modal"],' +
      '[data-testid="dictation-languages-popover"]'
    )) {
      const dialog = marker.closest('[role="dialog"],[role="group"]');
      if (dialog) {
        translateTree(dialog);
        for (const el of dialog.querySelectorAll('[aria-label],[title]')) {
          if (!el.matches('[data-testid^="coordinator-icon-option-"],[data-testid^="coordinator-color-option-"]')) {
            translateAttrs(el);
          }
        }
        translateAttrs(dialog);
      }
    }
    for (const el of document.querySelectorAll(
      '[data-testid="plugin-permissions-allowed"],[data-testid="plugin-permissions-included"],' +
      '[data-testid="plugin-theme-apply"],[data-testid="plugin-detail-added-badge"]'
    )) {
      translateTree(el);
      for (const sub of el.querySelectorAll('[aria-label],[title]')) translateAttrs(sub);
      translateAttrs(el);
    }
    for (const el of document.querySelectorAll(
      '[data-testid="selection-quote-button"],[data-testid="selection-btw-button"],' +
      '[data-testid="selection-comment-button"],[data-testid="selection-popup-quote-button"],' +
      '[data-testid="terminal-selection-popup-quote-button"],[data-testid="terminal-selection-popup-side-question-button"],' +
      '[data-testid="side-panel-pending-terminal-tab"],[data-testid="agent-browser-cross-window-banner"],' +
      '[data-testid="side-question-waiting-status"],[data-testid="side-question-followup-thinking"],' +
      '[data-testid="file-tree-filter-empty"],[data-testid="csv-file-viewer"],' +
      '[data-testid="builtin-browser-caa-open-external"],[data-testid="onboarding-auth-code-panel"],' +
      '[data-testid="automation-model-unavailable"],[data-testid="browser-cert-interstitial"] h1,' +
      '[data-testid="browser-cert-advanced-button"],[data-testid="browser-cert-back-button"],' +
      '[data-testid="jetbox-access-blocked-screen"]'
    )) {
      translateTree(el);
    }
    for (const inp of document.querySelectorAll(
      '[data-testid="side-question-reply-input"],[data-testid="file-tree-filter-input"],[data-testid="csv-filter-input"]'
    )) {
      translatePlaceholder(inp);
      translateAttrs(inp);
    }
    for (const el of document.querySelectorAll(
      '[data-testid="side-question-reply-send"],[data-testid="side-question-popout"],' +
      '[data-testid="side-question-tab-stop"],[data-testid="side-question-tab-promote"],' +
      '[data-testid="side-question-tab-delete"],[data-testid="side-question-resize-handle"],' +
      '[data-testid="side-question-insert-to-main"],' +
      '[data-testid="selection-code-search-button"],[data-testid="side-panel-new-tab-button"],' +
      '[data-testid="comment-card-delete"],[data-testid="comment-card-edit"],' +
      '[data-testid="coordinator-worker-card"] + button,[data-testid="coordinator-worker-stop"],' +
      '[data-testid="file-tree-filter-clear"],' +
      '[data-testid="file-tree-vcs-dot"],[data-testid="csv-filter-clear"],' +
      '[data-testid="lsp-peek-references-close"],[data-testid="url-security-indicator"]'
    )) {
      translateTree(el);
      translateAttrs(el);
    }
    for (const marker of document.querySelectorAll(
      '[data-testid="ssh-connect-dialog"],[data-testid="provider-resource-filter"],' +
      '[data-testid="google-drive-connect-view"],[data-testid="project-folder-selection-step"],' +
      '[data-testid="cloud-resource-disclosure-modal"]'
    )) {
      const dialog = marker.closest('.z-\\[6000\\],[role="dialog"]') || marker;
      translateTree(dialog);
      for (const el of dialog.querySelectorAll('[aria-label],[title]')) translateAttrs(el);
    }
    for (const btn of document.querySelectorAll('[data-testid="project-folder-selection-step"] button')) {
      if (originalText(btn) !== 'project') continue;
      const prev = btn.previousElementSibling;
      const next = btn.nextElementSibling;
      const pNode = prev?.firstChild;
      const bNode = btn.firstChild;
      const nNode = next?.firstChild;
      if ([prev, btn, next].some((el) => el?.childNodes.length !== 1) ||
        [pNode, bNode, nNode].some((n) => n?.nodeType !== 3)) continue;
      const zhPrefix = {
        'A|exists': '已存在相同的',
        'A similar|exists': '已存在相似的',
        'A|with the same folder exists': '已存在使用相同文件夹的',
        'A|with the same folders exists': '已存在使用相同文件夹组合的',
      }[`${originalText(prev)}|${originalText(next)}`];
      if (!zhPrefix) continue;
      setRecordedText(pNode, zhPrefix);
      setRecordedText(bNode, '项目');
      setRecordedText(nNode, '');
    }
    for (const dialog of document.querySelectorAll('[role="dialog"]')) {
      const h2 = dialog.querySelector('h2');
      const title = h2 ? originalText(h2) : '';
      if (title === 'Set Project Name') {
        const inp = dialog.querySelector('form input[type="text"]');
        const rawPh = inp ? (placeholderRecords.get(inp)?.original ?? inp.getAttribute('placeholder')) : '';
        if (rawPh && normalizeWhitespace(rawPh) === 'Enter project name...') {
          translateTree(dialog);
          for (const el of dialog.querySelectorAll('[aria-label],[title]')) translateAttrs(el);
        }
      } else if (title === 'Project already exists') {
        const btnTexts = new Set([...dialog.querySelectorAll('button')].map(originalText));
        if (['Back', 'Create New', 'Use Existing'].every((t) => btnTexts.has(t))) {
          translateTree(h2);
          for (const el of dialog.querySelectorAll('button')) {
            translateTree(el);
            translateAttrs(el);
          }
          const body = [...dialog.querySelectorAll('div')].find((d) =>
            d.children.length === 1 && d.firstElementChild?.matches('span.font-semibold'));
          const nodes = body ? [...body.childNodes] : [];
          if (nodes.length === 5 && nodes[0].nodeType === 3 && nodes[1].nodeType === 1 &&
            nodes[2].nodeType === 3 && nodes[3].nodeType === 3 && nodes[4].nodeType === 3 &&
            english(nodes[0]) === 'Project ' && english(nodes[2]) === ' already exists for this ' &&
            english(nodes[4]) === '. Would you like to continue using it or make a new one?') {
            const scope = english(nodes[3]);
            if (scope === 'folder' || scope === 'combination of folders') {
              setRecordedText(nodes[0], '项目 ');
              setRecordedText(nodes[2], scope === 'folder'
                ? ' 已关联此文件夹。要继续使用该项目，还是创建一个新项目？'
                : ' 已关联此文件夹组合。要继续使用该项目，还是创建一个新项目？');
              setRecordedText(nodes[3], '');
              setRecordedText(nodes[4], '');
            }
          }
        }
      }
    }
  }

  function scanMultiAgentAndMenuBar() {
    for (const greeting of document.querySelectorAll('[data-testid="onboarding-brand-shell"] p.entrance-step')) {
      const node = greeting.firstChild;
      if (node?.nodeType !== 3 || node.nextSibling || protectedNode(node)) continue;
      const raw = english(node);
      if (/^Hey .+!$/u.test(raw)) setRecordedText(node, raw.replace(/^Hey (.+)!$/u, '你好，$1！'));
    }
    for (const rootEl of document.querySelectorAll(
      '[data-testid="computer-permissions-card"],[data-agy-agents-pane],' +
      '[data-testid="webview-url-bar-draft-capsule"],[data-testid="comment-box"],' +
      '[data-testid="title-menu-bar"],[data-testid="onboarding-brand-shell"]'
    )) {
      translateTree(rootEl);
      for (const el of rootEl.querySelectorAll('[aria-label],[title]')) translateAttrs(el);
    }
    for (const el of document.querySelectorAll(
      '[data-testid="ssh-machine-switcher"],[data-testid="permissions-selector-trigger"],' +
      '[data-testid="model-selector-refresh"],[data-testid="setup-script-save-button"],' +
      '[data-testid="setup-script-validate-button"],[data-testid="setup-script-status"],' +
      '[data-testid="setup-script-copy-button"],[data-testid="jj-worktree-selector-trigger"],' +
      '[data-testid="jj-worktree-menu"],[data-testid="jj-previous-worktrees-menu"],' +
      '[data-testid="ssh-sign-in-subtitle"]'
    )) {
      translateTree(el);
      translateAttrs(el);
    }
    for (const el of document.querySelectorAll(
      '[data-testid="ssh-host-label"],[data-testid="file-tree-filter-error"],' +
      '[data-testid="onboarding-switch-to-auth-code"],[data-testid="home-greeting"],' +
      '[data-testid="orchestrator-picker-default"]'
    )) {
      translateTree(el);
    }
    for (const el of document.querySelectorAll(
      '[data-testid="app-logo-menu-trigger"],[data-testid="custom-window-controls"] button,' +
      '[data-testid="window-control-minimize"],[data-testid="window-control-maximize"],[data-testid="window-control-close"]'
    )) {
      translateAttrs(el);
    }
    const coordHoverLabels = { blocked: '已阻塞', ready: '已就绪', running: '运行中', failed: '已失败' };
    for (const el of document.querySelectorAll('[data-testid="coordinator-hover-card-count"] > span:last-child')) {
      const node = el.firstChild;
      if (node?.nodeType === 3 && !node.nextSibling && !protectedNode(node)) {
        const label = coordHoverLabels[originalText(el)];
        if (label) setRecordedText(node, label);
      }
    }
    for (const card of document.querySelectorAll('[data-testid="sidebar-agent-hover-card"]')) {
      const status = card.querySelector('[data-testid="sidebar-agent-hover-card-status"] > span:first-child');
      if (status) translateTree(status);
      for (const sp of card.querySelectorAll('[data-testid="sidebar-agent-hover-card-status"] > span')) {
        if (sp.children.length === 0 && /^\d+\s+waiting$/.test(originalText(sp))) translateTree(sp);
      }
    }
  }

  const scanners = [
    scanSettings,
    scanVoicePicker,
    scanSidebar,
    scanChatChromeAndApprovals,
    scanPopupsAndOverview,
    scanAutomationsAndSidecars,
    scanPanelsAndBrowser,
    scanCustomizationsAndHistory,
    scanDialogsAndWizards,
    scanMultiAgentAndMenuBar,
  ];

  function observe() {
    if (!isDisposed() && isEnabled() && document.body && observer) {
      observer.observe(document.body, {
        childList: true,
        subtree: true,
        characterData: true,
        attributes: true,
        attributeFilter: ['aria-label', 'title', 'placeholder'],
      });
    }
  }

  function scan() {
    if (isDisposed() || !isEnabled() || !document.body) return;
    observer?.disconnect();
    try {
      for (const node of records.keys()) if (!node.isConnected) records.delete(node);
      for (const el of placeholderRecords.keys()) if (!el.isConnected) placeholderRecords.delete(el);
      for (const el of attrRecords.keys()) if (!el.isConnected) attrRecords.delete(el);
      for (let i = 0; i < scanners.length; i++) {
        scanners[i]();
      }
    } finally {
      observe();
    }
  }

  function restore() {
    observer?.disconnect();
    for (const [node, record] of records) {
      if (node.isConnected && node.nodeValue === record.translated) node.nodeValue = record.original;
    }
    records.clear();
    for (const [el, record] of placeholderRecords) {
      if (el.isConnected && el.getAttribute('placeholder') === record.translated) {
        el.setAttribute('placeholder', record.original);
      }
    }
    placeholderRecords.clear();
    for (const [el, entry] of attrRecords) {
      if (!el.isConnected) continue;
      for (const [attr, record] of Object.entries(entry)) {
        if (el.getAttribute(attr) === record.translated) el.setAttribute(attr, record.original);
      }
    }
    attrRecords.clear();
  }

  function schedule() {
    if (scheduled || isDisposed() || !isEnabled()) return;
    scheduled = true;
    window.requestAnimationFrame(() => {
      scheduled = false;
      scan();
    });
  }

  function onMutations(mutations) {
    if (mutations.some((m) => {
      const el = m.target.nodeType === 1 ? m.target : m.target.parentElement;
      if (!el) return false;
      const protectedElement = m.type === 'attributes' && el.matches('input,textarea') ? el.parentElement : el;
      if (protectedElement?.closest(SKIP)) return false;
      if (protectedContent(el)) {
        return !!el.closest('[data-testid="ask-question-option-text"]') &&
          isPermissionCard(el.closest('[tabindex="-1"]'));
      }
      if (el.closest(UI_TARGETS) || isPermissionCard(el.closest('[tabindex="-1"]'))) return true;
      if (m.type !== 'childList') return false;
      return [...m.addedNodes, ...m.removedNodes].some((node) => {
        if (node.nodeType !== 1 || skippedControl(node) || protectedContent(node)) return false;
        if (node.matches(UI_TARGETS)) return true;
        return [...node.querySelectorAll(UI_TARGETS)].some((candidate) =>
          !skippedControl(candidate) && !protectedContent(candidate));
      });
    })) schedule();
  }

  function startObserver() {
    if (!observer && typeof MutationObserver !== 'undefined') {
      observer = new MutationObserver(onMutations);
    }
  }

  function dispose() {
    restore();
    observer = null;
  }

  return {
    startObserver,
    observe,
    scan,
    restore,
    dispose,
  };
}
