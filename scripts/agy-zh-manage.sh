#!/usr/bin/env bash
set -euo pipefail

# Antigravity Desktop 简体中文汉化工具 (agy-zh) 一键安装与维护脚本
# 兼容 macOS (Bash 3.2+) 与 Linux

SCRIPT_VERSION="3.0.5"
REPO="${AGY_ZH_REPO:-zhangsubo/antigravity-chinese-patch}"
RELEASES_BASE_URL="${AGY_ZH_RELEASES_BASE_URL:-https://github.com/${REPO}/releases}"
API_LATEST_URL="${AGY_ZH_API_LATEST_URL:-https://api.github.com/repos/${REPO}/releases/latest}"

OS_NAME="$(uname -s)"
ARCH_NAME="$(uname -m)"

case "$OS_NAME" in
  Darwin)
    PLATFORM="darwin"
    case "$ARCH_NAME" in
      arm64|aarch64) ASSET_NAME="agy-zh-macos-arm64" ;;
      x86_64) ASSET_NAME="agy-zh-macos-x64" ;;
      *)
        echo "错误: 不支持的 macOS 架构: $ARCH_NAME" >&2
        exit 1
        ;;
    esac
    DEFAULT_CONFIG_DIR="$HOME/Library/Application Support/antigravity-chinese-patch"
    DEFAULT_LOG_FILE="$HOME/Library/Logs/antigravity-chinese-patch.log"
    ;;
  Linux)
    PLATFORM="linux"
    case "$ARCH_NAME" in
      x86_64|amd64) ASSET_NAME="agy-zh-linux-x64" ;;
      aarch64|arm64) ASSET_NAME="agy-zh-linux-arm64" ;;
      *)
        echo "错误: 不支持的 Linux 架构: $ARCH_NAME" >&2
        exit 1
        ;;
    esac
    DEFAULT_CONFIG_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/antigravity-chinese-patch"
    DEFAULT_LOG_FILE="${XDG_RUNTIME_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}}/antigravity-chinese-patch/antigravity-chinese-patch.log"
    ;;
  *)
    echo "错误: 此脚本仅支持 macOS 与 Linux（Windows 请使用 agy-zh-manage.ps1 或 agy-zh-manage.cmd）" >&2
    exit 1
    ;;
esac

CONFIG_DIR="${AGY_ZH_CONFIG_DIR:-$DEFAULT_CONFIG_DIR}"
CONF_FILE="$CONFIG_DIR/manager.conf"
LOG_FILE="${AGY_ZH_LOG_FILE:-$DEFAULT_LOG_FILE}"

SCRIPT_SOURCE="${BASH_SOURCE[0]:-}"
SCRIPT_DIR=""
if [ -n "$SCRIPT_SOURCE" ] && [ -f "$SCRIPT_SOURCE" ]; then
  SCRIPT_DIR="$(cd "$(dirname "$SCRIPT_SOURCE")" && pwd)"
fi

CONF_INSTALL_DIR=""
CONF_MIRROR=""
CONF_APP_PATH=""

CLI_INSTALL_DIR=""
CLI_FROM=""
CLI_VERSION=""
CLI_MIRROR=""
CLI_MIRROR_SET=0
CLI_APP_PATH=""
CLI_MODE=""
ACTION=""
OPT_YES=0
OPT_DELETE_SOURCE=0
OPT_NO_PATH=0
OPT_RESTORE_APP=0
OPT_ACK_SIG=0
OPT_ALLOW_LIB=0
OPT_REPLACE_STALE=0
OPT_REMOVE_SHORTCUT=0

TMP_WORK_DIR=""
LAST_SUMS_FILE=""
LAST_RESOLVED_TAG=""

cleanup() {
  if [ -n "${TMP_WORK_DIR:-}" ] && [ -d "$TMP_WORK_DIR" ]; then
    rm -rf "$TMP_WORK_DIR"
  fi
}
trap cleanup EXIT

ensure_tmp_dir() {
  if [ -z "$TMP_WORK_DIR" ] || [ ! -d "$TMP_WORK_DIR" ]; then
    TMP_WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/agy-zh-manage.XXXXXX")"
  fi
}

usage() {
  cat <<'EOF'
Antigravity 简体中文汉化工具 (agy-zh) 一键安装与维护脚本

用法:
  agy-zh-manage [选项]

主要动作:
  --mode cdp                  一键部署零改包 CDP 动态汉化（推荐，生成快捷方式）
  --mode asar                 一键安装静态 ASAR 安全补丁
  --stop                      停止 CDP 动态守护进程（可配合 --remove-shortcut 移除快捷方式）
  --remove-shortcut           移除「Antigravity 中文」快捷方式
  --update                    在线检查并更新 agy-zh 与本地维护脚本（自动刷新快捷方式）
  --status                    查看本地/云端版本、安装目录、快捷方式与 doctor 诊断摘要
  --logs                      查看最近 50 行 CDP 动态守护日志
  --clear-logs                清空 CDP 动态守护日志
  --restore                   将 Antigravity 还原为官方原版（执行 agy-zh restore）
  --uninstall                 彻底卸载 agy-zh、维护脚本、快捷方式与 PATH 配置

常用选项:
  --install-dir <DIR>         指定并持久化 agy-zh 安装目录（默认 ~/.local/bin）
  --from <FILE>               指定本地已下载的 agy-zh-* 文件进行迁移并重命名
  --version <TAG>             指定下载版本（如 v3.0.5，默认 latest）
  --mirror <PREFIX|none>      配置并持久化 GitHub 下载加速前缀（传 none 清除）
  --app <PATH>                指定并持久化 Antigravity 安装路径
  --delete-source             使用 --from 本地迁移成功后删除源文件
  --no-path                   不修改 shell 配置文件中的 PATH
  --restore-app               配合 --uninstall 使用，卸载前先还原官方原版应用
  --acknowledge-signature-change   静态安装时确认 macOS 签名变化
  --allow-local-library-loading    静态安装时确认 macOS 库校验例外
  --replace-stale-backup      静态安装时自动清理旧版本残留备份
  -y, --yes                   非交互确认（安全签名标志仍需显式传入）
  -h, --help                  显示帮助信息

不带动作参数在终端运行时，将进入中文交互式维护菜单。
EOF
}

