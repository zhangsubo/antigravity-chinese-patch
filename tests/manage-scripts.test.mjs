import { test } from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..');
const SH_SCRIPT = path.join(REPO_ROOT, 'scripts', 'agy-zh-manage.sh');
const PS1_SCRIPT = path.join(REPO_ROOT, 'scripts', 'agy-zh-manage.ps1');
const CMD_SCRIPT = path.join(REPO_ROOT, 'scripts', 'agy-zh-manage.cmd');

function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function unixAssetName() {
  if (process.platform === 'darwin') {
    return process.arch === 'arm64' ? 'agy-zh-macos-arm64' : 'agy-zh-macos-x64';
  }
  return process.arch === 'arm64' ? 'agy-zh-linux-arm64' : 'agy-zh-linux-x64';
}

function windowsAssetName() {
  return process.arch === 'arm64' ? 'agy-zh-windows-arm64.exe' : 'agy-zh-windows-x64.exe';
}

function createMockUnixBinary(filePath, opts = {}) {
  const version = opts.version || '3.0.5';
  const patched = opts.patched ? 'true' : 'false';
  const staleVer = opts.staleBackupVersion ? `"${opts.staleBackupVersion}"` : 'null';
  const script = `#!/usr/bin/env bash
set -eu
if [ -n "\${MOCK_LOG_FILE:-}" ]; then
  printf '%s\\n' "$*" >> "\${MOCK_LOG_FILE}"
fi
cmd="\${1:-}"
case "$cmd" in
  --version)
    printf 'agy-zh %s\\n' '${version}'
    ;;
  doctor)
    printf '{"tool_version":"%s","payload_sha256":"abc","app":"/mock/Antigravity","platform":"%s","version":"2.35.2","compatibility_mode":"verified","patched":%s,"asar_sha256":"def","translations":2000,"backup":"none","state_present":false,"native_ui_tested":false,"main_swarm_hook":false,"scope":"all","signature_effect":"none","stale_backup_version":%s,"cdp_daemon_running":false}\\n' '${version}' '${process.platform}' '${patched}' '${staleVer}'
    ;;
  shortcut)
    shift || true
    remove_mode=0
    out_dir="\${AGY_ZH_SHORTCUT_DIR:-}"
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --remove) remove_mode=1; shift ;;
        --output) out_dir="$2"; shift 2 ;;
        --app) shift 2 ;;
        *) shift ;;
      esac
    done
    if [ -n "$out_dir" ]; then
      if [ "$(uname -s)" = "Darwin" ]; then
        sc="$out_dir/Antigravity 中文.app"
        if [ "$remove_mode" -eq 1 ]; then
          rm -rf "$sc"
        else
          mkdir -p "$sc/Contents"
        fi
      else
        sc="$out_dir/antigravity-zh.desktop"
        if [ "$remove_mode" -eq 1 ]; then
          rm -f "$sc"
        else
          mkdir -p "$out_dir"
          printf '[Desktop Entry]\\n' > "$sc"
        fi
      fi
    fi
    printf '{"action":"ok"}\\n'
    ;;
  stop|restore|install|run)
    printf '{"ok":true,"cmd":"%s"}\\n' "$cmd"
    ;;
  *)
    exit 0
    ;;
esac
`;
  fs.writeFileSync(filePath, script, 'utf8');
  fs.chmodSync(filePath, 0o755);
}

test('management scripts satisfy encoding and line-ending invariants', () => {
  const shBuf = fs.readFileSync(SH_SCRIPT);
  assert.equal(shBuf.includes(Buffer.from('\r\n')), false, 'agy-zh-manage.sh must use LF');

  const ps1Buf = fs.readFileSync(PS1_SCRIPT);
  assert.deepEqual(
    [ps1Buf[0], ps1Buf[1], ps1Buf[2]],
    [0xef, 0xbb, 0xbf],
    'agy-zh-manage.ps1 must start with UTF-8 BOM for PowerShell 5.1',
  );

  const cmdBuf = fs.readFileSync(CMD_SCRIPT);
  for (let i = 0; i < cmdBuf.length; i += 1) {
    assert.ok(cmdBuf[i] <= 0x7f, `agy-zh-manage.cmd byte ${i} must be pure ASCII`);
    if (cmdBuf[i] === 0x0a) {
      assert.equal(cmdBuf[i - 1], 0x0d, 'agy-zh-manage.cmd must use CRLF');
    }
  }
});

