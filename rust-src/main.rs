mod asar;
mod cdp;
mod compatibility;
mod custom_dict;
mod daemon;
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
        eprintln!("Stopped: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