log_info() {
  printf '%s\n' "$*"
}

log_warn() {
  printf '警告: %s\n' "$*" >&2
}

log_err() {
  printf '错误: %s\n' "$*" >&2
}

to_lower() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

trim_ws() {
  printf '%s' "$1" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
}

strip_cr_nl() {
  printf '%s' "$1" | tr -d '\r\n'
}

has_tty() {
  [ -t 0 ] || { [ -r /dev/tty ] && [ -w /dev/tty ]; }
}

prompt_read() {
  local prompt_text="$1"
  local var_name="$2"
  local ans=""
  if [ -t 0 ]; then
    printf '%s' "$prompt_text"
    IFS= read -r ans || true
  elif [ -r /dev/tty ] && [ -w /dev/tty ]; then
    printf '%s' "$prompt_text" >/dev/tty
    IFS= read -r ans </dev/tty || true
  else
    ans=""
  fi
  eval "$var_name=\$ans"
}

load_config() {
  CONF_INSTALL_DIR=""
  CONF_MIRROR=""
  CONF_APP_PATH=""
  [ -f "$CONF_FILE" ] || return 0
  local line key val
  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%$'\r'}"
    case "$line" in
      ""|\#*) continue ;;
    esac
    case "$line" in
      *=*)
        key="${line%%=*}"
        val="${line#*=}"
        case "$key" in
          INSTALL_DIR) CONF_INSTALL_DIR="$val" ;;
          MIRROR) CONF_MIRROR="$val" ;;
          APP_PATH) CONF_APP_PATH="$val" ;;
        esac
        ;;
    esac
  done < "$CONF_FILE"
}

save_config() {
  mkdir -p "$CONFIG_DIR"
  local tmp_conf="$CONFIG_DIR/.manager.conf.tmp.$$"
  {
    printf '# agy-zh-manage configuration\n'
    printf 'INSTALL_DIR=%s\n' "$(strip_cr_nl "$INSTALL_DIR")"
    printf 'MIRROR=%s\n' "$(strip_cr_nl "$MIRROR")"
    printf 'APP_PATH=%s\n' "$(strip_cr_nl "$APP_PATH")"
  } > "$tmp_conf"
  mv -f "$tmp_conf" "$CONF_FILE"
}

resolve_effective_settings() {
  load_config
  if [ -n "$CLI_INSTALL_DIR" ]; then
    INSTALL_DIR="$CLI_INSTALL_DIR"
  elif [ -n "${AGY_ZH_INSTALL_DIR:-}" ]; then
    INSTALL_DIR="$AGY_ZH_INSTALL_DIR"
  elif [ -n "$CONF_INSTALL_DIR" ]; then
    INSTALL_DIR="$CONF_INSTALL_DIR"
  else
    INSTALL_DIR="$HOME/.local/bin"
  fi

  # 展开前导 ~/
  case "$INSTALL_DIR" in
    "~") INSTALL_DIR="$HOME" ;;
    "~/"*) INSTALL_DIR="$HOME/${INSTALL_DIR#~/}" ;;
  esac

  if [ "$CLI_MIRROR_SET" -eq 1 ]; then
    MIRROR="$CLI_MIRROR"
  elif [ -n "${AGY_ZH_MIRROR+x}" ]; then
    MIRROR="${AGY_ZH_MIRROR}"
  else
    MIRROR="$CONF_MIRROR"
  fi
  if [ "$(to_lower "$MIRROR")" = "none" ]; then
    MIRROR=""
  fi
  MIRROR="${MIRROR%/}"

  if [ -n "$CLI_APP_PATH" ]; then
    APP_PATH="$CLI_APP_PATH"
  elif [ -n "${AGY_ZH_APP:-}" ]; then
    APP_PATH="$AGY_ZH_APP"
  else
    APP_PATH="$CONF_APP_PATH"
  fi
  case "$APP_PATH" in
    "~") APP_PATH="$HOME" ;;
    "~/"*) APP_PATH="$HOME/${APP_PATH#~/}" ;;
  esac

  TARGET_BIN="$INSTALL_DIR/agy-zh"
  TARGET_SCRIPT="$INSTALL_DIR/agy-zh-manage"
}

apply_mirror_url() {
  local raw_url="$1"
  if [ -z "$MIRROR" ]; then
    printf '%s' "$raw_url"
    return 0
  fi
  case "$raw_url" in
    file://*)
      printf '%s' "$raw_url"
      ;;
    https://github.com/*|https://raw.githubusercontent.com/*|http://github.com/*)
      printf '%s/%s' "$MIRROR" "$raw_url"
      ;;
    *)
      printf '%s' "$raw_url"
      ;;
  esac
}

fetch_url() {
  local raw_url="$1"
  local dest="$2"
  case "$raw_url" in
    file://*)
      local local_path="${raw_url#file://}"
      if [ ! -f "$local_path" ]; then
        log_err "本地测试资源不存在: $local_path"
        return 1
      fi
      cp "$local_path" "$dest"
      return 0
      ;;
  esac

  local target_url
  target_url="$(apply_mirror_url "$raw_url")"
  if command -v curl >/dev/null 2>&1; then
    if curl -fsSL --connect-timeout 15 --max-time 180 -o "$dest" "$target_url"; then
      return 0
    fi
  elif command -v wget >/dev/null 2>&1; then
    if wget -q --timeout=20 -O "$dest" "$target_url"; then
      return 0
    fi
  else
    log_err "未找到 curl 或 wget，无法下载文件"
    return 1
  fi

  log_err "下载失败: $target_url"
  log_err "网络提示: 若直连 GitHub 超时，可配置代理（HTTPS_PROXY）、使用 --mirror <加速前缀>，或手动从 Releases 下载 ${ASSET_NAME} 后使用 --from 本地迁移。"
  return 1
}

compute_sha256() {
  local file="$1"
  local hash=""
  if command -v sha256sum >/dev/null 2>&1; then
    hash="$(sha256sum "$file" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    hash="$(shasum -a 256 "$file" | awk '{print $1}')"
  elif command -v openssl >/dev/null 2>&1; then
    hash="$(openssl dgst -sha256 "$file" | awk '{print $NF}')"
  else
    log_err "系统缺少 sha256sum / shasum / openssl，无法校验文件完整性"
    return 1
  fi
  to_lower "$hash"
}

