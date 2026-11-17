#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import vm from 'node:vm';
import { RUNTIME_MARKER, SWARM_MARKER, VERSION } from '../src/shared/constants.js';
import { installSwarmFrames } from '../src/swarm/main-frame-hook.js';

const ROOT = path.resolve(import.meta.dirname, '..');

export function countCaptureGroups(regexStr) {
  let count = 0;
  let inCharClass = false;
  for (let i = 0; i < regexStr.length; i++) {
    const ch = regexStr[i];
    if (ch === '\\') {
      i++;
      continue;
    }
    if (ch === '[' && !inCharClass) {
      inCharClass = true;
      continue;
    }
    if (ch === ']' && inCharClass) {
      inCharClass = false;
      continue;
    }
    if (!inCharClass && ch === '(') {
      const lookahead = regexStr.slice(i, i + 4);
      if (
        lookahead.startsWith('(?:') ||
        lookahead.startsWith('(?=') ||
        lookahead.startsWith('(?!') ||
        lookahead.startsWith('(?<=') ||
        lookahead.startsWith('(?<!')
      ) {
        continue;
      }
      count++;
    }
  }
  return count;
}

export function getReferencedGroups(replacementStr) {
  const matches = replacementStr.match(/\$(\d+)/g) || [];
  const indices = matches.map((m) => parseInt(m.slice(1), 10));
  return {
    indices,
    max: indices.length > 0 ? Math.max(...indices) : 0,
  };
}

export function findRawJsonDuplicateKeys(rawText) {
  const duplicates = [];
  let i = 0;
  const n = rawText.length;
  const skipWs = () => { while (i < n && /\s/.test(rawText[i])) i++; };
  const parseString = () => {
    const start = i;
    i++; // skip opening "
    while (i < n) {
      const ch = rawText[i++];
      if (ch === '\\') { i++; continue; }
      if (ch === '"') return JSON.parse(rawText.slice(start, i));
    }
    throw new SyntaxError('Unterminated JSON string');
  };
  const skipPrimitive = () => { while (i < n && !/[,\]}\s]/.test(rawText[i])) i++; };
  const parseValue = () => {
    skipWs();
    if (i >= n) return;
    const ch = rawText[i];
    if (ch === '"') {
      parseString();
    } else if (ch === '{') {
      i++;
      const seen = new Set();
      skipWs();
      if (rawText[i] === '}') { i++; return; }
      while (i < n) {
        skipWs();
        if (rawText[i] !== '"') break;
        const key = parseString();
        if (seen.has(key)) duplicates.push(key);
        seen.add(key);
        skipWs();
        if (rawText[i] === ':') i++;
        parseValue();
        skipWs();
        if (rawText[i] === ',') { i++; continue; }
        if (rawText[i] === '}') { i++; break; }
        break;
      }
    } else if (ch === '[') {
      i++;
      skipWs();
      if (rawText[i] === ']') { i++; return; }
      while (i < n) {
        parseValue();
        skipWs();
        if (rawText[i] === ',') { i++; continue; }
        if (rawText[i] === ']') { i++; break; }
        break;
      }
    } else {
      skipPrimitive();
    }
  };
  parseValue();
  return duplicates;
}

