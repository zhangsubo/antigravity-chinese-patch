#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { createTranslator, normalizeWhitespace } from '../src/shared/translator.js';

const ROOT = path.resolve(import.meta.dirname, '..');

const ALLOWED_EN_IN_ZH = new Set([
  'ai', 'api', 'apis', 'app', 'apps', 'cli', 'ide', 'ui', 'ux', 'url', 'urls', 'uri',
  'mcp', 'lsp', 'ssh', 'wsl', 'git', 'github', 'gitlab', 'http', 'https', 'ws', 'wss',
  'json', 'yaml', 'yml', 'toml', 'csv', 'tsv', 'xml', 'html', 'css', 'js', 'ts', 'mjs',
  'cjs', 'jsx', 'tsx', 'dom', 'cdp', 'sdk', 'ipc', 'rpc', 'grpc', 'tls', 'ssl',
  'oauth', 'adc', 'gcp', 'google', 'gemini', 'live', 'antigravity', 'chrome', 'chromium',
  'macos', 'windows', 'linux', 'finder', 'dock', 'tray', 'zsh', 'bash', 'powershell',
  'cmd', 'env', 'eof', 'pid', 'uid', 'gid', 'sha', 'hash', 'utf', 'ascii', 'ctrl',
  'alt', 'shift', 'enter', 'esc', 'tab', 'space', 'pro', 'ultra', 'flash', 'nano',
  'lite', 'argon', 'carbon', 'turbo', 'default', 'auto', 'swarm', 'btw', 'plan',
  'diff', 'hub', 'sidecar', 'sidecars', 'worktree', 'worktrees', 'citc', 'cider',
  'electron', 'asar', 'node', 'npm', 'npx', 'rust', 'cargo', 'main', 'zip', 'tar', 'dmg',
]);

export function readAsarJsEntries(asarPath) {
  const buf = fs.readFileSync(asarPath);
  if (buf.length < 16) throw new Error('ASAR header is truncated');
  const size = buf.readUInt32LE(0);
  const headerSize = buf.readUInt32LE(4);
  const payloadSize = buf.readUInt32LE(8);
  const jsonSize = buf.readUInt32LE(12);
  if (
    size !== 4 ||
    headerSize < 8 ||
    headerSize > 32 * 1024 * 1024 ||
    payloadSize + 4 !== headerSize ||
    jsonSize > payloadSize - 4 ||
    8 + headerSize > buf.length
  ) {
    throw new Error('Invalid ASAR header');
  }
  const header = JSON.parse(buf.subarray(16, 16 + jsonSize).toString('utf8'));
  const body = buf.subarray(8 + headerSize);
  const results = [];

  function walk(node, prefix) {
    if (node.files) {
      for (const [name, child] of Object.entries(node.files)) {
        walk(child, prefix ? `${prefix}/${name}` : name);
      }
      return;
    }
    if (node.unpacked || node.link || !prefix.endsWith('.js') || prefix.startsWith('node_modules/')) {
      return;
    }
    const offset = Number(node.offset);
    const entrySize = Number(node.size);
    if (!Number.isFinite(offset) || !Number.isFinite(entrySize) || offset < 0 || offset + entrySize > body.length) {
      return;
    }
    results.push({
      name: prefix,
      content: body.subarray(offset, offset + entrySize).toString('utf8'),
    });
  }

  walk(header, '');
  return results;
}