lookup_sha256_in_sums() {
  local sums_file="$1"
  local asset="$2"
  awk -v target="$asset" '
    NF >= 2 {
      name = $2
      sub(/^\*/, "", name)
      sub(/^\.\//, "", name)
      if (name == target) {
        print tolower($1)
        exit 0
      }
    }
  ' "$sums_file"
}

verify_sha256() {
  local file="$1"
  local sums_file="$2"
  local asset="$3"
  local expected actual
  expected="$(lookup_sha256_in_sums "$sums_file" "$asset")"
  if [ -z "$expected" ]; then
    log_err "SHA256SUMS.txt 中未找到 ${asset} 的校验条目"
    return 1
  fi
  actual="$(compute_sha256 "$file")"
  if [ "$actual" != "$expected" ]; then
    log_err "SHA-256 校验失败: ${asset}"
    log_err "  期望值: ${expected}"
    log_err "  实际值: ${actual}"
    return 1
  fi
  return 0
}

normalize_tag() {
  local raw="$1"
  raw="$(trim_ws "$raw")"
  if [ -z "$raw" ] || [ "$raw" = "latest" ]; then
    printf ''
    return 0
  fi
  case "$raw" in
    v*) printf '%s' "$raw" ;;
    *) printf 'v%s' "$raw" ;;
  esac
}

resolve_latest_tag() {
  if [ -n "$CLI_VERSION" ]; then
    normalize_tag "$CLI_VERSION"
    return 0
  fi

  case "$RELEASES_BASE_URL" in
    file://*)
      local latest_file="${RELEASES_BASE_URL#file://}/LATEST"
      if [ ! -f "$latest_file" ]; then
        log_err "本地测试目录缺少 LATEST 文件: $latest_file"
        return 1
      fi
      normalize_tag "$(head -n 1 "$latest_file")"
      return 0
      ;;
  esac

  local latest_url effective_url tag=""
  latest_url="$(apply_mirror_url "${RELEASES_BASE_URL%/}/latest")"
  if command -v curl >/dev/null 2>&1; then
    effective_url="$(curl -fsSLI --connect-timeout 10 --max-time 20 -o /dev/null -w '%{url_effective}' "$latest_url" 2>/dev/null || true)"
    case "$effective_url" in
      */tag/*)
        tag="${effective_url##*/tag/}"
        tag="${tag%%\?*}"
        tag="${tag%%#*}"
        ;;
    esac
    if [ -z "$tag" ]; then
      local api_json
      api_json="$(curl -fsSL --connect-timeout 10 --max-time 20 "$(apply_mirror_url "$API_LATEST_URL")" 2>/dev/null || true)"
      tag="$(printf '%s' "$api_json" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    fi
  elif command -v wget >/dev/null 2>&1; then
    local api_json
    api_json="$(wget -qO- --timeout=20 "$(apply_mirror_url "$API_LATEST_URL")" 2>/dev/null || true)"
    tag="$(printf '%s' "$api_json" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  fi

  if [ -z "$tag" ]; then
    log_err "无法获取最新 Release 版本号"
    log_err "网络提示: 请检查网络连接、配置 --mirror / HTTPS_PROXY，或通过 --version 指定版本 / --from 本地迁移。"
    return 1
  fi
  normalize_tag "$tag"
}

get_local_version() {
  if [ -x "$TARGET_BIN" ]; then
    local out
    out="$("$TARGET_BIN" --version 2>/dev/null | head -n 1 || true)"
    if [ -n "$out" ]; then
      printf '%s' "${out##* }"
      return 0
    fi
  fi
  printf ''
}

discover_local_asset() {
  if [ -f "$PWD/$ASSET_NAME" ]; then
    printf '%s' "$PWD/$ASSET_NAME"
    return 0
  fi
  if [ -n "$SCRIPT_DIR" ] && [ "$SCRIPT_DIR" != "$PWD" ] && [ -f "$SCRIPT_DIR/$ASSET_NAME" ]; then
    printf '%s' "$SCRIPT_DIR/$ASSET_NAME"
    return 0
  fi
  printf ''
}

shell_rc_file() {
  if [ -n "${AGY_ZH_SHELL_RC:-}" ]; then
    printf '%s' "$AGY_ZH_SHELL_RC"
    return 0
  fi
  if [ "$PLATFORM" = "darwin" ]; then
    printf '%s' "$HOME/.zshrc"
    return 0
  fi
  case "${SHELL:-}" in
    */zsh) printf '%s' "$HOME/.zshrc" ;;
    *) printf '%s' "$HOME/.bashrc" ;;
  esac
}

posix_single_quote() {
  local raw="$1"
  local escaped
  escaped="$(printf '%s' "$raw" | sed "s/'/'\\\\''/g")"
  printf "'%s'" "$escaped"
}

remove_path_block_from_file() {
  local rc_file="$1"
  [ -f "$rc_file" ] || return 0
  local tmp_rc="${rc_file}.agy-zh.tmp.$$"
  awk '
    $0 == "# >>> agy-zh-manage PATH >>>" { skip=1; next }
    $0 == "# <<< agy-zh-manage PATH <<<" { skip=0; next }
    !skip { print }
  ' "$rc_file" > "$tmp_rc"
  mv -f "$tmp_rc" "$rc_file"
}

ensure_path_configured() {
  if [ "$OPT_NO_PATH" -eq 1 ]; then
    return 0
  fi
  local rc_file
  rc_file="$(shell_rc_file)"
  mkdir -p "$(dirname "$rc_file")"
  remove_path_block_from_file "$rc_file"

  local quoted_dir
  quoted_dir="$(posix_single_quote "$INSTALL_DIR")"
  {
    if [ -s "$rc_file" ] && [ -n "$(tail -n 1 "$rc_file" 2>/dev/null || true)" ]; then
      printf '\n'
    fi
    printf '# >>> agy-zh-manage PATH >>>\n'
    printf 'case ":$PATH:" in\n'
    printf '  *:%s:*) ;;\n' "$quoted_dir"
    printf '  *) export PATH=%s:"$PATH" ;;\n' "$quoted_dir"
    printf 'esac\n'
    printf '# <<< agy-zh-manage PATH <<<\n'
  } >> "$rc_file"

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) export PATH="$INSTALL_DIR:$PATH" ;;
  esac
}

