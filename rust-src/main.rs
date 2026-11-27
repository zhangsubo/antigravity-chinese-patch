mod asar;
mod cdp;
mod compatibility;
mod custom_dict;
mod daemon;
mod passkeys;
mod patcher;
mod platform;
mod shortcut;

use clap::{CommandFactory, Parser, Subcommand};
use custom_dict::load_custom_translations;
use daemon::{run_cdp, stop_daemon, RunOptions};
use patcher::{doctor_app, install_app, restore_app, InstallOptions, TOOL_VERSION};
use platform::{app_platform, default_app_path, is_inside_agent_session, Platform};
use shortcut::manage_shortcut;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "agy-zh",
    version = TOOL_VERSION,
    about = "Dual-mode Simplified Chinese localization tool for Google Antigravity Desktop"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Inspect installed Antigravity Desktop app, backup state, and compatibility
    Doctor {
        #[arg(long)]
        app: Option<PathBuf>,
    },
    /// Install static ASAR localization patch (supports native dialogs & Swarm)
    Install {
        #[arg(long)]
        app: Option<PathBuf>,
        #[arg(long)]
        acknowledge_signature_change: bool,
        #[arg(long)]
        allow_local_library_loading: bool,
        #[arg(long)]
        replace_stale_backup: bool,
        #[arg(long)]
        custom_translations: Option<PathBuf>,
        #[arg(long)]
        trust_custom_translations: bool,
    },
    /// Restore original Antigravity Desktop app and signature from backup
    Restore {
        #[arg(long)]
        app: Option<PathBuf>,
    },
    /// Launch Antigravity Desktop with zero-modification CDP dynamic localization
    Run {
        #[arg(long)]
        app: Option<PathBuf>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, hide = true)]
        foreground: bool,
        #[arg(long)]
        custom_translations: Option<PathBuf>,
        #[arg(long)]
        trust_custom_translations: bool,
    },
    /// Stop background CDP dynamic localization daemon
    Stop,
    /// Create or remove desktop / application launcher shortcut for CDP mode
    Shortcut {
        #[arg(long)]
        app: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, hide = true)]
        bin_dir: Option<PathBuf>,
        #[arg(long)]
        remove: bool,
    },
}

#[cfg(windows)]
fn init_console_utf8() {
    use windows_sys::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};
    unsafe {
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);
    }
}

#[cfg(not(windows))]
fn init_console_utf8() {}

fn char_display_width(ch: char) -> usize {
    if ch.is_control() {
        return 0;
    }
    let cp = ch as u32;
    let wide = matches!(
        cp,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6B
            | 0xFF01..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x20000..=0x3FFFD
    );
    if wide {
        2
    } else {
        1
    }
}

fn str_display_width(s: &str) -> usize {
    s.chars().map(char_display_width).sum()
}

