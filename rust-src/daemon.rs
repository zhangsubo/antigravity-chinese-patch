use crate::cdp::{fetch_targets, poll_and_inject};
use crate::custom_dict::{load_custom_translations, LoadedCustomDict};
use crate::platform::{
    app_platform, default_app_path, is_main_app_running, lexically_abs, user_log_file,
    user_runtime_dir, Platform, RealSystemOps,
};
#[cfg(windows)]
use crate::platform::terminate_daemon_pid_windows;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static SIGNAL_STOP: AtomicBool = AtomicBool::new(false);

pub const CDP_RECOVERY_HINT: &str =
    "若客户端正在启动，请稍后重试。若更新后持续断开，请完全退出 Antigravity Desktop，再执行 agy-zh stop 和 agy-zh run。";

fn disconnect_expired(start: Instant, last_connected: Option<Instant>, now: Instant) -> bool {
    match last_connected {
        Some(last) => now.duration_since(last) >= Duration::from_secs(30),
        None => now.duration_since(start) >= Duration::from_secs(90),
    }
}

fn should_probe_app_process(last_probe: Option<Instant>, now: Instant) -> bool {
    match last_probe {
        None => true,
        Some(prev) => now.duration_since(prev) >= Duration::from_secs(5),
    }
}

fn sleep_interruptible_at(duration: Duration, stop_flag: &Path) -> bool {
    let deadline = Instant::now() + duration;
    loop {
        if SIGNAL_STOP.load(Ordering::SeqCst) || stop_flag.exists() {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let step = deadline
            .saturating_duration_since(now)
            .min(Duration::from_millis(100));
        thread::sleep(step);
    }
}

#[cfg(unix)]
extern "C" fn handle_stop_signal(_sig: libc::c_int) {
    SIGNAL_STOP.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
fn install_signal_handlers() {
    SIGNAL_STOP.store(false, Ordering::SeqCst);
    unsafe {
        libc::signal(
            libc::SIGTERM,
            handle_stop_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            handle_stop_signal as *const () as libc::sighandler_t,
        );
    }
}

#[cfg(not(unix))]
fn install_signal_handlers() {
    SIGNAL_STOP.store(false, Ordering::SeqCst);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub pid: u32,
    pub port: u16,
    pub app: String,
    pub started_at: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DaemonSummary {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub status: String,
    pub mode: String,
    pub app: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub log_file: String,
    pub custom_translations_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StopReport {
    pub stopped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub app: Option<PathBuf>,
    pub port: Option<u16>,
    pub foreground: bool,
    pub custom_translations: Option<PathBuf>,
    pub trust_custom_translations: bool,
}

pub fn lock_path() -> PathBuf {
    user_runtime_dir().join("cdp-daemon.lock")
}

pub fn state_path() -> PathBuf {
    user_runtime_dir().join("cdp-daemon.json")
}

pub fn stop_flag_path() -> PathBuf {
    user_runtime_dir().join("cdp-daemon.stop")
}

struct DaemonLockGuard {
    _file: File,
}

impl DaemonLockGuard {
    fn try_acquire(path: &Path) -> Result<Option<Self>, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)
                .map_err(|e| format!("Failed to open lock file {}: {e}", path.display()))?;
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if rc != 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.raw_os_error() == Some(libc::EWOULDBLOCK)
                {
                    return Ok(None);
                }
                return Err(format!("Failed to lock {}: {err}", path.display()));
            }
            Ok(Some(Self { _file: file }))
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .share_mode(0)
                .open(path)
            {
                Ok(file) => Ok(Some(Self { _file: file })),
                Err(e) if e.raw_os_error() == Some(32) || e.raw_os_error() == Some(33) => {
                    Ok(None)
                }
                Err(e) => Err(format!("Failed to lock {}: {e}", path.display())),
            }
        }
    }
}

pub fn daemon_status_summary() -> DaemonSummary {
    daemon_status_summary_at(&lock_path(), &state_path())
}

pub fn daemon_status_summary_at(lock: &Path, state: &Path) -> DaemonSummary {
    if !lock.exists() {
        return DaemonSummary::default();
    }
    match DaemonLockGuard::try_acquire(lock) {
        Ok(Some(_guard)) => {
            // Lock acquired => no live daemon owns it.
            DaemonSummary::default()
        }
        Ok(None) => {
            let parsed = fs::read_to_string(state)
                .ok()
                .and_then(|raw| serde_json::from_str::<DaemonState>(&raw).ok());
            match parsed {
                Some(s) => DaemonSummary {
                    running: true,
                    pid: Some(s.pid),
                    port: Some(s.port),
                    app: Some(s.app),
                },
                None => DaemonSummary {
                    running: true,
                    pid: None,
                    port: None,
                    app: None,
                },
            }
        }
        Err(_) => DaemonSummary::default(),
    }
}