install_binary_from_staged() {
  local staged_src="$1"
  mkdir -p "$INSTALL_DIR"
  local tmp_bin="$INSTALL_DIR/.agy-zh.tmp.$$"
  cp "$staged_src" "$tmp_bin"
  chmod 755 "$tmp_bin"
  if [ "$PLATFORM" = "darwin" ] && command -v xattr >/dev/null 2>&1; then
    xattr -d com.apple.quarantine "$tmp_bin" 2>/dev/null || true
  fi

  local ver_check=""
  if ! ver_check="$("$tmp_bin" --version 2>/dev/null | head -n 1)"; then
    rm -f "$tmp_bin"
    log_err "待安装的可执行文件自检失败（无法执行 --version），已中止以避免覆盖现有版本"
    return 1
  fi

  if [ -x "$TARGET_BIN" ]; then
    "$TARGET_BIN" stop >/dev/null 2>&1 || true
  fi

  mv -f "$tmp_bin" "$TARGET_BIN"
  log_info "已安装 agy-zh (${ver_check}) -> ${TARGET_BIN}"
}

install_from_local_file() {
  local src_file="$1"
  if [ ! -f "$src_file" ]; then
    log_err "找不到本地文件: $src_file"
    return 1
  fi
  local src_dir src_base sums_candidate
  src_dir="$(cd "$(dirname "$src_file")" && pwd)"
  src_base="$(basename "$src_file")"
  sums_candidate="$src_dir/SHA256SUMS.txt"
  if [ -f "$sums_candidate" ]; then
    if [ -n "$(lookup_sha256_in_sums "$sums_candidate" "$src_base")" ]; then
      log_info "检测到同目录 SHA256SUMS.txt，正在校验 ${src_base}..."
      verify_sha256 "$src_file" "$sums_candidate" "$src_base"
      log_info "SHA-256 校验通过。"
    fi
  fi

  install_binary_from_staged "$src_file"
  save_config

  local abs_src="$src_dir/$src_base"
  local abs_dst
  abs_dst="$(cd "$INSTALL_DIR" && pwd)/agy-zh"
  if [ "$abs_src" != "$abs_dst" ]; then
    if [ "$OPT_DELETE_SOURCE" -eq 1 ]; then
      rm -f "$src_file"
      log_info "已清理源文件: $src_file"
    elif [ "$OPT_YES" -eq 0 ] && has_tty; then
      local del_ans=""
      prompt_read "是否删除原下载文件 ${src_file}？[y/N]: " del_ans
      case "$(to_lower "$del_ans")" in
        y|yes)
          rm -f "$src_file"
          log_info "已删除源文件: $src_file"
          ;;
      esac
    fi
  fi
}

install_from_release() {
  ensure_tmp_dir
  local tag
  tag="$(resolve_latest_tag)"
  LAST_RESOLVED_TAG="$tag"
  local base_dl="${RELEASES_BASE_URL%/}/download/${tag}"
  local sums_tmp="$TMP_WORK_DIR/SHA256SUMS.txt"
  local bin_tmp="$TMP_WORK_DIR/$ASSET_NAME"

  log_info "正在从 Release ${tag} 下载校验清单与 ${ASSET_NAME}..."
  fetch_url "${base_dl}/SHA256SUMS.txt" "$sums_tmp"
  fetch_url "${base_dl}/${ASSET_NAME}" "$bin_tmp"
  verify_sha256 "$bin_tmp" "$sums_tmp" "$ASSET_NAME"
  log_info "SHA-256 校验通过。"

  LAST_SUMS_FILE="$sums_tmp"
  install_binary_from_staged "$bin_tmp"
  save_config
}

sync_manage_script() {
  mkdir -p "$INSTALL_DIR"
  if [ -n "$LAST_SUMS_FILE" ] && [ -f "$LAST_SUMS_FILE" ] && [ -n "$(lookup_sha256_in_sums "$LAST_SUMS_FILE" "agy-zh-manage.sh")" ]; then
    local expected_hash current_hash=""
    expected_hash="$(lookup_sha256_in_sums "$LAST_SUMS_FILE" "agy-zh-manage.sh")"
    if [ -f "$TARGET_SCRIPT" ]; then
      current_hash="$(compute_sha256 "$TARGET_SCRIPT")"
    fi
    if [ "$current_hash" != "$expected_hash" ]; then
      ensure_tmp_dir
      local script_tmp="$TMP_WORK_DIR/agy-zh-manage.sh"
      local base_dl="${RELEASES_BASE_URL%/}/download/${LAST_RESOLVED_TAG}"
      if fetch_url "${base_dl}/agy-zh-manage.sh" "$script_tmp" && verify_sha256 "$script_tmp" "$LAST_SUMS_FILE" "agy-zh-manage.sh"; then
        local staged="$INSTALL_DIR/.agy-zh-manage.tmp.$$"
        cp "$script_tmp" "$staged"
        chmod 755 "$staged"
        if [ "$PLATFORM" = "darwin" ] && command -v xattr >/dev/null 2>&1; then
          xattr -d com.apple.quarantine "$staged" 2>/dev/null || true
        fi
        mv -f "$staged" "$TARGET_SCRIPT"
        log_info "已同步维护脚本 -> ${TARGET_SCRIPT}"
        return 0
      fi
    else
      return 0
    fi
  fi

  if [ -n "$SCRIPT_SOURCE" ] && [ -f "$SCRIPT_SOURCE" ]; then
    local src_abs dst_abs
    src_abs="$(cd "$(dirname "$SCRIPT_SOURCE")" && pwd)/$(basename "$SCRIPT_SOURCE")"
    dst_abs="$(cd "$INSTALL_DIR" && pwd)/agy-zh-manage"
    if [ "$src_abs" != "$dst_abs" ]; then
      local staged="$INSTALL_DIR/.agy-zh-manage.tmp.$$"
      cp "$SCRIPT_SOURCE" "$staged"
      chmod 755 "$staged"
      if [ "$PLATFORM" = "darwin" ] && command -v xattr >/dev/null 2>&1; then
        xattr -d com.apple.quarantine "$staged" 2>/dev/null || true
      fi
      mv -f "$staged" "$TARGET_SCRIPT"
      log_info "已安装维护脚本 -> ${TARGET_SCRIPT}"
    fi
  fi
}

