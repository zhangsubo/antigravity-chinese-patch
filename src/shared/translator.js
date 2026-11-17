import { ENGLISH_OPTION_LABELS } from './constants.js';

export function normalizeWhitespace(value) {
  return String(value ?? '').trim().replace(/\s+/g, ' ');
}

export function formatLimitName(raw) {
  const lower = raw.trim().toLowerCase();
  if (lower === 'weekly' || lower === 'weekly limit') return '本周额度';
  if (lower === '5-hour' || lower === '5 hour' || lower === 'five-hour' || lower === 'five hour') return ' 5 小时额度';
  if (lower === 'daily' || lower === 'daily limit') return '今日额度';
  if (lower === 'hourly' || lower === 'hourly limit') return '每小时额度';
  if (lower === 'monthly' || lower === 'monthly limit') return '本月额度';
  return `${raw.trim()}额度`;
}

export function formatDuration(raw) {
  return raw.trim()
    .replace(/less than a minute|<1m/gi, '不到 1 分钟')
    .replace(/(\d+)\s*days?/gi, '$1 天')
    .replace(/(\d+)\s*hours?/gi, '$1 小时')
    .replace(/(\d+)\s*(?:minutes?|mins?)/gi, '$1 分钟')
    .replace(/(\d+)\s*(?:seconds?|secs?)/gi, '$1 秒')
    .replace(/(\d+)d\b/gi, '$1 天')
    .replace(/(\d+)h\b/gi, '$1 小时')
    .replace(/(\d+)m\b/gi, '$1 分钟')
    .replace(/,\s*and\s+|\s+and\s+|,\s*/gi, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

export function createTranslator(options = {}) {
  const dictionary = options.dictionary || {};
  const rawPatterns = Array.isArray(options.patterns) ? options.patterns : [];
  const compiledPatterns = rawPatterns.map((item) => ({
    id: item.id || '',
    regex: new RegExp(item.regex, (item.flags || '').replace(/g/g, '')),
    replacement: item.replacement,
  }));

  const translatedValues = new Set();
  for (const val of Object.values(dictionary)) {
    if (typeof val === 'string' && val) translatedValues.add(val);
  }

  let keepEnglishLabels = Boolean(options.keepEnglishLabels);
  const customMap = new Map();
  const cache = new Map();
  const MAX_CACHE = 8000;

  function setCustomTranslations(entries) {
    customMap.clear();
    cache.clear();
    if (!Array.isArray(entries)) return;
    for (const entry of entries) {
      if (!Array.isArray(entry) || entry.length < 1) continue;
      const key = normalizeWhitespace(entry[0]).toLowerCase();
      if (!key) continue;
      const value = typeof entry[1] === 'string' ? entry[1].trim() : '';
      customMap.set(key, value);
    }
  }

  if (options.customEntries) {
    setCustomTranslations(options.customEntries);
  }

  function setKeepEnglishLabels(next) {
    const boolNext = Boolean(next);
    if (keepEnglishLabels === boolNext) return false;
    keepEnglishLabels = boolNext;
    cache.clear();
    return true;
  }

  function getKeepEnglishLabels() {
    return keepEnglishLabels;
  }

  function lookupOptionLabel(raw) {
    const key = normalizeWhitespace(raw);
    if (customMap.has(key.toLowerCase())) {
      const custom = customMap.get(key.toLowerCase());
      return custom === '' ? key : custom;
    }
    if (keepEnglishLabels && ENGLISH_OPTION_LABELS.has(key.toLowerCase())) {
      return key;
    }
    return Object.hasOwn(dictionary, key) ? dictionary[key] : key;
  }

  function cacheSet(key, val) {
    if (cache.size >= MAX_CACHE) {
      const iter = cache.keys();
      for (let i = 0; i < 800; i++) {
        const oldKey = iter.next().value;
        if (oldKey === undefined) break;
        cache.delete(oldKey);
      }
    }
    cache.set(key, val);
    return val;
  }

  function translateCompound(key) {
    let match = key.match(
      /^You(?:'ve| have) used (some|all|most|\d+%) of your ([^,]+?)(?: limit)?,\s*(?:and )?it will (?:fully )?refresh in (.+?)\.?$/i
    );
    if (match) {
      const amountMap = { some: '部分', all: '全部', most: '大部分' };
      const amount = amountMap[match[1].toLowerCase()] || ` ${match[1]} 的`;
      return `你已使用${amount}${formatLimitName(match[2])}，将于 ${formatDuration(match[3])}后完全恢复。`;
    }

    match = key.match(
      /^You(?:'ve| have) reached your ([^,]+?)(?: limit)?,\s*(?:and )?it will (?:fully )?refresh in (.+?)\.?$/i
    );
    if (match) {
      return `你已达到${formatLimitName(match[1])}上限，将于 ${formatDuration(match[2])}后完全恢复。`;
    }

    match = key.match(/^You (?:have not|haven't) used any of your ([^.]+?)(?: limit)?\.?$/i);
    if (match) {
      return `你尚未使用${formatLimitName(match[1])}。`;
    }

    match = key.match(/^Resets in (.+)$/i);
    if (match) {
      return `将于 ${formatDuration(match[1])}后重置`;
    }

    match = key.match(/^(\d+) active conversations?(?: and (\d+) archived conversations?)?$/i);
    if (match) {
      return match[2]
        ? `${match[1]} 个进行中对话和 ${match[2]} 个已归档对话`
        : `${match[1]} 个进行中对话`;
    }

    match = key.match(/^(.+) \((Experimental|Default|Preview)\)$/);
    if (match && Object.hasOwn(dictionary, match[1])) {
      const suffix = { Experimental: '实验性', Default: '默认', Preview: '预览' }[match[2]];
      return `${dictionary[match[1]]}（${suffix}）`;
    }

    match = key.match(/^(Submit|Continue) \((.+)\)$/);
    if (match) {
      return `${match[1] === 'Submit' ? '提交' : '继续'} (${match[2]})`;
    }

    match = key.match(/^Running for (.+)$/);
    if (match) {
      return `已运行 ${formatDuration(match[1])}`;
    }

    match = key.match(/^No (automations|scheduled tasks|sidecars) match [“"]([\s\S]+)[”"]\.$/);
    if (match) {
      const noun = { automations: '自动化任务', 'scheduled tasks': '计划任务', sidecars: '扩展服务' }[match[1]];
      return `没有匹配“${match[2]}”的${noun}。`;
    }

    match = key.match(/^An? (automation|scheduled task) with (?:the )?ID [“"]?(.+?)[”"]? already exists\.$/);
    if (match) {
      return `ID 为“${match[2]}”的${match[1] === 'automation' ? '自动化任务' : '计划任务'}已存在。`;
    }

    match = key.match(/^Agent permissions preset selector, current:\s*(.+)$/);
    if (match) {
      return `智能体权限预设选择器，当前：${lookupOptionLabel(match[1])}`;
    }

    match = key.match(/^Agent permissions preset:\s*([^,]+),\s*(.+)$/);
    if (match) {
      return `智能体权限预设：${lookupOptionLabel(match[1])}，${dictionary[match[2]] || match[2]}`;
    }

    match = key.match(/^(.+) needs? the terminal sandbox, which isn't available on this host\.$/);
    if (match) {
      const presets = match[1].split(/\s+and\s+/).map((p) => lookupOptionLabel(p.trim())).join(' 和 ');
      return `${presets} 需要终端沙盒，但当前主机不支持该功能。`;
    }

    match = key.match(/^(\d+)([smhd]) limit$/i);
    if (match) {
      return `${match[1]} ${{ s: '秒', m: '分钟', h: '小时', d: '天' }[match[2].toLowerCase()]}额度`;
    }

    match = key.match(/^(Daily limit|Weekly limit|\d+[smhd] limit),\s*resets in (.+)$/i);
    if (match) {
      const cap = match[1][0].toUpperCase() + match[1].slice(1).toLowerCase();
      const head = Object.hasOwn(dictionary, cap) ? dictionary[cap] : translateDynamic(match[1]);
      return `${head}，将于 ${formatDuration(match[2])}后重置`;
    }

    return null;
  }

  function translateDynamic(key) {
    const compound = translateCompound(key);
    if (compound !== null) return compound;

    for (let i = 0; i < compiledPatterns.length; i++) {
      const p = compiledPatterns[i];
      if (p.regex.test(key)) {
        return key.replace(p.regex, p.replacement);
      }
    }
    return null;
  }

  function translate(raw, context = {}) {
    if (typeof raw !== 'string' || !raw) return null;
    const key = context.normalized ? raw : normalizeWhitespace(raw);
    if (!key) return null;

    // 1. User custom translations override everything (empty string forces keeping English)
    const lower = key.toLowerCase();
    if (customMap.has(lower)) {
      const custom = customMap.get(lower);
      return custom === '' ? null : custom;
    }

    // 2. Built-in "Keep English option labels" preset
    if (keepEnglishLabels && ENGLISH_OPTION_LABELS.has(lower)) {
      return null;
    }

    // 3. Context-sensitive badge override ("New" in rounded-full pill -> "新")
    if (key === 'New' && context.isSecondaryPill) {
      return '新';
    }

    if (cache.has(key)) {
      return cache.get(key);
    }

    // Fast O(1) short-circuit for pure punctuation / numbers / already-translated copy
    if (!/[a-zA-Z]/.test(key) || translatedValues.has(key)) {
      return cacheSet(key, null);
    }

    // 4. Exact dictionary match
    if (Object.hasOwn(dictionary, key)) {
      return cacheSet(key, dictionary[key]);
    }

    // 5. Dynamic compound formatters and pattern rules
    const dyn = translateDynamic(key);
    return cacheSet(key, dyn);
  }

  return {
    translate,
    translateDynamic,
    setCustomTranslations,
    setKeepEnglishLabels,
    getKeepEnglishLabels,
    hasExact: (key) => Object.hasOwn(dictionary, key),
    clearCache: () => cache.clear(),
  };
}