pub fn pick_free_port(requested: Option<u16>) -> Result<u16, String> {
    if let Some(p) = requested {
        if p == 0 {
            return Err("Invalid CDP port 0".to_string());
        }
        return Ok(p);
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("Failed to allocate free loopback port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to read allocated port: {e}"))?
        .port();
    Ok(port)
}

#[cfg(windows)]
fn clear_stdio_inheritance() {
    use windows_sys::Win32::Foundation::{
        SetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    unsafe {
        for kind in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let h = GetStdHandle(kind);
            if !h.is_null() && h != INVALID_HANDLE_VALUE {
                let _ = SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

fn launch_app_with_cdp(app: &Path, platform: Platform, port: u16) -> Result<(), String> {
    let port_flag = format!("--remote-debugging-port={port}");
    let addr_flag = "--remote-debugging-address=127.0.0.1";

    match platform {
        Platform::Darwin => {
            let status = Command::new("/usr/bin/open")
                .arg("-a")
                .arg(app)
                .arg("--args")
                .arg(&port_flag)
                .arg(addr_flag)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|e| format!("Failed to launch Antigravity via `open`: {e}"))?;
            if !status.success() {
                return Err(format!(
                    "`open -a` exited with status {}",
                    status.code().unwrap_or(-1)
                ));
            }
            Ok(())
        }
        Platform::Linux => {
            let exe = app.join(platform.executable_rel());
            Command::new(&exe)
                .arg(&port_flag)
                .arg(addr_flag)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to launch {}: {e}", exe.display()))?;
            Ok(())
        }
        Platform::Win32 => {
            let exe = app.join(platform.executable_rel());
            let mut cmd = Command::new(&exe);
            cmd.arg(&port_flag)
                .arg(addr_flag)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const DETACHED_PROCESS: u32 = 0x0000_0008;
                const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
                clear_stdio_inheritance();
                cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            }
            cmd.spawn()
                .map_err(|e| format!("Failed to launch {}: {e}", exe.display()))?;
            Ok(())
        }
    }
}

pub fn run_cdp(opts: RunOptions) -> Result<RunReport, String> {
    let app = lexically_abs(opts.app.as_deref().unwrap_or(&default_app_path()))?;
    let platform = app_platform(&app);
    if !app.exists() {
        return Err(format!("Antigravity app not found at {}", app.display()));
    }
    let exe_file = app.join(platform.executable_rel());
    if !exe_file.exists() {
        return Err(format!(
            "Antigravity executable not found at {}",
            exe_file.display()
        ));
    }

    let custom_dict = load_custom_translations(
        opts.custom_translations.as_deref(),
        platform,
        opts.trust_custom_translations,
    )?;
    let log_file = user_log_file();

    let existing = daemon_status_summary();
    if existing.running {
        if let Some(existing_port) = existing.port {
            if fetch_targets(existing_port).is_ok() {
                let _ = poll_and_inject(existing_port, &custom_dict);
                return Ok(RunReport {
                    status: "already_running".to_string(),
                    mode: "cdp".to_string(),
                    app: existing.app.unwrap_or_else(|| app.to_string_lossy().into_owned()),
                    port: existing_port,
                    pid: existing.pid,
                    log_file: log_file.to_string_lossy().into_owned(),
                    custom_translations_count: custom_dict.entries.len(),
                });
            }
        }
        return Err(format!("CDP 守护进程正在运行，调试端口暂不可用。{CDP_RECOVERY_HINT}"));
    }

    let attach_existing = match opts.port {
        Some(p) if fetch_targets(p).is_ok() => true,
        _ => false,
    };

    if !attach_existing && is_main_app_running(&app, platform, &RealSystemOps)? {
        return Err(
            "Antigravity Desktop 正在运行且未开启本地调试端口；请先完全退出客户端（macOS 按 Cmd+Q），再运行 `agy-zh run`"
                .to_string(),
        );
    }

    let port = pick_free_port(opts.port)?;

    if opts.foreground {
        run_foreground_loop(&app, platform, port, attach_existing, &custom_dict, &log_file)
    } else {
        spawn_detached_daemon(&app, port, &opts, &custom_dict, &log_file)
    }
}

fn spawn_detached_daemon(
    app: &Path,
    port: u16,
    opts: &RunOptions,
    custom_dict: &LoadedCustomDict,
    log_file: &Path,
) -> Result<RunReport, String> {
    if let Some(parent) = log_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create log directory {}: {e}", parent.display()))?;
    }
    let log_out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .map_err(|e| format!("Failed to open log file {}: {e}", log_file.display()))?;
    let log_err = log_out
        .try_clone()
        .map_err(|e| format!("Failed to clone log handle: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(log_file, fs::Permissions::from_mode(0o600));
    }

    let exe = std::env::current_exe()
        .map_err(|e| format!("Failed to resolve current agy-zh executable: {e}"))?;
    let mut cmd = Command::new(&exe);
    cmd.arg("run")
        .arg("--foreground")
        .arg("--app")
        .arg(app)
        .arg("--port")
        .arg(port.to_string());
    if let Some(custom_path) = &opts.custom_translations {
        cmd.arg("--custom-translations").arg(custom_path);
    }
    if opts.trust_custom_translations {
        cmd.arg("--trust-custom-translations");
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(log_out))
        .stderr(Stdio::from(log_err));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        clear_stdio_inheritance();
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn background CDP daemon: {e}"))?;

    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if let Ok(Some(exit)) = child.try_wait() {
            return Err(format!(
                "Background CDP daemon exited early ({}); check log {}",
                exit,
                log_file.display()
            ));
        }
        let summary = daemon_status_summary();
        if summary.running && summary.port == Some(port) {
            return Ok(RunReport {
                status: "started".to_string(),
                mode: "cdp".to_string(),
                app: app.to_string_lossy().into_owned(),
                port,
                pid: summary.pid.or_else(|| Some(child.id())),
                log_file: log_file.to_string_lossy().into_owned(),
                custom_translations_count: custom_dict.entries.len(),
            });
        }
        thread::sleep(Duration::from_millis(120));
    }

    Err(format!(
        "Timed out waiting for CDP daemon startup; check log {}",
        log_file.display()
    ))
}

fn run_foreground_loop(
    app: &Path,
    platform: Platform,
    port: u16,
    attach_existing: bool,
    custom_dict: &LoadedCustomDict,
    log_file: &Path,
) -> Result<RunReport, String> {
    install_signal_handlers();
    let _lock = DaemonLockGuard::try_acquire(&lock_path())?
        .ok_or_else(|| "Another CDP daemon already holds the lock".to_string())?;
    let _ = fs::remove_file(stop_flag_path());

    if !attach_existing {
        launch_app_with_cdp(app, platform, port)?;
    }

    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let state = DaemonState {
        pid: std::process::id(),
        port,
        app: app.to_string_lossy().into_owned(),
        started_at,
    };
    let state_bytes = serde_json::to_vec_pretty(&state)
        .map_err(|e| format!("Failed to serialize daemon state: {e}"))?;
    fs::write(state_path(), state_bytes)
        .map_err(|e| format!("Failed to write daemon state: {e}"))?;

    let start = Instant::now();
    let mut last_connected = None;
    let mut last_process_probe: Option<Instant> = None;
    let mut miss_count = 0u32;

    loop {
        if SIGNAL_STOP.load(Ordering::SeqCst) || stop_flag_path().exists() {
            break;
        }

        match poll_and_inject(port, custom_dict) {
            Ok(_cycle) => {
                last_connected = Some(Instant::now());
                last_process_probe = None;
                miss_count = 0;
            }
            Err(_) => {
                miss_count = miss_count.saturating_add(1);
                let now = Instant::now();
                if disconnect_expired(start, last_connected, now) {
                    break;
                }
                let past_startup = last_connected.is_some() || start.elapsed() > Duration::from_secs(12);
                if past_startup && miss_count >= 3 && should_probe_app_process(last_process_probe, now)
                {
                    last_process_probe = Some(now);
                    if !is_main_app_running(app, platform, &RealSystemOps).unwrap_or(false) {
                        break;
                    }
                }
            }
        }

        let interval = if start.elapsed() < Duration::from_secs(15) {
            Duration::from_millis(500)
        } else {
            Duration::from_millis(2000)
        };
        if sleep_interruptible_at(interval, &stop_flag_path()) {
            break;
        }
    }

    let _ = fs::remove_file(state_path());
    let _ = fs::remove_file(stop_flag_path());

    Ok(RunReport {
        status: "stopped".to_string(),
        mode: "cdp".to_string(),
        app: app.to_string_lossy().into_owned(),
        port,
        pid: Some(std::process::id()),
        log_file: log_file.to_string_lossy().into_owned(),
        custom_translations_count: custom_dict.entries.len(),
    })
}

pub fn stop_daemon() -> Result<StopReport, String> {
    let summary = daemon_status_summary();
    if !summary.running {
        let _ = fs::remove_file(state_path());
        let _ = fs::remove_file(stop_flag_path());
        return Ok(StopReport {
            stopped: false,
            pid: None,
            port: None,
            reason: Some("no_active_daemon".to_string()),
        });
    }

    if let Some(parent) = stop_flag_path().parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = File::create(stop_flag_path()) {
        let _ = f.write_all(b"stop\n");
    }

    #[cfg(unix)]
    if let Some(pid) = summary.pid {
        if pid > 0 {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
    }

    let deadline = Instant::now() + Duration::from_secs(4);
    #[cfg(windows)]
    let force_after = Instant::now() + Duration::from_millis(2500);
    #[cfg(windows)]
    let mut force_attempted = false;
    while Instant::now() < deadline {
        if !daemon_status_summary().running {
            let _ = fs::remove_file(state_path());
            let _ = fs::remove_file(stop_flag_path());
            return Ok(StopReport {
                stopped: true,
                pid: summary.pid,
                port: summary.port,
                reason: None,
            });
        }
        #[cfg(windows)]
        if !force_attempted && Instant::now() >= force_after {
            force_attempted = true;
            if let Some(pid) = summary.pid {
                let _ = terminate_daemon_pid_windows(pid);
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err("Timed out waiting for CDP daemon to exit".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::canonical_temp_dir;

    #[test]
    fn disconnect_grace_restarts_after_a_successful_poll() {
        let start = Instant::now();
        let after = |s| start + Duration::from_secs(s);
        assert!(!disconnect_expired(start, None, after(89)));
        assert!(disconnect_expired(start, None, after(90)));
        assert!(!disconnect_expired(start, Some(after(88)), after(117)));
        assert!(disconnect_expired(start, Some(after(88)), after(118)));
    }

    #[test]
    fn single_instance_lock_prevents_double_acquire_and_reports_running_state() {
        let (_tmp, root) = canonical_temp_dir();
        let lock = root.join("cdp-daemon.lock");
        let state = root.join("cdp-daemon.json");

        assert!(!daemon_status_summary_at(&lock, &state).running);

        let guard = DaemonLockGuard::try_acquire(&lock).unwrap().unwrap();
        let meta = DaemonState {
            pid: 4242,
            port: 9333,
            app: "/Applications/Antigravity.app".to_string(),
            started_at: 1700000000,
        };
        fs::write(&state, serde_json::to_vec(&meta).unwrap()).unwrap();

        // Second acquire in-process must return None (already locked)
        assert!(DaemonLockGuard::try_acquire(&lock).unwrap().is_none());

        let summary = daemon_status_summary_at(&lock, &state);
        assert!(summary.running);
        assert_eq!(summary.pid, Some(4242));
        assert_eq!(summary.port, Some(9333));

        drop(guard);
        assert!(!daemon_status_summary_at(&lock, &state).running);
    }

    #[test]
    fn process_probe_throttling_respects_cooldown() {
        let base = Instant::now();
        assert!(should_probe_app_process(None, base));
        assert!(!should_probe_app_process(
            Some(base),
            base + Duration::from_millis(4999)
        ));
        assert!(should_probe_app_process(
            Some(base),
            base + Duration::from_secs(5)
        ));
    }

    #[test]
    fn sleep_interruptible_returns_early_when_stop_flag_exists() {
        let (_tmp, root) = canonical_temp_dir();
        let stop_flag = root.join("cdp-daemon.stop");

        let t0 = Instant::now();
        assert!(!sleep_interruptible_at(Duration::from_millis(30), &stop_flag));
        assert!(t0.elapsed() >= Duration::from_millis(20));

        fs::write(&stop_flag, b"stop\n").unwrap();
        let t1 = Instant::now();
        assert!(sleep_interruptible_at(Duration::from_secs(2), &stop_flag));
        assert!(t1.elapsed() < Duration::from_millis(500));
    }
}