detect_existing_shortcut_path() {
  if [ -n "${AGY_ZH_SHORTCUT_DIR:-}" ]; then
    if [ "$PLATFORM" = "darwin" ]; then
      if [ -d "$AGY_ZH_SHORTCUT_DIR/Antigravity 中文.app" ]; then
        printf '%s' "$AGY_ZH_SHORTCUT_DIR/Antigravity 中文.app"
        return 0
      fi
    else
      if [ -f "$AGY_ZH_SHORTCUT_DIR/antigravity-zh.desktop" ]; then
        printf '%s' "$AGY_ZH_SHORTCUT_DIR/antigravity-zh.desktop"
        return 0
      fi
    fi
    return 1
  fi

  if [ "$PLATFORM" = "darwin" ]; then
    if [ -d "/Applications/Antigravity 中文.app" ]; then
      printf '%s' "/Applications/Antigravity 中文.app"
      return 0
    elif [ -d "$HOME/Applications/Antigravity 中文.app" ]; then
      printf '%s' "$HOME/Applications/Antigravity 中文.app"
      return 0
    fi
  else
    local desktop_file="${XDG_DATA_HOME:-$HOME/.local/share}/applications/antigravity-zh.desktop"
    if [ -f "$desktop_file" ]; then
      printf '%s' "$desktop_file"
      return 0
    fi
  fi
  return 1
}

run_agy_shortcut() {
  local remove_flag="${1:-0}"
  local args=()
  if [ -n "$APP_PATH" ]; then
    args+=(--app "$APP_PATH")
  fi
  if [ -n "${AGY_ZH_SHORTCUT_DIR:-}" ]; then
    args+=(--output "$AGY_ZH_SHORTCUT_DIR")
  fi
  if [ "$remove_flag" -eq 1 ]; then
    args+=(--remove)
  fi
  if [ "${#args[@]}" -gt 0 ]; then
    "$TARGET_BIN" shortcut "${args[@]}"
  else
    "$TARGET_BIN" shortcut
  fi
}

refresh_existing_shortcut() {
  local sc_path=""
  if sc_path="$(detect_existing_shortcut_path 2>/dev/null)"; then
    log_info "检测到已存在快捷方式 (${sc_path})，正在刷新内嵌稳定副本..."
    if run_agy_shortcut 0 >/dev/null; then
      log_info "已刷新快捷方式 -> ${sc_path}"
    else
      log_warn "快捷方式刷新失败，请手动执行 agy-zh shortcut"
    fi
  fi
}

ensure_binary_installed() {
  if [ -x "$TARGET_BIN" ] && [ -z "$CLI_FROM" ] && [ -z "$CLI_VERSION" ]; then
    return 0
  fi
  if [ -n "$CLI_FROM" ]; then
    install_from_local_file "$CLI_FROM"
    sync_manage_script
    ensure_path_configured
    refresh_existing_shortcut
    return 0
  fi

  local local_candidate=""
  if [ "$OPT_YES" -eq 0 ] && [ -z "$CLI_VERSION" ] && has_tty; then
    local_candidate="$(discover_local_asset)"
  fi

  if [ -n "$local_candidate" ]; then
    log_info "检测到本地安装包: ${local_candidate}"
    log_info "  1. 迁移并重命名本地文件为 ${TARGET_BIN}（推荐）"
    log_info "  2. 从 GitHub Releases 在线下载最新版"
    local src_choice=""
    prompt_read "请选择来源 [1/2，默认 1]: " src_choice
    case "$src_choice" in
      2)
        install_from_release
        ;;
      *)
        install_from_local_file "$local_candidate"
        ;;
    esac
  else
    install_from_release
  fi
  sync_manage_script
  ensure_path_configured
  refresh_existing_shortcut
}

run_doctor_json() {
  if [ -n "$APP_PATH" ]; then
    "$TARGET_BIN" doctor --app "$APP_PATH"
  else
    "$TARGET_BIN" doctor
  fi
}

json_get_str() {
  local json="$1"
  local key="$2"
  printf '%s' "$json" | sed -n "s/.*\"${key}\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" | head -n 1
}

json_get_bool() {
  local json="$1"
  local key="$2"
  printf '%s' "$json" | sed -n "s/.*\"${key}\"[[:space:]]*:[[:space:]]*\(true\|false\).*/\1/p" | head -n 1
}

do_install_or_update() {
  local force_online="${1:-0}"
  if [ -n "$CLI_FROM" ]; then
    install_from_local_file "$CLI_FROM"
  elif [ "$force_online" -eq 1 ] || [ -n "$CLI_VERSION" ]; then
    install_from_release
  else
    local local_candidate=""
    if [ "$OPT_YES" -eq 0 ] && has_tty; then
      local_candidate="$(discover_local_asset)"
    fi
    if [ -n "$local_candidate" ]; then
      log_info "检测到本地安装包: ${local_candidate}"
      log_info "  1. 迁移并重命名本地文件为 ${TARGET_BIN}（推荐）"
      log_info "  2. 从 GitHub Releases 在线下载最新版"
      local src_choice=""
      prompt_read "请选择来源 [1/2，默认 1]: " src_choice
      case "$src_choice" in
        2) install_from_release ;;
        *) install_from_local_file "$local_candidate" ;;
      esac
    else
      install_from_release
    fi
  fi
  sync_manage_script
  ensure_path_configured
  refresh_existing_shortcut
}

