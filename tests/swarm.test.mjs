import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { JSDOM } from 'jsdom';

const swarmSource = fs.readFileSync('dist/swarm-runtime.js', 'utf8');

const swarmHtml = `
<div id="loading"><div class="loading-caption">Looking for this conversation's swarm…</div></div>
<div id="live" data-testid="swarm-live" title="Updates automatically"><span id="live-label">Live</span></div>
<nav data-testid="swarm-tabs" aria-label="Swarm views">
  <button id="tabbtn-overview" data-testid="swarm-tab-overview">Overview</button>
  <button id="tabbtn-feed" data-testid="swarm-tab-feed">Board<span>3</span></button>
  <button id="tabbtn-members" data-testid="swarm-tab-members">Members</button>
  <button id="tabbtn-graph" data-testid="swarm-tab-graph">Graph</button>
  <button id="tabbtn-raw" data-testid="swarm-tab-raw">Raw</button>
</nav>
<h1 id="swarm-title">Board</h1>
<section id="empty" data-testid="swarm-empty-state">
  <div id="empty-title">No swarm in this conversation</div>
  <div id="empty-body">Open this pane from a conversation to see its swarm. Swarms started there appear here automatically; start one with <code>/swarm</code>.</div>
  <div class="empty-hint">Watching for a swarm</div>
</section>
<section id="filters">
  <input id="f-search" data-testid="swarm-search" placeholder="Search posts">
  <button id="f-toggle" data-testid="swarm-filter-toggle">Filters</button>
  <select id="f-channel">
    <option value="">All channels</option>
    <option value="custom-all">All channels</option>
  </select>
  <label class="switch" title="Keep the newest post in view"><span>Follow</span></label>
</section>
<section id="feed">
  <article class="msg">
    <div class="msg-head">
      <span class="msg-author" title="Coordinator">Alice</span>
      <span class="msg-note">not on roster</span>
    </div>
    <div class="msg-body">Board</div>
    <button class="msg-more" data-testid="swarm-message-more">Show more</button>
  </article>
</section>
<section id="ov-plain">
  <div data-testid="swarm-onboarding">
    <div class="o2-onb-h">Your swarm is starting</div>
    <div class="o2-onb-steps"><div><b>One goal</b><span>The coordinator splits it into tasks.</span></div></div>
    <div class="o2-onb-wait"><span>Waiting for the first update</span></div>
  </div>
  <div class="o2">
    <div class="o2-progt"><b>3 of 7</b> tasks done</div>
    <span class="o2-pchip" id="chip-needs">1 needs you</span>
    <span class="o2-pchip" id="chip-blocked">2 blocked</span>
    <span class="o2-qhdr">Up next · 4</span>
    <div class="o2-doneempty">No tasks completed yet.</div>
    <div id="custom-narrator-lbl" class="o2-team"><div class="o2-h">NarratorCustomTeam</div></div>
    <div id="default-team-lbl" class="o2-team"><div class="o2-h">The team</div></div>
    <section class="o2-stall" data-testid="swarm-stall-banner">
      <div class="o2-stmain">
        <div class="o2-sttop"><b>Swarm stalled — no agents working</b><button class="o2-sttag">@worker #12</button></div>
        <p class="o2-sttxt">Board</p>
      </div>
    </section>
  </div>
</section>
<section id="tab-graph">
  <div class="segmented" role="group" aria-label="Graph view">
    <button id="gview-hive" title="Threads as cells, with each worker where it works">Hive</button>
    <button id="gview-mentions" title="Who @mentions whom">Mentions</button>
  </div>
  <button id="replay-play" title="Play replay"></button>
  <select id="replay-timing"><option value="even">Even pacing</option></select>
  <button id="graph-export-gif" title="Export graph replay as an animated GIF"><span id="graph-export-gif-label">Export GIF</span></button>
</section>
<div id="gif-modal">
  <div id="gif-modal-title">Board</div>
  <button id="gif-modal-close" aria-label="Close preview" title="Close"></button>
  <span class="gif-modal-hint">Right-click image to save, or open in a new tab.</span>
  <a id="gif-modal-open">Open in new tab</a>
</div>
<pre id="raw">Board</pre>
`;

function createSwarmDom(html = swarmHtml, url = 'plugin://swarm--swarm_board-9334/?token=secret') {
  const dom = new JSDOM(html, { url, runScripts: 'outside-only', pretendToBeVisual: true });
  dom.window.SwarmHive = {
    viewCopy: {
      ui: {
        teamHeader: 'NarratorCustomTeam',
      },
    },
  };
  dom.window.eval(swarmSource);
  dom.window.document.dispatchEvent(new dom.window.Event('DOMContentLoaded'));
  const close = dom.window.close.bind(dom.window);
  dom.window.close = () => {
    dom.window.__antigravity_chinese_patch_swarm__?.dispose();
    close();
  };
  return dom;
}

const wait = () => new Promise((r) => setTimeout(r, 50));

