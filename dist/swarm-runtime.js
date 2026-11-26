/* Antigravity Chinese Patch Swarm Runtime v3.0.5 */
(function () {
  'use strict';
// --- module: src/shared/constants.js ---
const VERSION = '3.0.5';
const GLOBAL_KEY = '__antigravity_chinese_patch__';
const LEGACY_KEY = '__teacherxu_antigravity_zh__';
const SWARM_GLOBAL_KEY = '__antigravity_chinese_patch_swarm__';
const ORIGINALS_KEY = '__antigravity_chinese_patch_originals__';
const CUSTOM_DICT_KEY = '__agyZhCustomTranslations';
const MODE_KEY = '__AGY_ZH_MODE__';

const RUNTIME_MARKER = `/* Antigravity Chinese Patch Runtime v${VERSION} */`;
const SWARM_MARKER = `/* Antigravity Chinese Patch Swarm Runtime v${VERSION} */`;

const LANG_CHANNEL = '__antigravity_chinese_patch_lang__';
const QUERY_LANG_CHANNEL = '__antigravity_chinese_patch_query_lang__';

const STORAGE_LANG = 'agyZhLang';
const STORAGE_POS = 'agyZhBallPos';
const STORAGE_KEEP_EN = 'agyZhKeepEnglishLabels';
const CONTROL_ATTR = 'data-agy-zh-control';

const ENGLISH_OPTION_LABELS = new Set([
  'this conversation',
  'request review',
  'default',
  'auto',
  'turbo',
  'high',
  'medium',
  'low',
]);

// --- module: src/swarm/swarm-i18n.js ---
function mountSwarmRuntime(swarmData, buildOptions = {}) {
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

  const __SWARM_DATA__ = {"scope":"desktop built-in plugin://swarm--swarm_board-<id>/ only","protectedRoots":["#ov-frame","#raw","#swarm-title","#gif-modal-title",".msg-body",".o2-sttxt",".o2-sttag",".o2-card",".o2-runmain",".o2-donem",".o2-ask",".o2-mnow",".o2-mrole",".o2-mauth","code","pre","script","style","[contenteditable='true']"],"selectors":["#loading .loading-caption","[data-testid='swarm-loading']","#live","#live-label","nav[data-testid='swarm-tabs']","[data-testid='swarm-tab-overview']","[data-testid='swarm-tab-feed']","[data-testid='swarm-tab-members']","[data-testid='swarm-tab-graph']","[data-testid='swarm-tab-raw']","#tabbtn-overview","#tabbtn-feed","#tabbtn-members","#tabbtn-graph","#tabbtn-raw","#empty-title","#empty-body","#empty .empty-hint","#ov-mode","#ov-label .ov-label-text","#ov-footer .ov-hint > span","#ov-open-coord","#ov-status .badge","[data-testid='swarm-overview-badge']","[data-testid='swarm-overview-stale']","#ov-system","#ov-system button[data-open]","#ov-notice","#f-search","#f-toggle","#channel-tabs","#agents","#f-agent","#f-mention","#f-channel","#f-agent option[value='']","#f-mention option[value='']","#f-mention option[value='@none']","#f-mention option[value='@all']","#f-channel option[value='']","#filters label.switch","#filters label.switch > span","#feed [data-testid='swarm-feed-empty'] > div","#feed [data-testid='swarm-feed-empty'] .feed-empty-title","[data-testid='swarm-feed-empty']","#feed [data-clear-filters]","#feed [data-testid='swarm-load-earlier']","#feed [data-testid='swarm-unread-divider']","#jump-latest","#feed > article.msg > button.msg-more[data-testid='swarm-message-more']","#feed > article.msg > .msg-head .msg-note","#tab-graph .btn-group","#gview-hive","#gview-mentions","#graph-export-gif","#graph-export-gif-label","#gif-modal .gif-modal-hint","#gif-modal-open","#replay-start","#replay-back","#replay-play","#replay-fwd","#replay-slider","#replay-live","#tab-graph label.switch","#tab-graph label.switch > span","#replay-timing","#replay-timing option","#replay-speed","#graph","#tab-graph .legend-item","#tab-raw .segmented","#raw-copy","#raw-copy-label","#members .o2-msin","#members .o2-msec > span","#members .o2-mret > span","#members .o2-mstat.info","#members .o2-mstat.bad",".o2-mrow.working .o2-mstat.info",".o2-mrow.needs_you .o2-mstat.warn",".o2-team > .o2-h",".o2-team .o2-lb > b",".o2-br > .o2-h",".o2-none",".o2-team .o2-m.all",".o2-more",".o2-tvh > b",".o2-progt",".o2-progt > b",".o2-pchip",".o2-qhdr",".o2-doneempty",".o2-tprog.rev",".o2-sttop > b","#ov-plain [data-testid='swarm-onboarding'] .o2-onb-h","#ov-plain [data-testid='swarm-onboarding'] .o2-onb-steps b","#ov-plain [data-testid='swarm-onboarding'] .o2-onb-steps div > span","#ov-plain [data-testid='swarm-onboarding'] .o2-onb-wait > span"],"attributeSelectors":["[data-testid='swarm-live']","[data-testid='swarm-tabs']","[data-testid='swarm-overview-system']","[data-testid='swarm-search']","[data-testid='swarm-filter-toggle']","[data-testid='swarm-channel-tabs']","#tab-graph .segmented","#gif-modal-close","#g-hide-inactive",".o2-crew",".o2-tvx",".o2-tvnav",".o2-tvstrip","#feed > article.msg > .msg-head .msg-author"],"exact":{".":"。","@all posts, drawn by Expand @all":"@all 消息，可通过“展开 @all”显示连线","@mention, width = count":"@提及，线宽表示次数","Active":"活跃成员","All":"全部","All channels":"所有频道","All workers":"所有成员","Blocked":"受阻","Board":"看板","Channels":"频道","Clear filters":"清除筛选","Close":"关闭","Close preview":"关闭预览","Coordinator":"协调者","Coordinator · Working":"协调者 · 工作中","Copied":"已复制","Copy":"复制","Copy failed":"复制失败","Copy to clipboard":"复制到剪贴板","Could not load earlier posts":"无法加载更早消息","Done":"已完成","Draw @all as an edge to every member":"将 @all 显示为连接各成员的连线","Each takes a task, and they run in parallel.":"每位执行者负责一项任务，并行开展工作。","Error":"错误","Even pacing":"均匀间隔","Expand @all":"展开 @all","Export GIF":"导出 GIF","Export graph replay as an animated GIF":"将关系图回放导出为 GIF 动图","Exporting…":"正在导出…","File":"文件","Filter by channel":"按频道筛选","Filter by poster":"按发送者筛选","Filter by recipient":"按接收者筛选","Filter by worker":"按成员筛选","Filters":"筛选","Filters (active)":"筛选（已启用）","Follow":"跟随最新消息","Frames":"帧","From anyone":"所有发送者","Goal":"目标","Graph":"关系图","Graph view":"关系图视图","Hide inactive":"隐藏不活跃成员","Hide killed and inactive members from the graph":"在关系图中隐藏已终止及不活跃的成员","Hive":"蜂巢","In review":"审核中","Jump to start":"跳转到开头","Keep the newest post in view":"始终显示最新消息","Latest":"最新消息","Live":"实时","Looking for this conversation's swarm…":"正在查找此会话的 Swarm…","Lost contact with the swarm sidecar; retrying":"已与 Swarm 辅助进程断开连接，正在重试","Many workers":"多位执行者","Members":"成员","Mentions":"提及","Narrator's page":"讲述者页面","Narrator’s page":"讲述者页面","Needs you":"需要你","New":"新消息","Next":"下一项","Next post":"下一条消息","Next post (→)":"下一条消息（→）","No loaded posts match":"已加载消息中没有匹配项","No narrator in this swarm. Ask the coordinator to add one.":"此 Swarm 尚无讲述者。可请协调者添加。","No posts match":"没有匹配的消息","No posts yet":"暂无消息","No swarm in this conversation":"此会话尚无 Swarm","No tasks completed yet.":"暂无已完成的任务。","Not on the roster":"不在成员列表中","Nothing blocked":"没有受阻事项","Nothing planned":"没有计划中的事项","Nothing running":"没有进行中的事项","Now":"现在","One board":"一个共享看板","One goal":"一个目标","Open":"打开","Open coordinator chat":"打开协调者会话","Open in new tab":"在新标签页中打开","Open this pane from a conversation to see its swarm. Swarms started there appear here automatically; start one with":"从会话中打开此面板即可查看其 Swarm。在该会话中启动的 Swarm 会自动显示在这里；可使用以下命令启动：","Overview":"概览","Pause replay":"暂停回放","Planned next":"接下来计划","Play / pause replay (space)":"播放 / 暂停回放（空格键）","Play replay":"播放回放","Posts to the swarm board will appear here as members talk.":"成员交流时，看板消息会显示在这里。","Previous":"上一项","Previous post":"上一条消息","Previous post (←)":"上一条消息（←）","Progress":"进度","Raw":"原始数据","Real-time gaps":"实际时间间隔","Reconnecting":"正在重新连接","Replay":"回放","Replay pacing":"回放节奏","Replay position":"回放位置","Replay speed":"回放速度","Retired":"已退出成员","Retry loading earlier posts":"重试加载更早消息","Retry narrator's page":"重新加载讲述者页面","Right-click image to save, or open in a new tab.":"右键点击图片即可保存，也可在新标签页中打开。","Running":"进行中","Scrub through board posts":"拖动以浏览看板消息","Search":"搜索","Search members":"搜索成员","Search posts":"搜索看板消息","Show everything and follow live":"显示全部内容并跟随实时更新","Show less":"收起","Show more":"展开更多","Show or hide each member's posts":"显示或隐藏各成员的消息","Show posts by one member":"显示某位成员发送的消息","Show posts in one channel":"显示某个频道中的消息","Show posts that call on one member":"显示提及某位成员的消息","Showing the last good view.json; the current file is not valid JSON.":"正在显示上次有效的 view.json；当前文件不是有效的 JSON。","Showing the last good view.json; the current file is over 64 KB.":"正在显示上次有效的 view.json；当前文件超过 64 KB。","Standing by":"待命成员","Stuck":"受阻","Swarm stalled — no agents working":"Swarm 已停滞 — 当前无成员在工作","Swarm views":"Swarm 视图","Swarms started in this conversation appear here automatically, with their board, members and @mention graph. Start one with":"在此会话中启动的 Swarm 会自动显示在这里，包括看板、成员和 @提及关系图。启动命令：","System alerts":"系统提醒","Team":"团队成员","The coordinator splits it into tasks.":"协调者将目标拆分为任务。","The narrator hasn't published an overview yet.":"讲述者尚未发布概览。","The narrator is updating the overview":"讲述者正在更新概览","The narrator is waiting for new board posts":"讲述者正在等待新的看板消息","The narrator was killed; the overview no longer updates":"讲述者已被终止，概览不再更新","The narrator's view.json is over 64 KB, so it isn't shown.":"讲述者的 view.json 超过 64 KB，因此未显示。","The narrator's view.json isn't valid JSON yet.":"讲述者的 view.json 尚不是有效的 JSON。","The team":"团队","Threads":"任务线","Threads as cells, with each worker where it works":"以蜂巢格展示各条任务线及成员所在位置","To @all":"提及 @all","To anyone":"所有接收者","To no one":"未提及任何人","Try a different search or member filter.":"请尝试其他搜索条件或成员筛选条件。","Updates automatically":"自动更新","Updates land here. You step in only when asked.":"进展会汇集到这里。需要您参与时才会请您介入。","Waiting for the first update":"等待首次更新","Want this view different? Tell the coordinator.":"想调整此视图？请告诉协调者。","Watching for a swarm":"等待 Swarm 启动","Who @mentions whom":"成员之间的 @提及关系","Working":"工作中","Your swarm is starting":"Swarm 正在启动","narrator's page · AI-generated":"讲述者页面 · 由 AI 生成","not on roster":"不在成员列表中","overview · from the narrator's notes":"概览 · 来自讲述者的记录","overview · from the narrator’s notes":"概览 · 来自讲述者的记录","paused: error":"已暂停：发生错误","paused: quota":"已暂停：配额不足","posted with no mention":"未提及任何人的消息","stale":"未及时更新","stopped":"已停止","tasks done":"项任务已完成","updating":"正在更新","via @all":"通过 @all","watching":"等待更新"},"patterns":[{"id":"swarm-needs-you","parentSelector":".o2-pchip","regex":"^(\\d+) needs? you$","flags":"","replacement":"$1 项需要你参与"},{"id":"swarm-blocked","parentSelector":".o2-pchip","regex":"^(\\d+) blocked$","flags":"","replacement":"$1 项受阻"},{"id":"swarm-threads-open","parentSelector":".o2-pchip","regex":"^(\\d+) threads? open$","flags":"","replacement":"$1 条任务线未结束"},{"id":"swarm-active","parentSelector":".o2-pchip","regex":"^(\\d+) active$","flags":"","replacement":"$1 项进行中"},{"id":"swarm-tasks-progress","parentSelector":".o2-progt > b","regex":"^(\\d+) of (\\d+)$","flags":"","replacement":"$1 / $2"},{"id":"swarm-up-next","parentSelector":".o2-qhdr","regex":"^Up next · (\\d+)$","flags":"","replacement":"接下来 · $1"},{"id":"swarm-plus-more","parentSelector":"","regex":"^\\+(\\d+) more$","flags":"","replacement":"还有 $1 项"},{"id":"swarm-show-earlier-posts","parentSelector":"[data-testid='swarm-load-earlier']","regex":"^Show (\\d+) earlier posts? \\((\\d+) older\\)$","flags":"","replacement":"显示更早的 $1 条消息（还有 $2 条较早消息）"},{"id":"swarm-loading-earlier-posts","parentSelector":"[data-testid='swarm-load-earlier']","regex":"^Loading earlier posts… \\((\\d+) older\\)$","flags":"","replacement":"正在加载更早消息…（还有 $1 条较早消息）"},{"id":"swarm-retry-loading-earlier-posts","parentSelector":"[data-testid='swarm-load-earlier']","regex":"^Retry loading (\\d+) earlier posts?$","flags":"","replacement":"重试加载 $1 条更早消息"},{"id":"swarm-earlier-posts-failed","parentSelector":"#feed [data-testid='swarm-feed-empty'] > div","regex":"^(\\d+) earlier posts? could not be loaded\\.$","flags":"","replacement":"有 $1 条更早消息未能加载。"}]};
  mountSwarmRuntime(__SWARM_DATA__, { fingerprint: "3.0.5-f003b67e2dbe" });
})();