do_mode_cdp() {
  ensure_binary_installed
  local doc_json=""
  if doc_json="$(run_doctor_json 2>/dev/null)"; then
    local patched
    patched="$(json_get_bool "$doc_json" "patched")"
    if [ "$patched" = "true" ]; then
      log_warn "检测到当前 Antigravity 已安装静态 ASAR 补丁，建议先还原官方原版再切换 CDP 动态模式。"
      if [ "$OPT_YES" -eq 0 ] && has_tty; then
        local ans=""
        prompt_read "是否立即执行 agy-zh restore 还原官方原版？[Y/n]: " ans
        case "$(to_lower "$ans")" in
          n|no)
            log_err "用户取消切换，请先运行 restore 后再试"
            return 1
            ;;
          *)
            if [ -n "$APP_PATH" ]; then
              "$TARGET_BIN" restore --app "$APP_PATH"
            else
              "$TARGET_BIN" restore
            fi
            ;;
        esac
      else
        log_err "请先执行 agy-zh-manage --restore 还原官方原版后再启用 CDP 模式"
        return 1
      fi
    fi
  fi

  log_info "正在生成「Antigravity 中文」快捷方式..."
  run_agy_shortcut 0

  if [ "$OPT_YES" -eq 0 ] && has_tty; then
    local launch_ans=""
    prompt_read "快捷方式已就绪，是否立即以动态汉化模式启动 Antigravity？[y/N]: " launch_ans
    case "$(to_lower "$launch_ans")" in
      y|yes)
        if [ -n "$APP_PATH" ]; then
          "$TARGET_BIN" run --app "$APP_PATH"
        else
          "$TARGET_BIN" run
        fi
        ;;
    esac
  fi
}

do_mode_asar() {
  ensure_binary_installed
  local doc_json="" stale_ver=""
  if doc_json="$(run_doctor_json 2>/dev/null)"; then
    stale_ver="$(json_get_str "$doc_json" "stale_backup_version")"
  fi

  local ack_sig="$OPT_ACK_SIG"
  local allow_lib="$OPT_ALLOW_LIB"
  local replace_stale="$OPT_REPLACE_STALE"

  if [ "$PLATFORM" = "darwin" ]; then
    if [ "$ack_sig" -eq 0 ] || [ "$allow_lib" -eq 0 ]; then
      if has_tty && [ "$OPT_YES" -eq 0 ]; then
        log_info ""
        log_info "安全提示: 静态补丁将修改 Antigravity.app 内置 ASAR，外层签名将变更为本地 ad-hoc 签名并附加库校验例外。"
        local confirm=""
        prompt_read "请先完全退出 Antigravity (Cmd+Q)，输入 yes 确认继续静态安装: " confirm
        if [ "$confirm" != "yes" ]; then
          log_err "用户取消静态 ASAR 安装"
          return 1
        fi
        ack_sig=1
        allow_lib=1
      else
        log_err "macOS 非交互静态安装必须显式传入 --acknowledge-signature-change --allow-local-library-loading"
        return 1
      fi
    fi
  fi

  if [ -n "$stale_ver" ] && [ "$replace_stale" -eq 0 ]; then
    log_warn "检测到旧版本残留备份 (${stale_ver})。"
    if has_tty && [ "$OPT_YES" -eq 0 ]; then
      local rep_ans=""
      prompt_read "是否自动替换旧版本备份并继续安装？[Y/n]: " rep_ans
      case "$(to_lower "$rep_ans")" in
        n|no) ;;
        *) replace_stale=1 ;;
      esac
    fi
  fi

  local args=(install)
  if [ -n "$APP_PATH" ]; then
    args+=(--app "$APP_PATH")
  fi
  if [ "$ack_sig" -eq 1 ]; then
    args+=(--acknowledge-signature-change)
  fi
  if [ "$allow_lib" -eq 1 ]; then
    args+=(--allow-local-library-loading)
  fi
  if [ "$replace_stale" -eq 1 ]; then
    args+=(--replace-stale-backup)
  fi

  "$TARGET_BIN" "${args[@]}"
}

do_status() {
  local local_ver remote_ver="" sc_path=""
  local_ver="$(get_local_version)"
  if [ -z "$local_ver" ]; then
    local_ver="未安装"
  fi

  if remote_ver="$(resolve_latest_tag 2>/dev/null)"; then
    :
  else
    remote_ver="获取失败（离线或网络受限）"
  fi

  if sc_path="$(detect_existing_shortcut_path 2>/dev/null)"; then
    :
  else
    sc_path="未生成"
  fi

  log_info "=== agy-zh 状态总览 ==="
  log_info "安装目录:       ${INSTALL_DIR}"
  log_info "可执行文件:     ${TARGET_BIN}"
  log_info "维护脚本:       ${TARGET_SCRIPT}"
  log_info "本地版本:       ${local_ver}"
  log_info "云端最新版本:   ${remote_ver}"
  log_info "下载加速镜像:   ${MIRROR:-直连 GitHub (无镜像)}"
  log_info "目标应用路径:   ${APP_PATH:-自动探测}"
  log_info "快捷方式状态:   ${sc_path}"
  log_info "守护日志路径:   ${LOG_FILE}"

  if [ -x "$TARGET_BIN" ]; then
    log_info ""
    log_info "=== Antigravity 诊断摘要 (agy-zh doctor) ==="
    local doc_json=""
    if doc_json="$(run_doctor_json 2>&1)"; then
      local app_disp app_ver compat_mode patched backup cdp_running
      app_disp="$(json_get_str "$doc_json" "app")"
      app_ver="$(json_get_str "$doc_json" "version")"
      compat_mode="$(json_get_str "$doc_json" "compatibility_mode")"
      patched="$(json_get_bool "$doc_json" "patched")"
      backup="$(json_get_str "$doc_json" "backup")"
      cdp_running="$(json_get_bool "$doc_json" "cdp_daemon_running")"
      log_info "应用目录:       ${app_disp}"
      log_info "客户端版本:     ${app_ver} (${compat_mode})"
      log_info "静态补丁状态:   ${patched} (备份: ${backup})"
      log_info "CDP 守护运行中: ${cdp_running}"
    else
      log_warn "doctor 检查未通过: ${doc_json}"
    fi
  fi
}

do_logs() {
  if [ ! -f "$LOG_FILE" ]; then
    log_info "暂无守护日志: ${LOG_FILE}"
    return 0
  fi
  log_info "=== 最近 50 行日志 (${LOG_FILE}) ==="
  tail -n 50 "$LOG_FILE"
}

do_clear_logs() {
  if [ -f "$LOG_FILE" ]; then
    : > "$LOG_FILE"
    log_info "已清空日志: ${LOG_FILE}"
  else
    log_info "日志文件不存在: ${LOG_FILE}"
  fi
}