test('Swarm runtime translates scoped UI controls while protecting names, bodies, commands, and narrator labels', async () => {
  const dom = createSwarmDom();
  const w = dom.window;
  const d = w.document;

  assert.equal(d.querySelector('#loading .loading-caption').textContent, '正在查找此会话的 Swarm…');
  assert.equal(d.querySelector('#live-label').textContent, '实时');
  assert.equal(d.querySelector('#live').getAttribute('title'), '自动更新');
  assert.equal(d.querySelector('nav[data-testid="swarm-tabs"]').getAttribute('aria-label'), 'Swarm 视图');
  assert.equal(d.querySelector('#tabbtn-overview').textContent, '概览');
  assert.equal(d.querySelector('#tabbtn-feed').textContent, '看板3');
  assert.equal(d.querySelector('#empty-title').textContent, '此会话尚无 Swarm');
  assert.ok(d.querySelector('#empty-body').textContent.includes('/swarm'));
  assert.equal(d.querySelector('#empty-body code').textContent, '/swarm');
  assert.equal(d.querySelector('#f-search').getAttribute('placeholder'), '搜索看板消息');
  assert.equal(d.querySelector('#f-channel option[value=""]').textContent, '所有频道');
  assert.equal(d.querySelector('#f-channel option[value="custom-all"]').textContent, 'All channels');
  assert.equal(d.querySelector('.msg-author').textContent, 'Alice');
  assert.equal(d.querySelector('.msg-author').getAttribute('title'), '协调者');
  assert.equal(d.querySelector('.msg-note').textContent, '不在成员列表中');
  assert.equal(d.querySelector('.msg-body').textContent, 'Board');
  assert.equal(d.querySelector('.msg-more').textContent, '展开更多');
  assert.equal(d.querySelector('#swarm-title').textContent, 'Board');
  assert.equal(d.querySelector('#raw').textContent, 'Board');
  assert.equal(d.querySelector('.o2-progt').textContent, '3 / 7 项任务已完成');
  assert.equal(d.querySelector('#chip-needs').textContent, '1 项需要你参与');
  assert.equal(d.querySelector('#chip-blocked').textContent, '2 项受阻');
  assert.equal(d.querySelector('.o2-qhdr').textContent, '接下来 · 4');
  assert.equal(d.querySelector('#custom-narrator-lbl .o2-h').textContent, 'NarratorCustomTeam');
  assert.equal(d.querySelector('#default-team-lbl .o2-h').textContent, '团队');
  assert.equal(d.querySelector('#replay-play').getAttribute('title'), '播放回放');
  assert.equal(d.querySelector('.o2-sttop > b').textContent, 'Swarm 已停滞 — 当前无成员在工作');
  assert.equal(d.querySelector('.o2-sttxt').textContent, 'Board');
  assert.equal(d.querySelector('#tab-graph .segmented').getAttribute('aria-label'), '关系图视图');
  assert.equal(d.querySelector('#gview-hive').textContent, '蜂巢');
  assert.equal(d.querySelector('#gview-mentions').textContent, '提及');
  assert.equal(d.querySelector('#graph-export-gif').getAttribute('title'), '将关系图回放导出为 GIF 动图');
  assert.equal(d.querySelector('#graph-export-gif-label').textContent, '导出 GIF');
  assert.equal(d.querySelector('#gif-modal-title').textContent, 'Board');
  assert.equal(d.querySelector('#gif-modal-close').getAttribute('aria-label'), '关闭预览');
  assert.equal(d.querySelector('#gif-modal .gif-modal-hint').textContent, '右键点击图片即可保存，也可在新标签页中打开。');
  assert.equal(d.querySelector('#gif-modal-open').textContent, '在新标签页中打开');

  // If a translated node is moved into a protected container, revert it automatically
  const note = d.querySelector('.msg-note');
  d.querySelector('.msg-body').appendChild(note);
  await wait();
  assert.equal(note.textContent, 'not on roster');

  // Sync disable/enable via postMessage from parent window
  w.dispatchEvent(new w.MessageEvent('message', {
    source: w.parent,
    origin: 'http://127.0.0.1:12345',
    data: { channel: '__antigravity_chinese_patch_lang__', enabled: false },
  }));
  assert.equal(d.querySelector('#tabbtn-overview').textContent, 'Overview');
  assert.equal(d.querySelector('#f-search').getAttribute('placeholder'), 'Search posts');
  assert.equal(d.querySelector('.o2-progt').textContent, '3 of 7 tasks done');

  w.dispatchEvent(new w.MessageEvent('message', {
    source: w.parent,
    origin: 'http://127.0.0.1:12345',
    data: { channel: '__antigravity_chinese_patch_lang__', enabled: true },
  }));
  assert.equal(d.querySelector('#tabbtn-overview').textContent, '概览');
  dom.window.close();
});

test('Swarm runtime rejects non-plugin origins and missing Swarm DOM markers', () => {
  const nonPlugin = createSwarmDom(swarmHtml, 'https://example.com/');
  assert.equal(nonPlugin.window.__antigravity_chinese_patch_swarm__, undefined);
  assert.equal(nonPlugin.window.document.querySelector('#tabbtn-overview').textContent, 'Overview');
  nonPlugin.window.close();

  const missingMarkerHtml = swarmHtml.replace('data-testid="swarm-empty-state"', 'data-other="x"');
  const noMarker = createSwarmDom(missingMarkerHtml, 'plugin://swarm--swarm_board-ab12c/');
  assert.equal(noMarker.window.document.querySelector('#tabbtn-overview').textContent, 'Overview');
  noMarker.window.close();
});