if (process.platform !== 'win32') {
  test('agy-zh-manage.sh passes bash -n syntax check', () => {
    const res = spawnSync('bash', ['-n', SH_SCRIPT], { encoding: 'utf8' });
    assert.equal(res.status, 0, res.stderr);
  });

  test('agy-zh-manage.sh handles local migration, custom dir, PATH block, file:// update with SHA-256, modes, logs, and uninstall', () => {
    const sandbox = fs.mkdtempSync(path.join(os.tmpdir(), 'agy-zh-manage-unix-'));
    try {
      const configDir = path.join(sandbox, 'config');
      const customInstallDir = path.join(sandbox, '自定义 目录', 'bin');
      const shortcutDir = path.join(sandbox, 'shortcuts');
      const shellRc = path.join(sandbox, '.zshrc');
      const logFile = path.join(sandbox, 'agy-zh.log');
      const mockLog = path.join(sandbox, 'mock-calls.log');
      const downloadDir = path.join(sandbox, 'downloads');
      fs.mkdirSync(downloadDir, { recursive: true });
      fs.mkdirSync(shortcutDir, { recursive: true });
      fs.writeFileSync(shellRc, 'export EXISTING_VAR=1\n', 'utf8');
      fs.writeFileSync(logFile, 'line1\nline2\n', 'utf8');

      const asset = unixAssetName();
      const localSrc = path.join(downloadDir, asset);
      createMockUnixBinary(localSrc, { version: '3.0.5', patched: false });
      fs.writeFileSync(
        path.join(downloadDir, 'SHA256SUMS.txt'),
        `${sha256File(localSrc)}  ${asset}\n`,
        'utf8',
      );

      const baseEnv = {
        ...process.env,
        AGY_ZH_CONFIG_DIR: configDir,
        AGY_ZH_SHORTCUT_DIR: shortcutDir,
        AGY_ZH_SHELL_RC: shellRc,
        AGY_ZH_LOG_FILE: logFile,
        MOCK_LOG_FILE: mockLog,
      };

      // 1. Local migration with --install-dir, --from, --delete-source
      let res = spawnSync(
        'bash',
        [SH_SCRIPT, '--install-dir', customInstallDir, '--from', localSrc, '--delete-source', '--yes'],
        { env: baseEnv, encoding: 'utf8' },
      );
      assert.equal(res.status, 0, res.stderr || res.stdout);
      const installedBin = path.join(customInstallDir, 'agy-zh');
      const installedMgr = path.join(customInstallDir, 'agy-zh-manage');
      assert.equal(fs.existsSync(installedBin), true);
      assert.equal(fs.existsSync(installedMgr), true);
      assert.equal(fs.existsSync(localSrc), false, 'source binary should be deleted');
      const rcText = fs.readFileSync(shellRc, 'utf8');
      assert.ok(rcText.includes('# >>> agy-zh-manage PATH >>>'));
      assert.ok(rcText.includes(customInstallDir));

      const confText = fs.readFileSync(path.join(configDir, 'manager.conf'), 'utf8');
      assert.ok(confText.includes(`INSTALL_DIR=${customInstallDir}`));

      // 2. Mock file:// release + existing shortcut refresh + self-update
      const releaseRoot = path.join(sandbox, 'mock-releases');
      const v309Dir = path.join(releaseRoot, 'download', 'v3.0.9');
      fs.mkdirSync(v309Dir, { recursive: true });
      fs.writeFileSync(path.join(releaseRoot, 'LATEST'), 'v3.0.9\n', 'utf8');

      const v309Bin = path.join(v309Dir, asset);
      createMockUnixBinary(v309Bin, { version: '3.0.9', patched: false });
      const v309Mgr = path.join(v309Dir, 'agy-zh-manage.sh');
      fs.copyFileSync(SH_SCRIPT, v309Mgr);
      fs.writeFileSync(
        path.join(v309Dir, 'SHA256SUMS.txt'),
        `${sha256File(v309Bin)}  ${asset}\n${sha256File(v309Mgr)}  agy-zh-manage.sh\n`,
        'utf8',
      );

      // Pre-create shortcut to verify update refreshes it automatically
      if (process.platform === 'darwin') {
        fs.mkdirSync(path.join(shortcutDir, 'Antigravity 中文.app', 'Contents'), { recursive: true });
      } else {
        fs.writeFileSync(path.join(shortcutDir, 'antigravity-zh.desktop'), '[Desktop Entry]\n', 'utf8');
      }

      const releaseEnv = {
        ...baseEnv,
        AGY_ZH_RELEASES_BASE_URL: pathToFileURL(releaseRoot).href,
      };
      res = spawnSync('bash', [installedMgr, '--update', '--yes'], {
        env: releaseEnv,
        encoding: 'utf8',
      });
      assert.equal(res.status, 0, res.stderr || res.stdout);
      const verCheck = spawnSync(installedBin, ['--version'], { encoding: 'utf8' });
      assert.equal(verCheck.stdout.trim(), 'agy-zh 3.0.9');
      const callsAfterUpdate = fs.readFileSync(mockLog, 'utf8');
      assert.ok(callsAfterUpdate.includes('stop'), 'update should stop old daemon');
      assert.ok(callsAfterUpdate.includes('shortcut'), 'update should refresh existing shortcut');

      // 3. Tampered release artifact must fail SHA-256 check and leave 3.0.9 untouched
      fs.writeFileSync(v309Bin, '#!/bin/sh\necho tampered\n', 'utf8');
      res = spawnSync('bash', [installedMgr, '--update', '--yes'], {
        env: releaseEnv,
        encoding: 'utf8',
      });
      assert.notEqual(res.status, 0, 'tampered binary must fail SHA-256 check');
      const verAfterTamper = spawnSync(installedBin, ['--version'], { encoding: 'utf8' });
      assert.equal(verAfterTamper.stdout.trim(), 'agy-zh 3.0.9');

      // 4. Mode CDP & ASAR checks
      res = spawnSync('bash', [installedMgr, '--mode', 'cdp', '--yes'], {
        env: releaseEnv,
        encoding: 'utf8',
      });
      assert.equal(res.status, 0, res.stderr || res.stdout);

      if (process.platform === 'darwin') {
        res = spawnSync('bash', [installedMgr, '--mode', 'asar', '--yes'], {
          env: releaseEnv,
          encoding: 'utf8',
        });
        assert.notEqual(res.status, 0, 'macOS non-interactive ASAR install without safety flags must fail');

        res = spawnSync(
          'bash',
          [
            installedMgr,
            '--mode',
            'asar',
            '--acknowledge-signature-change',
            '--allow-local-library-loading',
            '--yes',
          ],
          { env: releaseEnv, encoding: 'utf8' },
        );
        assert.equal(res.status, 0, res.stderr || res.stdout);
      } else {
        res = spawnSync('bash', [installedMgr, '--mode', 'asar', '--yes'], {
          env: releaseEnv,
          encoding: 'utf8',
        });
        assert.equal(res.status, 0, res.stderr || res.stdout);
      }

      // 5. Status & Logs & ClearLogs
      res = spawnSync('bash', [installedMgr, '--status'], { env: releaseEnv, encoding: 'utf8' });
      assert.equal(res.status, 0, res.stderr || res.stdout);
      assert.ok(res.stdout.includes('3.0.9'));

      res = spawnSync('bash', [installedMgr, '--logs'], { env: releaseEnv, encoding: 'utf8' });
      assert.equal(res.status, 0);
      assert.ok(res.stdout.includes('line2'));

      res = spawnSync('bash', [installedMgr, '--clear-logs'], { env: releaseEnv, encoding: 'utf8' });
      assert.equal(res.status, 0);
      assert.equal(fs.readFileSync(logFile, 'utf8'), '');

      // 6. Uninstall cleans binaries, script, config, shortcut, and PATH block
      res = spawnSync('bash', [installedMgr, '--uninstall', '--yes'], {
        env: releaseEnv,
        encoding: 'utf8',
      });
      assert.equal(res.status, 0, res.stderr || res.stdout);
      assert.equal(fs.existsSync(installedBin), false);
      assert.equal(fs.existsSync(installedMgr), false);
      assert.equal(fs.existsSync(path.join(configDir, 'manager.conf')), false);
      const rcAfterUninstall = fs.readFileSync(shellRc, 'utf8');
      assert.equal(rcAfterUninstall.includes('agy-zh-manage PATH'), false);
      assert.ok(rcAfterUninstall.includes('export EXISTING_VAR=1'));
    } finally {
      fs.rmSync(sandbox, { recursive: true, force: true });
    }
  });
}

