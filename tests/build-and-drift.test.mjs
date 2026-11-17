import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import struct from 'node:buffer';
import {
  buildAllPayloads,
  validateAndCompileLocaleData,
} from '../tools/build-payloads.mjs';
import { runDriftScan } from '../tools/drift-detector.mjs';

void struct;

function makeTestAsar(files) {
  const headerFiles = {};
  const chunks = [];
  let offset = 0;
  for (const [rel, rawStr] of Object.entries(files)) {
    const data = Buffer.from(rawStr, 'utf8');
    const parts = rel.split('/');
    let cursor = headerFiles;
    for (const part of parts.slice(0, -1)) {
      cursor[part] = cursor[part] || { files: {} };
      cursor = cursor[part].files;
    }
    cursor[parts[parts.length - 1]] = {
      size: data.length,
      offset: String(offset),
    };
    chunks.push(data);
    offset += data.length;
  }
  const rawJson = Buffer.from(JSON.stringify({ files: headerFiles }), 'utf8');
  const pad = Buffer.alloc((4 - (rawJson.length % 4)) % 4);
  const headerPickle = Buffer.alloc(8 + rawJson.length + pad.length);
  headerPickle.writeUInt32LE(4 + rawJson.length + pad.length, 0);
  headerPickle.writeUInt32LE(rawJson.length, 4);
  rawJson.copy(headerPickle, 8);
  pad.copy(headerPickle, 8 + rawJson.length);

  const sizeHeader = Buffer.alloc(8);
  sizeHeader.writeUInt32LE(4, 0);
  sizeHeader.writeUInt32LE(headerPickle.length, 4);
  return Buffer.concat([sizeHeader, headerPickle, ...chunks]);
}

test('buildAllPayloads is deterministic across repeated builds', () => {
  const first = buildAllPayloads({ write: false });
  const second = buildAllPayloads({ write: false });
  assert.deepEqual(first.hashes, second.hashes);
  assert.ok(first.exactCount >= 1873);
  assert.ok(first.swarmExactCount >= 140);
});

test('compiler quality gates reject untrimmed, duplicate, markup, CJK key, double-escaped, and mismatched capture groups', () => {
  const baseSwarm = {
    name: 'swarm.json',
    raw: JSON.stringify({
      scope: 'test',
      protectedRoots: [],
      selectors: [],
      attributeSelectors: [],
      exact: { Ok: '确定' },
      patterns: [],
    }),
  };

  // Gate 1: untrimmed key
  assert.throws(
    () =>
      validateAndCompileLocaleData({
        exactFiles: [{ name: 'a.json', raw: '{" Settings ":"设置"}' }],
        patternFiles: [],
        swarmFile: baseSwarm,
      }),
    /Trim & Non-Empty Gate/
  );

  // Gate 2: duplicate key across files
  assert.throws(
    () =>
      validateAndCompileLocaleData({
        exactFiles: [
          { name: 'a.json', raw: '{"Settings":"设置"}' },
          { name: 'b.json', raw: '{"Settings":"配置"}' },
        ],
        patternFiles: [],
        swarmFile: baseSwarm,
      }),
    /Duplicate Key Gate/
  );

  // Gate 3: HTML markup in value + CJK in exact key
  assert.throws(
    () =>
      validateAndCompileLocaleData({
        exactFiles: [{ name: 'a.json', raw: '{"Settings":"<b>设置</b>","设置":"Settings"}' }],
        patternFiles: [],
        swarmFile: baseSwarm,
      }),
    /ASCII \/ Markup Gate/
  );

  // Gate 4: double-escaped regex fragment
  assert.throws(
    () =>
      validateAndCompileLocaleData({
        exactFiles: [{ name: 'a.json', raw: '{"Settings":"设置"}' }],
        patternFiles: [
          {
            name: 'rules.json',
            raw: JSON.stringify([{ regex: '^(\\\\d+) items$', replacement: '$1 项' }]),
          },
        ],
        swarmFile: baseSwarm,
      }),
    /Double-Escape Gate/
  );

  // Gate 5: out-of-bounds capture group $2 when regex has 1 group
  assert.throws(
    () =>
      validateAndCompileLocaleData({
        exactFiles: [{ name: 'a.json', raw: '{"Settings":"设置"}' }],
        patternFiles: [
          {
            name: 'rules.json',
            raw: JSON.stringify([{ regex: '^(\\d+) items$', replacement: '$1 / $2 项' }]),
          },
        ],
        swarmFile: baseSwarm,
      }),
    /Capture-Group Invariant Gate/
  );
});

test('drift detector scans synthetic JS bundle and in-memory ASAR archive', () => {
  const bundle = JSON.parse(fs.readFileSync('dist/zh-CN.bundle.json', 'utf8'));
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'agy-drift-'));
  try {
    const jsPath = path.join(tmpDir, 'sample-ui.js');
    fs.writeFileSync(
      jsPath,
      'const a = "Settings"; const b = "3 files changed"; const c = "Brand New Unseen Option";\n',
      'utf8'
    );
    const jsReport = runDriftScan(jsPath, bundle);
    assert.equal(jsReport.exactMatchedCount, 1);
    assert.equal(jsReport.ruleMatchedCount, 1);
    assert.equal(jsReport.untranslatedCount, 1);
    assert.deepEqual(jsReport.untranslatedSamples, ['Brand New Unseen Option']);

    const asarPath = path.join(tmpDir, 'sample.asar');
    fs.writeFileSync(
      asarPath,
      makeTestAsar({
        'dist/main.js': 'const x = "Global Permissions"; const y = "Unmapped Feature Label";\n',
      })
    );
    const asarReport = runDriftScan(asarPath, bundle);
    assert.equal(asarReport.filesScanned, 1);
    assert.equal(asarReport.exactMatchedCount, 1);
    assert.equal(asarReport.untranslatedCount, 1);
    assert.deepEqual(asarReport.untranslatedSamples, ['Unmapped Feature Label']);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