do_remove_shortcut() {
  if [ ! -x "$TARGET_BIN" ]; then
    log_err "未找到 ${TARGET_BIN}，请先安装 agy-zh"
    return 1
  fi
  local sc_path=""
  if sc_path="$(detect_existing_shortcut_path 2>/dev/null)"; then
    run_agy_shortcut 1
    log_info "已移除快捷方式: ${sc_path}"
  else
    run_agy_shortcut 1 >/dev/null 2>&1 || true
    log_info "未检测到已生成的中文快捷方式。"
  fi
}

do_stop_cdp() {
  if [ ! -x "$TARGET_BIN" ]; then
    log_err "未找到 ${TARGET_BIN}，请先安装 agy-zh"
    return 1
  fi
  "$TARGET_BIN" stop
  if [ "$OPT_REMOVE_SHORTCUT" -eq 1 ]; then
    do_remove_shortcut
  elif [ "$OPT_YES" -eq 0 ] && has_tty; then
    local sc_path=""
    if sc_path="$(detect_existing_shortcut_path 2>/dev/null)"; then
      local rm_ans=""
      prompt_read "是否同时移除「Antigravity 中文」快捷方式 (${sc_path})？[y/N]: " rm_ans
      case "$(to_lower "$rm_ans")" in
        y|yes)
          run_agy_shortcut 1
          log_info "已移除快捷方式: ${sc_path}"
          ;;
      esac
    fi
  fi
  log_info "提示: CDP 动态模式不修改官方原包；完全退出 Antigravity 后直接启动原版图标即为官方英文原版。"
}

do_restore() {
  if [ ! -x "$TARGET_BIN" ]; then
    log_err "未找到 ${TARGET_BIN}，请先安装 agy-zh"
    return 1
  fi
  if [ -n "$APP_PATH" ]; then
    "$TARGET_BIN" restore --app "$APP_PATH"
  else
    "$TARGET_BIN" restore
  fi
}

do_uninstall() {
  if [ -x "$TARGET_BIN" ]; then
    "$TARGET_BIN" stop >/dev/null 2>&1 || true

    local sc_path=""
    if sc_path="$(detect_existing_shortcut_path 2>/dev/null)"; then
      log_info "正在移除快捷方式: ${sc_path}"
      run_agy_shortcut 1 >/dev/null 2>&1 || rm -rf "$sc_path"
    fi

    local should_restore="$OPT_RESTORE_APP"
    local doc_json="" patched=""
    if [ "$should_restore" -eq 0 ] && doc_json="$(run_doctor_json 2>/dev/null)"; then
      patched="$(json_get_bool "$doc_json" "patched")"
      if [ "$patched" = "true" ] && [ "$OPT_YES" -eq 0 ] && has_tty; then
        local r_ans=""
        prompt_read "检测到 Antigravity 仍保留静态补丁，卸载前是否先还原为官方原版？[Y/n]: " r_ans
        case "$(to_lower "$r_ans")" in
          n|no) ;;
          *) should_restore=1 ;;
        esac
      fi
    fi

    if [ "$should_restore" -eq 1 ]; then
      log_info "正在还原 Antigravity 官方原版..."
      do_restore || log_warn "还原应用失败，将继续清理本地工具文件"
    fi
  fi

  local rc_file
  rc_file="$(shell_rc_file)"
  if [ "$OPT_NO_PATH" -eq 0 ] && [ -f "$rc_file" ]; then
    remove_path_block_from_file "$rc_file"
    log_info "已从 ${rc_file} 移除 PATH 配置"
  fi

  rm -f "$TARGET_BIN" "$TARGET_SCRIPT" "$CONF_FILE"
  log_info "已完成彻底卸载（自定义词表与日志如存在未主动删除）。"
}

interactive_configure_settings() {
  log_info ""
  log_info "=== 目录与下载源设置 ==="
  log_info "当前安装目录: ${INSTALL_DIR}"
  log_info "当前加速镜像: ${MIRROR:-直连 GitHub}"
  log_info "当前应用路径: ${APP_PATH:-自动探测}"
  log_info ""
  local new_dir="" new_mirror="" new_app=""
  prompt_read "输入新的安装目录（留空保持不变）: " new_dir
  if [ -n "$new_dir" ]; then
    case "$new_dir" in
      "~") new_dir="$HOME" ;;
      "~/"*) new_dir="$HOME/${new_dir#~/}" ;;
    esac
    if [ "$new_dir" != "$INSTALL_DIR" ]; then
      mkdir -p "$new_dir"
      if [ -f "$TARGET_BIN" ]; then
        cp "$TARGET_BIN" "$new_dir/agy-zh"
        chmod 755 "$new_dir/agy-zh"
        rm -f "$TARGET_BIN"
      fi
      if [ -f "$TARGET_SCRIPT" ]; then
        cp "$TARGET_SCRIPT" "$new_dir/agy-zh-manage"
        chmod 755 "$new_dir/agy-zh-manage"
        rm -f "$TARGET_SCRIPT"
      fi
      INSTALL_DIR="$new_dir"
      TARGET_BIN="$INSTALL_DIR/agy-zh"
      TARGET_SCRIPT="$INSTALL_DIR/agy-zh-manage"
      ensure_path_configured
    fi
  fi

  prompt_read "输入 GitHub 加速镜像前缀（输入 none 恢复直连，留空保持不变）: " new_mirror
  if [ -n "$new_mirror" ]; then
    if [ "$(to_lower "$new_mirror")" = "none" ]; then
      MIRROR=""
    else
      MIRROR="${new_mirror%/}"
    fi
  fi

  prompt_read "输入 Antigravity 应用路径（输入 auto 恢复自动探测，留空保持不变）: " new_app
  if [ -n "$new_app" ]; then
    if [ "$(to_lower "$new_app")" = "auto" ]; then
      APP_PATH=""
    else
      case "$new_app" in
        "~") new_app="$HOME" ;;
        "~/"*) new_app="$HOME/${new_app#~/}" ;;
      esac
      APP_PATH="$new_app"
    fi
  fi

  save_config
  log_info "设置已保存至 ${CONF_FILE}"
}