export function validateAndCompileLocaleData({ exactFiles, patternFiles, swarmFile }) {
  const errors = [];
  const exact = {};

  const sortedExactFiles = [...exactFiles].sort((a, b) => a.name.localeCompare(b.name));
  for (const { name, raw } of sortedExactFiles) {
    let parsed;
    try {
      parsed = JSON.parse(raw);
    } catch (err) {
      errors.push(`[JSON] ${name}: 解析失败 (${err.message})`);
      continue;
    }
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      errors.push(`[Schema] ${name}: 必须是 JSON 对象`);
      continue;
    }
    for (const dup of findRawJsonDuplicateKeys(raw)) {
      errors.push(`[Duplicate Key Gate] ${name} 内部存在重复键 "${dup}"`);
    }

    for (const [key, value] of Object.entries(parsed)) {
      if (typeof key !== 'string' || !key || key.trim() !== key) {
        errors.push(`[Trim & Non-Empty Gate] ${name}: 非法键名 "${key}"`);
        continue;
      }
      if (typeof value !== 'string' || value.trim() === '') {
        errors.push(`[Trim & Non-Empty Gate] ${name}: 词条 "${key}" 译文为空`);
        continue;
      }
      if (/[\u4e00-\u9fa5]/.test(key)) {
        errors.push(`[ASCII / Markup Gate] ${name}: 精确词典 Key 不得包含汉字 "${key}"`);
      }
      if (/[<>]/.test(value)) {
        errors.push(`[ASCII / Markup Gate] ${name}: 词条 "${key}" 的译文包含危险尖括号字符`);
      }
      if (Object.hasOwn(exact, key)) {
        errors.push(`[Duplicate Key Gate] 跨文件重复定义词条 "${key}" (文件: ${name})`);
      }
      exact[key] = value;
    }
  }

  const patterns = [];
  const sortedPatternFiles = [...patternFiles].sort((a, b) => a.name.localeCompare(b.name));
  for (const { name, raw } of sortedPatternFiles) {
    let list;
    try {
      list = JSON.parse(raw);
    } catch (err) {
      errors.push(`[JSON] ${name}: 解析失败 (${err.message})`);
      continue;
    }
    if (!Array.isArray(list)) {
      errors.push(`[Schema] ${name}: 必须是数组结构`);
      continue;
    }
    list.forEach((item, index) => {
      const loc = `${name}[${index}]`;
      if (!item || typeof item.regex !== 'string' || !item.regex) {
        errors.push(`[Schema] ${loc}: 缺少有效 regex 字符串`);
        return;
      }
      if (typeof item.replacement !== 'string' || item.replacement === '') {
        errors.push(`[Trim & Non-Empty Gate] ${loc}: 缺少有效 replacement`);
        return;
      }
      if (/[<>]/.test(item.replacement)) {
        errors.push(`[ASCII / Markup Gate] ${loc}: replacement 包含危险尖括号字符`);
      }
      const flags = typeof item.flags === 'string' ? item.flags : '';
      if (flags.includes('g')) {
        errors.push(`[Regex Syntax Gate] ${loc}: 不允许在规则中使用全局标志 g`);
      }
      if (
        item.regex.includes('\\\\s') ||
        item.regex.includes('\\\\b') ||
        item.regex.includes('\\\\d') ||
        item.regex.includes('\\\\w')
      ) {
        errors.push(`[Regex Syntax & Double-Escape Gate] ${loc}: 检测到非法双重转义 "${item.regex}"`);
      }
      try {
        new RegExp(item.regex, flags);
      } catch (err) {
        errors.push(`[Regex Syntax Gate] ${loc}: 正则语法错误 (${err.message})`);
        return;
      }

      const captureCount = countCaptureGroups(item.regex);
      const referenced = getReferencedGroups(item.replacement);
      if (referenced.max > captureCount) {
        errors.push(
          `[Capture-Group Invariant Gate] ${loc}: replacement 引用了 $${referenced.max}，但 regex 仅有 ${captureCount} 个捕获组`
        );
      }
      if (captureCount > 0 && referenced.indices.length === 0 && !item.allowDiscard) {
        errors.push(
          `[Capture-Group Invariant Gate] ${loc}: regex 定义了 ${captureCount} 个捕获组但未在 replacement 中引用`
        );
      }
      patterns.push({
        id: item.id || `${name}-${index}`,
        regex: item.regex,
        flags,
        replacement: item.replacement,
      });
    });
  }

  let swarm = null;
  if (swarmFile) {
    try {
      swarm = JSON.parse(swarmFile.raw);
      if (
        !swarm ||
        !Array.isArray(swarm.selectors) ||
        !Array.isArray(swarm.attributeSelectors) ||
        !Array.isArray(swarm.protectedRoots) ||
        !swarm.exact ||
        typeof swarm.exact !== 'object'
      ) {
        errors.push(`[Schema] ${swarmFile.name}: Swarm 词库缺少必要字段`);
      } else {
        for (const dup of findRawJsonDuplicateKeys(swarmFile.raw)) {
          errors.push(`[Duplicate Key Gate] ${swarmFile.name} 存在重复键 "${dup}"`);
        }
        for (const [k, v] of Object.entries(swarm.exact)) {
          if (typeof k !== 'string' || !k || k.trim() !== k) {
            errors.push(`[Trim & Non-Empty Gate] ${swarmFile.name}: 非法键名 "${k}"`);
          }
          if (typeof v !== 'string' || v.trim() === '') {
            errors.push(`[Trim & Non-Empty Gate] ${swarmFile.name}: 词条 "${k}" 译文为空`);
          }
          if (/[\u4e00-\u9fa5]/.test(k)) {
            errors.push(`[ASCII / Markup Gate] ${swarmFile.name}: Key 不得包含汉字 "${k}"`);
          }
          if (typeof v === 'string' && /[<>]/.test(v)) {
            errors.push(`[ASCII / Markup Gate] ${swarmFile.name}: 译文包含危险尖括号 "${k}"`);
          }
        }
        for (const [idx, p] of (swarm.patterns || []).entries()) {
          const loc = `${swarmFile.name}.patterns[${idx}]`;
          if (!p || typeof p.regex !== 'string' || typeof p.replacement !== 'string') {
            errors.push(`[Schema] ${loc}: 无效规则`);
            continue;
          }
          if (/[<>]/.test(p.replacement)) {
            errors.push(`[ASCII / Markup Gate] ${loc}: replacement 包含尖括号`);
          }
          if (
            p.regex.includes('\\\\s') ||
            p.regex.includes('\\\\b') ||
            p.regex.includes('\\\\d') ||
            p.regex.includes('\\\\w')
          ) {
            errors.push(`[Regex Syntax & Double-Escape Gate] ${loc}: 双重转义错误`);
          }
          try {
            new RegExp(p.regex, p.flags || '');
          } catch (err) {
            errors.push(`[Regex Syntax Gate] ${loc}: ${err.message}`);
            continue;
          }
          const captureCount = countCaptureGroups(p.regex);
          const referenced = getReferencedGroups(p.replacement);
          if (referenced.max > captureCount) {
            errors.push(`[Capture-Group Invariant Gate] ${loc}: 越界捕获组 $${referenced.max}`);
          }
          if (captureCount > 0 && referenced.indices.length === 0 && !p.allowDiscard) {
            errors.push(`[Capture-Group Invariant Gate] ${loc}: 存在未消费的捕获组`);
          }
        }
      }
    } catch (err) {
      errors.push(`[JSON] ${swarmFile.name}: 解析失败 (${err.message})`);
    }
  }

  if (errors.length > 0) {
    const err = new Error(`词库质量门禁拦截（共 ${errors.length} 项错误）:\n- ${errors.join('\n- ')}`);
    err.validationErrors = errors;
    throw err;
  }

  const sortedExact = {};
  for (const key of Object.keys(exact).sort()) {
    sortedExact[key] = exact[key];
  }

  let sortedSwarm = null;
  if (swarm) {
    const sortedSwarmExact = {};
    for (const key of Object.keys(swarm.exact).sort()) {
      sortedSwarmExact[key] = swarm.exact[key];
    }
    sortedSwarm = {
      scope: swarm.scope,
      protectedRoots: swarm.protectedRoots,
      selectors: swarm.selectors,
      attributeSelectors: swarm.attributeSelectors,
      exact: sortedSwarmExact,
      patterns: swarm.patterns || [],
    };
  }

  return {
    bundle: {
      version: VERSION,
      exact: sortedExact,
      patterns,
    },
    swarm: sortedSwarm,
  };
}