fn wrap_display_lines(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let max_w = max_width.max(20);
    for raw_para in text.split('\n') {
        let para = raw_para.trim_end_matches('\r');
        if para.is_empty() {
            lines.push(String::new());
            continue;
        }
        let chars: Vec<char> = para.chars().collect();
        let mut cur = String::new();
        let mut cur_w = 0usize;
        for (idx, &ch) in chars.iter().enumerate() {
            let w = char_display_width(ch);
            if cur_w + w > max_w && !cur.is_empty() {
                if ch.is_ascii_alphanumeric() {
                    if let Some(space_pos) = cur.rfind(' ') {
                        let tail = &cur[space_pos + 1..];
                        if !tail.is_empty() && str_display_width(tail) < 18 {
                            let carried = tail.to_string();
                            cur.truncate(space_pos);
                            lines.push(cur.trim_end().to_string());
                            cur = carried;
                            cur_w = str_display_width(&cur);
                        }
                    }
                }
                if cur_w + w > max_w && !cur.is_empty() {
                    lines.push(cur.trim_end().to_string());
                    cur.clear();
                    cur_w = 0;
                }
            }
            if cur.is_empty() && ch == ' ' {
                continue;
            }
            cur.push(ch);
            cur_w += w;
            if ch == '；' && cur_w >= max_w / 3 && idx + 1 < chars.len() {
                lines.push(cur);
                cur = String::new();
                cur_w = 0;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn format_stopped_message(err: &str, use_box: bool) -> String {
    if !use_box {
        return format!("Stopped: {err}");
    }

    const BOX_MAX_CONTENT_WIDTH: usize = 66;
    const HEADER: &str = "[!] 操作已暂停 (Stopped)";
    let body_lines = wrap_display_lines(err, BOX_MAX_CONTENT_WIDTH);
    let inner_width = body_lines
        .iter()
        .map(|l| str_display_width(l))
        .max()
        .unwrap_or(0)
        .max(str_display_width(HEADER));
    let border = "*".repeat(inner_width + 4);

    let mut out = Vec::with_capacity(body_lines.len() + 5);
    out.push(border.clone());

    let header_pad = " ".repeat(inner_width.saturating_sub(str_display_width(HEADER)));
    out.push(format!("* {HEADER}{header_pad} *"));
    out.push(format!("* {} *", " ".repeat(inner_width)));

    for line in body_lines {
        let pad = " ".repeat(inner_width.saturating_sub(str_display_width(&line)));
        out.push(format!("* {line}{pad} *"));
    }

    out.push(border);
    out.join("\n")
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let out = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize output JSON: {e}"))?;
    println!("{out}");
    Ok(())
}

fn dispatch_command(cmd: Commands) -> Result<(), String> {
    match cmd {
        Commands::Doctor { app } => {
            let target = app.unwrap_or_else(default_app_path);
            let report = doctor_app(&target)?;
            print_json(&report)
        }
        Commands::Install {
            app,
            acknowledge_signature_change,
            allow_local_library_loading,
            replace_stale_backup,
            custom_translations,
            trust_custom_translations,
        } => {
            let target = app.unwrap_or_else(default_app_path);
            let platform = app_platform(&target);
            let custom_dict = load_custom_translations(
                custom_translations.as_deref(),
                platform,
                trust_custom_translations,
            )?;
            let report = install_app(
                &target,
                InstallOptions {
                    acknowledge_signature_change,
                    allow_local_library_loading,
                    replace_stale_backup,
                },
                &custom_dict,
            )?;
            print_json(&report)
        }
        Commands::Restore { app } => {
            let target = app.unwrap_or_else(default_app_path);
            let report = restore_app(&target)?;
            print_json(&report)
        }
        Commands::Run {
            app,
            port,
            foreground,
            custom_translations,
            trust_custom_translations,
        } => {
            let report = run_cdp(RunOptions {
                app,
                port,
                foreground,
                custom_translations,
                trust_custom_translations,
            })?;
            print_json(&report)
        }
        Commands::Stop => {
            let report = stop_daemon()?;
            print_json(&report)
        }
        Commands::Shortcut {
            app,
            output,
            bin_dir,
            remove,
        } => {
            let report = manage_shortcut(app.as_deref(), output.as_deref(), bin_dir.as_deref(), remove)?;
            print_json(&report)
        }
    }
}

fn prompt_line(prompt: &str) -> Result<String, String> {
    print!("{prompt}");
    io::stdout()
        .flush()
        .map_err(|e| format!("Failed to flush stdout: {e}"))?;
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read input: {e}"))?;
    Ok(line.trim().to_string())
}

fn run_interactive_menu() -> Result<(), String> {
    let default_target = default_app_path();
    println!("Antigravity Desktop 简体中文汉化工具 v{TOOL_VERSION}");
    println!("目标应用: {}", default_target.display());
    if is_inside_agent_session() {
        println!("提示: 当前运行在智能体会话环境内，请勿强杀宿主客户端。");
    }
    println!();
    println!("请选择操作:");
    println!("  1. 零改包动态汉化启动 (agy-zh run)");
    println!("  2. 生成「Antigravity 中文」快捷方式 (agy-zh shortcut)");
    println!("  3. 停止后台动态守护进程 (agy-zh stop)");
    println!("  4. 诊断当前应用与备份状态 (agy-zh doctor)");
    println!("  5. 安装静态 ASAR 安全补丁 (agy-zh install)");
    println!("  6. 还原为官方原版应用 (agy-zh restore)");
    println!("  0. 退出");
    println!();

    let choice = prompt_line("请输入编号 [0-6]: ")?;
    match choice.as_str() {
        "1" => dispatch_command(Commands::Run {
            app: Some(default_target),
            port: None,
            foreground: false,
            custom_translations: None,
            trust_custom_translations: false,
        }),
        "2" => dispatch_command(Commands::Shortcut {
            app: Some(default_target),
            output: None,
            bin_dir: None,
            remove: false,
        }),
        "3" => dispatch_command(Commands::Stop),
        "4" => dispatch_command(Commands::Doctor {
            app: Some(default_target),
        }),
        "5" => interactive_static_install(&default_target),
        "6" => dispatch_command(Commands::Restore {
            app: Some(default_target),
        }),
        "0" | "" => Ok(()),
        other => Err(format!("未知选项: {other}")),
    }
}

fn interactive_static_install(target: &Path) -> Result<(), String> {
    let platform = app_platform(target);
    let mut ack_sig = false;
    let mut allow_lib = false;
    if platform == Platform::Darwin {
        println!();
        println!("安全提示: 静态补丁将修改 Antigravity.app 内置 ASAR，外层签名将变更为本地 ad-hoc 签名并附加库校验例外。");
        let confirm = prompt_line("确认已阅读安全说明并继续静态安装吗？输入 yes 确认: ")?;
        if confirm != "yes" {
            return Err("用户取消静态安装".to_string());
        }
        ack_sig = true;
        allow_lib = true;
    }
    let replace_stale = if let Ok(rep) = doctor_app(target) {
        if let Some(old_ver) = rep.stale_backup_version {
            let ans = prompt_line(&format!(
                "检测到旧版 ({old_ver}) 残留备份，是否替换为当前版本备份？[y/N]: "
            ))?;
            matches!(ans.to_ascii_lowercase().as_str(), "y" | "yes")
        } else {
            false
        }
    } else {
        false
    };

    dispatch_command(Commands::Install {
        app: Some(target.to_path_buf()),
        acknowledge_signature_change: ack_sig,
        allow_local_library_loading: allow_lib,
        replace_stale_backup: replace_stale,
        custom_translations: None,
        trust_custom_translations: false,
    })
}

fn main() -> ExitCode {
    init_console_utf8();
    let cli = Cli::parse();
    let res = match cli.command {
        Some(cmd) => dispatch_command(cmd),
        None => {
            if io::stdin().is_terminal() {
                run_interactive_menu()
            } else {
                let mut cmd = Cli::command();
                let _ = cmd.print_help();
                println!();
                Ok(())
            }
        }
    };

    if let Err(err) = res {
        eprintln!("{}", format_stopped_message(&err, io::stderr().is_terminal()));
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_tty_stopped_message_keeps_plain_format() {
        let msg = "Antigravity Desktop 正在运行且未开启本地调试端口；请先完全退出客户端（macOS 按 Cmd+Q），再运行 `agy-zh run`";
        let out = format_stopped_message(msg, false);
        assert_eq!(out, format!("Stopped: {msg}"));
        assert!(!out.contains("\x1b["));
    }

    #[test]
    fn tty_stopped_message_renders_aligned_asterisk_box() {
        let msg = "Antigravity Desktop 正在运行且未开启本地调试端口；请先完全退出客户端（macOS 按 Cmd+Q），再运行 `agy-zh run`";
        let boxed = format_stopped_message(msg, true);
        assert!(!boxed.contains("\x1b["));
        assert!(boxed.contains("[!] 操作已暂停 (Stopped)"));
        assert!(boxed.contains("请先完全退出客户端（macOS 按 Cmd+Q），再运行 `agy-zh run`"));
        let lines: Vec<&str> = boxed.lines().collect();
        assert!(lines.len() >= 6);
        assert!(lines.first().unwrap().chars().all(|c| c == '*'));
        assert!(lines.last().unwrap().chars().all(|c| c == '*'));
        assert!(lines[1..lines.len() - 1]
            .iter()
            .all(|l| l.starts_with("* ") && l.ends_with(" *")));
        let widths: Vec<usize> = lines.iter().map(|l| str_display_width(l)).collect();
        assert!(widths.iter().all(|&w| w == widths[0] && w <= 72));
    }

    #[test]
    fn wrap_display_lines_handles_long_unbroken_paths() {
        let long_path = format!(
            "Target path failed: /Applications/VeryLongDirectoryName/{}",
            "segment_".repeat(12)
        );
        let wrapped = wrap_display_lines(&long_path, 48);
        assert!(wrapped.len() >= 3);
        assert!(wrapped.iter().all(|l| str_display_width(l) <= 48));
    }
}