if (process.platform === 'win32') {
  function compileMockWindowsExe(outExe, version) {
    const csCode = `
using System;
using System.IO;
class Program {
  static int Main(string[] args) {
    string logFile = Environment.GetEnvironmentVariable("MOCK_LOG_FILE");
    if (!string.IsNullOrEmpty(logFile)) {
      File.AppendAllText(logFile, string.Join(" ", args) + Environment.NewLine);
    }
    string cmd = args.Length > 0 ? args[0] : "";
    if (cmd == "--version") {
      Console.WriteLine("agy-zh ${version}");
      return 0;
    }
    if (cmd == "doctor") {
      Console.WriteLine("{\\"tool_version\\":\\"${version}\\",\\"payload_sha256\\":\\"abc\\",\\"app\\":\\"C:\\\\\\\\Mock\\\\\\\\Antigravity\\",\\"platform\\":\\"win32\\",\\"version\\":\\"2.35.2\\",\\"compatibility_mode\\":\\"verified\\",\\"patched\\":false,\\"asar_sha256\\":\\"def\\",\\"translations\\":2000,\\"backup\\":\\"none\\",\\"state_present\\":false,\\"native_ui_tested\\":false,\\"main_swarm_hook\\":false,\\"scope\\":\\"all\\",\\"signature_effect\\":\\"none\\",\\"cdp_daemon_running\\":false}");
      return 0;
    }
    if (cmd == "shortcut") {
      bool remove = Array.IndexOf(args, "--remove") >= 0;
      string scDir = Environment.GetEnvironmentVariable("AGY_ZH_SHORTCUT_DIR");
      if (!string.IsNullOrEmpty(scDir)) {
        Directory.CreateDirectory(scDir);
        string lnk = Path.Combine(scDir, "Antigravity 中文.lnk");
        if (remove) {
          if (File.Exists(lnk)) File.Delete(lnk);
        } else {
          File.WriteAllText(lnk, "mock-lnk");
        }
      }
      Console.WriteLine("{\\"action\\":\\"ok\\"}");
      return 0;
    }
    return 0;
  }
}
`;
    const psCmd = `Add-Type -TypeDefinition @'\n${csCode}\n'@ -OutputAssembly '${outExe.replace(/'/g, "''")}' -OutputType ConsoleApplication`;
    const res = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', psCmd], {
      encoding: 'utf8',
    });
    assert.equal(res.status, 0, res.stderr || res.stdout);
  }

  test('agy-zh-manage.ps1 passes PowerShell parser check', () => {
    const parseCmd = `$errors = $null; $null = [System.Management.Automation.Language.Parser]::ParseFile('${PS1_SCRIPT.replace(/'/g, "''")}', [ref]$null, [ref]$errors); if ($errors.Count -gt 0) { $errors | ForEach-Object { Write-Error $_.Message }; exit 1 }`;
    const res = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', parseCmd], {
      encoding: 'utf8',
    });
    assert.equal(res.status, 0, res.stderr || res.stdout);
  });

  test('agy-zh-manage.ps1 handles Windows local migration, file:// update with SHA-256, CDP mode, status, and uninstall', () => {
    const sandbox = fs.mkdtempSync(path.join(os.tmpdir(), 'agy-zh-manage-win-'));
    try {
      const configDir = path.join(sandbox, 'config');
      const customInstallDir = path.join(sandbox, '中文 目录', 'bin');
      const shortcutDir = path.join(sandbox, 'Desktop');
      const logFile = path.join(sandbox, 'agy-zh.log');
      const mockLog = path.join(sandbox, 'mock-calls.log');
      const downloadDir = path.join(sandbox, 'downloads');
      fs.mkdirSync(downloadDir, { recursive: true });
      fs.mkdirSync(shortcutDir, { recursive: true });
      fs.writeFileSync(logFile, 'win-line1\n', 'utf8');

      const asset = windowsAssetName();
      const localSrc = path.join(downloadDir, asset);
      compileMockWindowsExe(localSrc, '3.0.5');
      fs.writeFileSync(
        path.join(downloadDir, 'SHA256SUMS.txt'),
        `${sha256File(localSrc)}  ${asset}\n`,
        'utf8',
      );

      const baseEnv = {
        ...process.env,
        AGY_ZH_CONFIG_DIR: configDir,
        AGY_ZH_SHORTCUT_DIR: shortcutDir,
        AGY_ZH_LOG_FILE: logFile,
        AGY_ZH_SKIP_USER_PATH: '1',
        MOCK_LOG_FILE: mockLog,
      };

      // 1. Local migration
      let res = spawnSync(
        'powershell.exe',
        [
          '-NoProfile',
          '-ExecutionPolicy',
          'Bypass',
          '-File',
          PS1_SCRIPT,
          '-InstallDir',
          customInstallDir,
          '-From',
          localSrc,
          '-DeleteSource',
          '-NoPath',
          '-Yes',
        ],
        { env: baseEnv, encoding: 'utf8' },
      );
      assert.equal(res.status, 0, res.stderr || res.stdout);
      const installedBin = path.join(customInstallDir, 'agy-zh.exe');
      const installedPs1 = path.join(customInstallDir, 'agy-zh-manage.ps1');
      const installedCmd = path.join(customInstallDir, 'agy-zh-manage.cmd');
      assert.equal(fs.existsSync(installedBin), true);
      assert.equal(fs.existsSync(installedPs1), true);
      assert.equal(fs.existsSync(installedCmd), true);
      assert.equal(fs.existsSync(localSrc), false);

      // 2. Mock file:// release update + shortcut refresh
      const releaseRoot = path.join(sandbox, 'mock-releases');
      const v309Dir = path.join(releaseRoot, 'download', 'v3.0.9');
      fs.mkdirSync(v309Dir, { recursive: true });
      fs.writeFileSync(path.join(releaseRoot, 'LATEST'), 'v3.0.9\n', 'utf8');

      const v309Bin = path.join(v309Dir, asset);
      compileMockWindowsExe(v309Bin, '3.0.9');
      const v309Ps1 = path.join(v309Dir, 'agy-zh-manage.ps1');
      const v309Cmd = path.join(v309Dir, 'agy-zh-manage.cmd');
      fs.copyFileSync(PS1_SCRIPT, v309Ps1);
      fs.copyFileSync(CMD_SCRIPT, v309Cmd);
      fs.writeFileSync(
        path.join(v309Dir, 'SHA256SUMS.txt'),
        `${sha256File(v309Bin)}  ${asset}\n${sha256File(v309Ps1)}  agy-zh-manage.ps1\n${sha256File(v309Cmd)}  agy-zh-manage.cmd\n`,
        'utf8',
      );
      fs.writeFileSync(path.join(shortcutDir, 'Antigravity 中文.lnk'), 'old-lnk', 'utf8');

      const releaseEnv = {
        ...baseEnv,
        AGY_ZH_RELEASES_BASE_URL: pathToFileURL(releaseRoot).href,
      };
      res = spawnSync(
        'powershell.exe',
        ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', installedPs1, '-Update', '-NoPath', '-Yes'],
        { env: releaseEnv, encoding: 'utf8' },
      );
      assert.equal(res.status, 0, res.stderr || res.stdout);
      const verCheck = spawnSync(installedBin, ['--version'], { encoding: 'utf8' });
      assert.equal(verCheck.stdout.trim(), 'agy-zh 3.0.9');

      // 3. Tampered release artifact fails SHA-256
      fs.writeFileSync(v309Bin, 'tampered', 'utf8');
      res = spawnSync(
        'powershell.exe',
        ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', installedPs1, '-Update', '-NoPath', '-Yes'],
        { env: releaseEnv, encoding: 'utf8' },
      );
      assert.notEqual(res.status, 0);
      assert.equal(spawnSync(installedBin, ['--version'], { encoding: 'utf8' }).stdout.trim(), 'agy-zh 3.0.9');

      // 4. Mode CDP + Status + Uninstall
      res = spawnSync(
        'powershell.exe',
        ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', installedPs1, '-Mode', 'cdp', '-NoPath', '-Yes'],
        { env: releaseEnv, encoding: 'utf8' },
      );
      assert.equal(res.status, 0, res.stderr || res.stdout);

      res = spawnSync(
        'powershell.exe',
        ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', installedPs1, '-Status'],
        { env: releaseEnv, encoding: 'utf8' },
      );
      assert.equal(res.status, 0, res.stderr || res.stdout);

      res = spawnSync(
        'powershell.exe',
        ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', installedPs1, '-Uninstall', '-NoPath', '-Yes'],
        { env: releaseEnv, encoding: 'utf8' },
      );
      assert.equal(res.status, 0, res.stderr || res.stdout);
      assert.equal(fs.existsSync(installedBin), false);
      assert.equal(fs.existsSync(installedPs1), false);
      assert.equal(fs.existsSync(installedCmd), false);
      assert.equal(fs.existsSync(path.join(configDir, 'manager.json')), false);
    } finally {
      fs.rmSync(sandbox, { recursive: true, force: true });
    }
  });
}