function stripModuleSyntax(source) {
  // Strip static ES import statements (including multi-line imports) and export keywords cleanly
  let out = source.replace(/^import\s+(?:[\s\S]*?from\s+)?['"][^'"]+['"];?\r?\n?/gm, '');
  out = out.replace(/^export\s+(?=(?:const|let|var|function|class)\b)/gm, '');
  return out.trim();
}

function bundleModulesInOrder(moduleRelPaths) {
  return moduleRelPaths
    .map((rel) => {
      const full = path.join(ROOT, rel);
      const raw = fs.readFileSync(full, 'utf8');
      return `// --- module: ${rel} ---\n${stripModuleSyntax(raw)}`;
    })
    .join('\n\n');
}

function sha256Hex(text) {
  return crypto.createHash('sha256').update(text, 'utf8').digest('hex');
}

function collectJsonFiles(dirRel) {
  const dir = path.join(ROOT, dirRel);
  if (!fs.existsSync(dir)) return [];
  const results = [];
  for (const name of fs.readdirSync(dir).sort()) {
    if (!name.endsWith('.json')) continue;
    const full = path.join(dir, name);
    results.push({
      name: path.join(dirRel, name),
      raw: fs.readFileSync(full, 'utf8'),
    });
  }
  return results;
}

export function buildAllPayloads({ write = true } = {}) {
  const exactFiles = [
    ...collectJsonFiles('locales/src/core'),
    ...collectJsonFiles('locales/src/views'),
  ];
  const patternFiles = collectJsonFiles('locales/src/rules');
  const swarmPath = path.join(ROOT, 'locales/scoped/swarm.json');
  const swarmFile = {
    name: 'locales/scoped/swarm.json',
    raw: fs.readFileSync(swarmPath, 'utf8'),
  };

  const { bundle, swarm } = validateAndCompileLocaleData({
    exactFiles,
    patternFiles,
    swarmFile,
  });

  const bundleJson = JSON.stringify(bundle, null, 2) + '\n';
  const bundleFingerprint = `${VERSION}-${sha256Hex(bundleJson).slice(0, 12)}`;

  const desktopModules = bundleModulesInOrder([
    'src/shared/constants.js',
    'src/shared/translator.js',
    'src/runtime/guards.js',
    'src/runtime/menu.js',
    'src/runtime/scanner.js',
    'src/runtime/floating-ball.js',
    'src/runtime/index.js',
  ]);
  const desktopFingerprint = `${VERSION}-${sha256Hex(bundleJson + desktopModules).slice(0, 12)}`;
  const desktopRuntime =
    `${RUNTIME_MARKER}\n` +
    `(function () {\n` +
    `  'use strict';\n` +
    `${desktopModules}\n\n` +
    `  const __BUNDLE_DATA__ = ${JSON.stringify(bundle)};\n` +
    `  mountDesktopRuntime(__BUNDLE_DATA__, { fingerprint: ${JSON.stringify(desktopFingerprint)} });\n` +
    `})();\n`;

  const swarmModules = bundleModulesInOrder([
    'src/shared/constants.js',
    'src/swarm/swarm-i18n.js',
  ]);
  const swarmJson = JSON.stringify(swarm);
  const swarmFingerprint = `${VERSION}-${sha256Hex(swarmJson + swarmModules).slice(0, 12)}`;
  const swarmRuntime =
    `${SWARM_MARKER}\n` +
    `(function () {\n` +
    `  'use strict';\n` +
    `${swarmModules}\n\n` +
    `  const __SWARM_DATA__ = ${swarmJson};\n` +
    `  mountSwarmRuntime(__SWARM_DATA__, { fingerprint: ${JSON.stringify(swarmFingerprint)} });\n` +
    `})();\n`;

  const mainSwarmHook =
    `// --- Antigravity Chinese Patch Swarm Hook Start ---\n` +
    `;(${installSwarmFrames.toString()})(${JSON.stringify(swarmRuntime)});\n` +
    `// --- Antigravity Chinese Patch Swarm Hook End ---\n`;

  // Syntax check all generated JS artifacts before writing
  new vm.Script(desktopRuntime, { filename: 'dist/desktop-runtime.js' });
  new vm.Script(swarmRuntime, { filename: 'dist/swarm-runtime.js' });
  new vm.Script(mainSwarmHook, { filename: 'dist/main-swarm-hook.js' });

  const outputs = {
    'dist/zh-CN.bundle.json': bundleJson,
    'dist/desktop-runtime.js': desktopRuntime,
    'dist/swarm-runtime.js': swarmRuntime,
    'dist/main-swarm-hook.js': mainSwarmHook,
  };

  if (write) {
    const distDir = path.join(ROOT, 'dist');
    fs.mkdirSync(distDir, { recursive: true });
    for (const [rel, content] of Object.entries(outputs)) {
      fs.writeFileSync(path.join(ROOT, rel), content, 'utf8');
    }
  }

  return {
    exactCount: Object.keys(bundle.exact).length,
    patternCount: bundle.patterns.length,
    swarmExactCount: Object.keys(swarm.exact).length,
    swarmPatternCount: swarm.patterns.length,
    desktopFingerprint,
    swarmFingerprint,
    bundleFingerprint,
    hashes: Object.fromEntries(
      Object.entries(outputs).map(([k, v]) => [k, sha256Hex(v)])
    ),
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const summary = buildAllPayloads({ write: true });
  console.log(JSON.stringify(summary, null, 2));
}