export function isLikelyUIText(s) {
  if (!s || s.length < 2 || s.length > 800) return false;
  if (/^[a-z0-9_\-./\\:]+$/.test(s)) return false;
  if (/^[A-Z0-9_]+$/.test(s) && !s.includes(' ')) return false;
  if (/[{}();=<>|&!~^%$@#*\\]/.test(s)) return false;
  if (s.includes('__') || s.includes('0x') || s.startsWith('.')) return false;
  if (/^(https?:|data:|file:|ws:|wss:|blob:|plugin:)/i.test(s)) return false;
  if (/^M\d/.test(s)) return false;
  if (/^(?:margin|padding|display:|border|background|color:|font-|flex|grid)/.test(s)) return false;
  if (/^[A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+)+$/.test(s)) return false;
  if (/^\d+\.\d+/.test(s) && !s.includes(' ')) return false;
  return /[a-zA-Z]{2}/.test(s) && (/\s/.test(s) || /^[A-Z][a-z]{2,}$/.test(s));
}

export function getUntranslatedResidue(zhText) {
  if (!zhText) return [];
  const cleaned = zhText
    .replace(/`[^`]+`/g, '')
    .replace(/--[a-z0-9-]+/gi, '')
    .replace(/\b[a-z0-9_]+\.[a-z0-9_*]+/gi, '')
    .replace(/\/[a-z0-9_\-/]+/gi, '');
  const words = cleaned.match(/\b[a-zA-Z]{3,}\b/g) || [];
  return words.filter((w) => !ALLOWED_EN_IN_ZH.has(w.toLowerCase()));
}

export function extractStringLiterals(source) {
  const candidates = new Set();
  const strRegex = /"([^"\r\n\\]*(?:\\.[^"\r\n\\]*)*)"|'([^'\r\n\\]*(?:\\.[^'\r\n\\]*)*)'|`([^`\\]*(?:\\.[^`\\]*)*)`/g;
  let match;
  while ((match = strRegex.exec(source)) !== null) {
    const raw = match[1] ?? match[2] ?? match[3] ?? '';
    if (!raw || raw.length > 900 || raw.includes('${')) continue;
    const unescaped = raw
      .replace(/\\n/g, ' ')
      .replace(/\\t/g, ' ')
      .replace(/\\u2026/g, '…')
      .replace(/\\(["'\\])/g, '$1');
    const norm = normalizeWhitespace(unescaped);
    if (isLikelyUIText(norm)) candidates.add(norm);
  }
  return [...candidates];
}

export function analyzeSourceContent(source, translator) {
  const candidates = extractStringLiterals(source);
  const exactMatched = [];
  const ruleMatched = [];
  const falsePositiveResidue = [];
  const untranslated = [];

  for (const text of candidates) {
    if (translator.hasExact(text)) {
      exactMatched.push({ text, translated: translator.translate(text, { normalized: true }) });
      continue;
    }
    const res = translator.translate(text, { normalized: true });
    if (res) {
      const residue = getUntranslatedResidue(res);
      if (residue.length > 0) {
        falsePositiveResidue.push({ text, translated: res, residue });
      } else {
        ruleMatched.push({ text, translated: res });
      }
    } else {
      untranslated.push(text);
    }
  }

  return {
    totalCandidates: candidates.length,
    exactMatched,
    ruleMatched,
    falsePositiveResidue,
    untranslated,
  };
}

export function runDriftScan(targetPath, bundleData) {
  const translator = createTranslator({
    dictionary: bundleData.exact || {},
    patterns: bundleData.patterns || [],
  });

  const stat = fs.statSync(targetPath);
  const sources = [];

  if (stat.isFile() && targetPath.endsWith('.asar')) {
    for (const entry of readAsarJsEntries(targetPath)) {
      sources.push(entry);
    }
  } else if (stat.isFile()) {
    sources.push({ name: path.basename(targetPath), content: fs.readFileSync(targetPath, 'utf8') });
  } else if (stat.isDirectory()) {
    const walk = (dir) => {
      for (const item of fs.readdirSync(dir).sort()) {
        if (item === 'node_modules' || item.startsWith('.')) continue;
        const full = path.join(dir, item);
        const st = fs.statSync(full);
        if (st.isDirectory()) walk(full);
        else if (st.isFile() && item.endsWith('.js')) {
          sources.push({ name: path.relative(targetPath, full), content: fs.readFileSync(full, 'utf8') });
        }
      }
    };
    walk(targetPath);
  } else {
    throw new Error(`不支持的目标路径: ${targetPath}`);
  }

  const combinedSource = sources.map((s) => s.content).join('\n');
  const analysis = analyzeSourceContent(combinedSource, translator);
  const coveredCount = analysis.exactMatched.length + analysis.ruleMatched.length;
  const coverageRatio = analysis.totalCandidates > 0
    ? Number((coveredCount / analysis.totalCandidates).toFixed(4))
    : 0;

  return {
    target: targetPath,
    filesScanned: sources.length,
    dictionaryExactCount: Object.keys(bundleData.exact || {}).length,
    dictionaryPatternCount: (bundleData.patterns || []).length,
    totalCandidates: analysis.totalCandidates,
    exactMatchedCount: analysis.exactMatched.length,
    ruleMatchedCount: analysis.ruleMatched.length,
    falsePositiveCount: analysis.falsePositiveResidue.length,
    untranslatedCount: analysis.untranslated.length,
    heuristicCoverageRatio: coverageRatio,
    falsePositiveSamples: analysis.falsePositiveResidue.slice(0, 50),
    untranslatedSamples: analysis.untranslated.slice(0, 200),
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const args = process.argv.slice(2);
  const target = args[0];
  const reportIdx = args.indexOf('--report');
  const reportPath = reportIdx !== -1 ? args[reportIdx + 1] : null;
  if (!target || target.startsWith('--') || !fs.existsSync(target) || (reportIdx !== -1 && !reportPath)) {
    console.error('用法: node tools/drift-detector.mjs <bundle.js|app.asar|js-dir> [--report <report.json>]');
    process.exit(2);
  }
  const bundlePath = path.join(ROOT, 'dist/zh-CN.bundle.json');
  if (!fs.existsSync(bundlePath)) {
    console.error('缺少 dist/zh-CN.bundle.json，请先执行 npm run build:payloads');
    process.exit(1);
  }
  const bundleData = JSON.parse(fs.readFileSync(bundlePath, 'utf8'));
  const report = runDriftScan(path.resolve(target), bundleData);
  if (reportPath) {
    fs.mkdirSync(path.dirname(path.resolve(reportPath)), { recursive: true });
    fs.writeFileSync(path.resolve(reportPath), JSON.stringify(report, null, 2) + '\n', 'utf8');
  }
  console.log(JSON.stringify({
    target: report.target,
    filesScanned: report.filesScanned,
    totalCandidates: report.totalCandidates,
    exactMatchedCount: report.exactMatchedCount,
    ruleMatchedCount: report.ruleMatchedCount,
    falsePositiveCount: report.falsePositiveCount,
    untranslatedCount: report.untranslatedCount,
    heuristicCoverageRatio: report.heuristicCoverageRatio,
    reportWritten: reportPath || null,
  }, null, 2));
}