interactive_menu() {
  while true; do
    resolve_effective_settings
    local local_ver
    local_ver="$(get_local_version)"
    log_info ""
    log_info "================================================"
    log_info "  Antigravity 简体中文汉化 一键维护工具 (v${SCRIPT_VERSION})"
    log_info "================================================"
    log_info "  安装位置: ${INSTALL_DIR}"
    log_info "  当前版本: ${local_ver:-未安装} | 镜像源: ${MIRROR:-直连 GitHub}"
    log_info "------------------------------------------------"
    log_info "  1. 一键部署 CDP 动态汉化（推荐：不改原包 + 快捷方式）"
    log_info "  2. 停止 CDP 动态守护 / 移除中文快捷方式"
    log_info "  3. 一键安装静态 ASAR 安全补丁（含原生右键/托盘菜单）"
    log_info "  4. 还原 Antigravity 为官方原版（静态补丁 restore）"
    log_info "  5. 安装 / 迁移本地文件 / 在线更新 agy-zh"
    log_info "  6. 查看状态总览与诊断报告"
    log_info "  7. 查看或清空 CDP 守护日志"
    log_info "  8. 设置安装目录 / 下载镜像源 / 应用路径"
    log_info "  9. 彻底卸载 agy-zh 与本维护脚本"
    log_info "  0. 退出"
    log_info ""

    local choice=""
    prompt_read "请输入编号 [0-9]: " choice
    case "$choice" in
      1) do_mode_cdp || true ;;
      2) do_stop_cdp || true ;;
      3) do_mode_asar || true ;;
      4) do_restore || true ;;
      5) do_install_or_update 0 || true ;;
      6) do_status || true ;;
      7)
        do_logs || true
        local clr=""
        prompt_read "是否清空日志文件？[y/N]: " clr
        case "$(to_lower "$clr")" in
          y|yes) do_clear_logs || true ;;
        esac
        ;;
      8) interactive_configure_settings || true ;;
      9)
        local confirm=""
        prompt_read "确认彻底卸载 agy-zh 与维护脚本？[y/N]: " confirm
        case "$(to_lower "$confirm")" in
          y|yes)
            do_uninstall
            return 0
            ;;
        esac
        ;;
      0|"")
        return 0
        ;;
      *)
        log_warn "无效编号: $choice"
        ;;
    esac
  done
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --install-dir)
        [ "$#" -ge 2 ] || { log_err "--install-dir 缺少参数"; exit 1; }
        CLI_INSTALL_DIR="$2"
        shift 2
        ;;
      --from)
        [ "$#" -ge 2 ] || { log_err "--from 缺少参数"; exit 1; }
        CLI_FROM="$2"
        ACTION="${ACTION:-install}"
        shift 2
        ;;
      --version)
        [ "$#" -ge 2 ] || { log_err "--version 缺少参数"; exit 1; }
        CLI_VERSION="$2"
        ACTION="${ACTION:-install}"
        shift 2
        ;;
      --mirror)
        [ "$#" -ge 2 ] || { log_err "--mirror 缺少参数"; exit 1; }
        CLI_MIRROR="$2"
        CLI_MIRROR_SET=1
        shift 2
        ;;
      --app)
        [ "$#" -ge 2 ] || { log_err "--app 缺少参数"; exit 1; }
        CLI_APP_PATH="$2"
        shift 2
        ;;
      --mode)
        [ "$#" -ge 2 ] || { log_err "--mode 缺少参数"; exit 1; }
        CLI_MODE="$(to_lower "$2")"
        case "$CLI_MODE" in
          cdp|asar) ACTION="mode_${CLI_MODE}" ;;
          *)
            log_err "不支持的模式: $2（仅支持 cdp 或 asar）"
            exit 1
            ;;
        esac
        shift 2
        ;;
      --update)
        ACTION="update"
        shift
        ;;
      --stop)
        ACTION="stop"
        shift
        ;;
      --remove-shortcut)
        OPT_REMOVE_SHORTCUT=1
        ACTION="${ACTION:-remove_shortcut}"
        shift
        ;;
      --status)
        ACTION="status"
        shift
        ;;
      --logs)
        ACTION="logs"
        shift
        ;;
      --clear-logs)
        ACTION="clear_logs"
        shift
        ;;
      --restore)
        ACTION="restore"
        shift
        ;;
      --uninstall)
        ACTION="uninstall"
        shift
        ;;
      --restore-app)
        OPT_RESTORE_APP=1
        shift
        ;;
      --delete-source)
        OPT_DELETE_SOURCE=1
        shift
        ;;
      --no-path)
        OPT_NO_PATH=1
        shift
        ;;
      --acknowledge-signature-change)
        OPT_ACK_SIG=1
        shift
        ;;
      --allow-local-library-loading)
        OPT_ALLOW_LIB=1
        shift
        ;;
      --replace-stale-backup)
        OPT_REPLACE_STALE=1
        shift
        ;;
      -y|--yes)
        OPT_YES=1
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        log_err "未知参数: $1"
        usage
        exit 1
        ;;
    esac
  done
}

main() {
  parse_args "$@"
  resolve_effective_settings

  # 若用户单独传入了持久化设置参数，先落盘保存
  if [ -n "$CLI_INSTALL_DIR" ] || [ "$CLI_MIRROR_SET" -eq 1 ] || [ -n "$CLI_APP_PATH" ]; then
    if [ "$ACTION" != "uninstall" ]; then
      save_config
    fi
  fi

  case "$ACTION" in
    install)
      do_install_or_update 0
      ;;
    update)
      do_install_or_update 1
      ;;
    mode_cdp)
      do_mode_cdp
      ;;
    mode_asar)
      do_mode_asar
      ;;
    status)
      do_status
      ;;
    logs)
      do_logs
      ;;
    clear_logs)
      do_clear_logs
      ;;
    restore)
      do_restore
      ;;
    stop)
      do_stop_cdp
      ;;
    remove_shortcut)
      do_remove_shortcut
      ;;
    uninstall)
      do_uninstall
      ;;
    "")
      if has_tty && [ "$OPT_YES" -eq 0 ]; then
        interactive_menu
      else
        do_install_or_update 0
        log_info "基础安装已完成，后续可运行 agy-zh-manage 打开维护菜单。"
      fi
      ;;
  esac
}

main "$@"
