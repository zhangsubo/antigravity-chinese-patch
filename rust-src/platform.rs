use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Darwin,
    Linux,
    Win32,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Darwin => "darwin",
            Platform::Linux => "linux",
            Platform::Win32 => "win32",
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Platform::Darwin => "macOS",
            Platform::Linux => "Linux",
            Platform::Win32 => "Windows",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "darwin" => Some(Platform::Darwin),
            "linux" => Some(Platform::Linux),
            "win32" => Some(Platform::Win32),
            _ => None,
        }
    }

    pub fn host() -> Self {
        #[cfg(target_os = "macos")]
        {
            Platform::Darwin
        }
        #[cfg(target_os = "windows")]
        {
            Platform::Win32
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Platform::Linux
        }
    }

    pub fn executable_rel(self) -> &'static str {
        match self {
            Platform::Darwin => "Contents/MacOS/Antigravity",
            Platform::Linux => "antigravity",
            Platform::Win32 => "Antigravity.exe",
        }
    }

    pub fn asar_rel(self) -> &'static str {
        match self {
            Platform::Darwin => "Contents/Resources/app.asar",
            Platform::Linux | Platform::Win32 => "resources/app.asar",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait SystemOps {
    fn run_cmd(&self, program: &str, args: &[&str]) -> Result<CmdOutput, String>;
    fn clone_dir(&self, src: &Path, dst: &Path) -> Result<(), String>;
    fn is_writable(&self, path: &Path, is_dir: bool) -> bool;
    fn list_windows_processes(&self) -> Result<Option<Vec<(u32, String)>>, String> {
        Ok(None)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RealSystemOps;

impl SystemOps for RealSystemOps {
    fn run_cmd(&self, program: &str, args: &[&str]) -> Result<CmdOutput, String> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let output = cmd
            .output()
            .map_err(|e| format!("Failed to execute {program}: {e}"))?;
        Ok(CmdOutput {
            status_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    #[cfg(windows)]
    fn list_windows_processes(&self) -> Result<Option<Vec<(u32, String)>>, String> {
        win_process::list_process_images().map(Some)
    }

    fn clone_dir(&self, src: &Path, dst: &Path) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            if macos_clone::clone_dir_apfs(src, dst).is_ok() {
                return Ok(());
            }
            let _ = fs::remove_dir_all(dst);
            let out = self.run_cmd(
                "/usr/bin/ditto",
                &[
                    src.to_string_lossy().as_ref(),
                    dst.to_string_lossy().as_ref(),
                ],
            )?;
            if out.status_code == 0 {
                return Ok(());
            }
            return Err(format!("ditto failed: {}", out.stderr.trim()));
        }

        #[cfg(not(target_os = "macos"))]
        {
            copy_dir_recursive(src, dst)
        }
    }

    fn is_writable(&self, path: &Path, is_dir: bool) -> bool {
        #[cfg(unix)]
        {
            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt;
            let Ok(cstr) = CString::new(path.as_os_str().as_bytes()) else {
                return false;
            };
            let mode = if is_dir {
                libc::W_OK | libc::X_OK
            } else {
                libc::W_OK
            };
            unsafe { libc::access(cstr.as_ptr(), mode) == 0 }
        }
        #[cfg(windows)]
        {
            if is_dir {
                if !path.is_dir() {
                    return false;
                }
                let probe = path.join(format!(".agy-zh-write-test-{}", std::process::id()));
                match fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&probe)
                {
                    Ok(_) => {
                        let _ = fs::remove_file(&probe);
                        true
                    }
                    Err(_) => false,
                }
            } else {
                match fs::metadata(path) {
                    Ok(meta) => {
                        if meta.permissions().readonly() {
                            return false;
                        }
                        fs::OpenOptions::new().write(true).open(path).is_ok()
                    }
                    Err(_) => false,
                }
            }
        }
    }
}

pub fn run_checked(ops: &dyn SystemOps, program: &str, args: &[&str]) -> Result<CmdOutput, String> {
    let out = ops.run_cmd(program, args)?;
    if out.status_code != 0 {
        let detail = if out.stderr.trim().is_empty() {
            out.stdout.trim()
        } else {
            out.stderr.trim()
        };
        if detail.is_empty() {
            return Err(format!("{program} exited with status {}", out.status_code));
        }
        return Err(format!("{program} failed: {detail}"));
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
mod macos_clone {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    const COPYFILE_RECURSIVE: u32 = 1 << 15;
    const COPYFILE_CLONE: u32 = 1 << 24;

    extern "C" {
        fn copyfile(
            from: *const libc::c_char,
            to: *const libc::c_char,
            state: *mut libc::c_void,
            flags: u32,
        ) -> libc::c_int;
    }

    pub fn clone_dir_apfs(src: &Path, dst: &Path) -> Result<(), String> {
        let src_c = CString::new(src.as_os_str().as_bytes())
            .map_err(|_| "Invalid NUL byte in source path".to_string())?;
        let dst_c = CString::new(dst.as_os_str().as_bytes())
            .map_err(|_| "Invalid NUL byte in destination path".to_string())?;
        let rc = unsafe {
            copyfile(
                src_c.as_ptr(),
                dst_c.as_ptr(),
                std::ptr::null_mut(),
                COPYFILE_CLONE | COPYFILE_RECURSIVE,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            let err = std::io::Error::last_os_error();
            Err(format!("copyfile APFS clone failed: {err}"))
        }
    }
}

#[allow(dead_code)]
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(src)
        .map_err(|e| format!("Failed to inspect {}: {e}", src.display()))?;
    if meta.file_type().is_symlink() {
        return copy_symlink(src, dst);
    }
    if !meta.is_dir() {
        return Err(format!("Expected directory: {}", src.display()));
    }
    fs::create_dir_all(dst).map_err(|e| format!("Failed to create {}: {e}", dst.display()))?;
    let entries =
        fs::read_dir(src).map_err(|e| format!("Failed to read {}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to iterate {}: {e}", src.display()))?;
        let child_src = entry.path();
        let child_dst = dst.join(entry.file_name());
        let child_meta = fs::symlink_metadata(&child_src)
            .map_err(|e| format!("Failed to inspect {}: {e}", child_src.display()))?;
        if child_meta.file_type().is_symlink() {
            copy_symlink(&child_src, &child_dst)?;
        } else if child_meta.is_dir() {
            copy_dir_recursive(&child_src, &child_dst)?;
        } else {
            fs::copy(&child_src, &child_dst).map_err(|e| {
                format!(
                    "Failed to copy {} -> {}: {e}",
                    child_src.display(),
                    child_dst.display()
                )
            })?;
            #[cfg(unix)]
            {
                let _ = fs::set_permissions(&child_dst, child_meta.permissions());
            }
        }
    }
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(dst, meta.permissions());
    }
    Ok(())
}

#[allow(dead_code)]
fn copy_symlink(src: &Path, dst: &Path) -> Result<(), String> {
    let target =
        fs::read_link(src).map_err(|e| format!("Failed to read link {}: {e}", src.display()))?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, dst)
            .map_err(|e| format!("Failed to create symlink {}: {e}", dst.display()))
    }
    #[cfg(windows)]
    {
        let is_dir = fs::metadata(src).map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            std::os::windows::fs::symlink_dir(&target, dst)
                .map_err(|e| format!("Failed to create dir symlink {}: {e}", dst.display()))
        } else {
            std::os::windows::fs::symlink_file(&target, dst)
                .map_err(|e| format!("Failed to create file symlink {}: {e}", dst.display()))
        }
    }
}

pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = env::var_os("HOMEDRIVE")?;
                let path = env::var_os("HOMEPATH")?;
                let mut p = PathBuf::from(drive);
                p.push(path);
                Some(p)
            })
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME").map(PathBuf::from)
    }
}

pub fn user_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library/Application Support/antigravity-chinese-patch")
    }
    #[cfg(target_os = "windows")]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join("AppData/Local")))
            .unwrap_or_else(env::temp_dir)
            .join("antigravity-chinese-patch")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        env::var_os("XDG_DATA_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".local/share")))
            .unwrap_or_else(env::temp_dir)
            .join("antigravity-chinese-patch")
    }
}

pub fn user_runtime_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        user_data_dir()
    }
    #[cfg(target_os = "windows")]
    {
        user_data_dir()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        env::var_os("XDG_RUNTIME_DIR")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("XDG_CACHE_HOME")
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
            })
            .or_else(|| home_dir().map(|h| h.join(".cache")))
            .unwrap_or_else(env::temp_dir)
            .join("antigravity-chinese-patch")
    }
}

pub fn user_log_file() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library/Logs/antigravity-chinese-patch.log")
    }
    #[cfg(not(target_os = "macos"))]
    {
        user_runtime_dir().join("antigravity-chinese-patch.log")
    }
}

pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

pub fn lexically_abs(path: &Path) -> Result<PathBuf, String> {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|e| format!("Failed to read working directory: {e}"))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for comp in joined.components() {
        match comp {
            Component::Prefix(p) => normalized.push(p.as_os_str()),
            Component::RootDir => normalized.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(comp.as_os_str());
                }
            }
            Component::Normal(s) => normalized.push(s),
        }
    }
    Ok(normalized)
}

pub fn is_link(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
            return true;
        }
    }
    false
}

pub fn paths(app: &Path) -> (PathBuf, PathBuf) {
    let base = lexically_abs(app).unwrap_or_else(|_| app.to_path_buf());
    let name = base
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let backup = base.with_file_name(format!("{name}.zh-cn-original"));
    let state = base.with_file_name(format!("{name}.zh-cn-state.json"));
    (backup, state)
}

pub fn same_app_path(recorded: &str, current: &Path, platform: Platform) -> bool {
    let current_abs = lexically_abs(current).unwrap_or_else(|_| current.to_path_buf());
    if platform == Platform::Win32 {
        let rec_abs = lexically_abs(Path::new(recorded)).unwrap_or_else(|_| PathBuf::from(recorded));
        rec_abs.to_string_lossy().to_lowercase() == current_abs.to_string_lossy().to_lowercase()
    } else {
        recorded == current_abs.to_string_lossy().as_ref()
    }
}

pub fn app_platform(app: &Path) -> Platform {
    let abs = lexically_abs(app).unwrap_or_else(|_| app.to_path_buf());
    let (backup, state) = paths(&abs);
    if abs.extension().and_then(|e| e.to_str()) == Some("app")
        || abs.join("Contents").exists()
        || backup.join("Contents").exists()
    {
        return Platform::Darwin;
    }
    if abs.join("Antigravity.exe").exists() || backup.join("Antigravity.exe").exists() {
        return Platform::Win32;
    }
    if abs.join("antigravity").exists() || backup.join("antigravity").exists() {
        return Platform::Linux;
    }
    if state.is_file() && !is_link(&state) {
        if let Ok(text) = fs::read_to_string(&state) {
            if let Ok(meta) = serde_json::from_str::<Value>(&text) {
                if let Some(p) = meta.get("platform").and_then(Value::as_str) {
                    if let Some(parsed) = Platform::from_str(p) {
                        return parsed;
                    }
                }
            }
        }
    }
    if abs.join("resources/app.asar").exists() || backup.join("resources/app.asar").exists() {
        return if Platform::host() == Platform::Win32 {
            Platform::Win32
        } else {
            Platform::Linux
        };
    }
    Platform::host()
}

pub fn asar_path(app: &Path, platform: Platform) -> PathBuf {
    app.join(platform.asar_rel())
}

pub fn default_app_path() -> PathBuf {
    let host = Platform::host();
    let home = home_dir();
    let path_env = env::var_os("PATH");
    match host {
        Platform::Darwin => {
            let mut candidates = vec![PathBuf::from("/Applications/Antigravity.app")];
            if let Some(h) = &home {
                candidates.push(h.join("Applications/Antigravity.app"));
            }
            for c in &candidates {
                if c.join("Contents/Resources/app.asar").is_file() {
                    return c.clone();
                }
            }
            candidates.into_iter().next().unwrap()
        }
        Platform::Linux => {
            let mut candidates = vec![
                PathBuf::from("/opt/Antigravity"),
                PathBuf::from("/opt/antigravity"),
                PathBuf::from("/usr/share/antigravity"),
                PathBuf::from("/usr/lib/antigravity"),
            ];
            if let Some(h) = &home {
                candidates.extend([
                    h.join(".local/share/antigravity"),
                    h.join("Antigravity-x64"),
                    h.join("Antigravity-arm64"),
                    h.join("Antigravity"),
                ]);
            }
            detect_app_from_candidates(&candidates, "antigravity", path_env.as_deref(), 1)
                .unwrap_or_else(|| candidates[0].clone())
        }
        Platform::Win32 => {
            let local_appdata = env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join("AppData/Local")))
                .unwrap_or_else(|| PathBuf::from(r"C:\Users\Default\AppData\Local"));
            let program_files = env::var_os("ProgramFiles")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
            let program_files_x86 = env::var_os("ProgramFiles(x86)")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
            let mut candidates = vec![
                local_appdata.join("Programs/Antigravity"),
                local_appdata.join("Antigravity"),
                program_files.join("Antigravity"),
                program_files_x86.join("Antigravity"),
            ];
            if let Some(h) = &home {
                candidates.extend([
                    h.join("AppData/Local/Programs/Antigravity"),
                    h.join("Antigravity"),
                    h.join("Antigravity-win32-x64"),
                    h.join("Antigravity-win32-arm64"),
                ]);
            }
            detect_app_from_candidates(&candidates, "Antigravity.exe", path_env.as_deref(), 4)
                .unwrap_or_else(|| candidates[0].clone())
        }
    }
}

pub fn detect_app_from_candidates(
    candidates: &[PathBuf],
    exe_name: &str,
    path_env: Option<&std::ffi::OsStr>,
    max_ancestors: usize,
) -> Option<PathBuf> {
    for candidate in candidates {
        if candidate.join("resources/app.asar").is_file() && candidate.join(exe_name).is_file() {
            return Some(candidate.clone());
        }
    }
    if let Some(raw_path) = path_env {
        for dir in env::split_paths(raw_path) {
            let bin = dir.join(exe_name);
            if bin.is_file() && !is_link(&bin) {
                let mut cur = bin.parent();
                for _ in 0..max_ancestors {
                    if let Some(p) = cur {
                        if p.join("resources/app.asar").is_file() && p.join(exe_name).is_file() {
                            return Some(p.to_path_buf());
                        }
                        cur = p.parent();
                    } else {
                        break;
                    }
                }
            }
        }
    }
    None
}

pub fn resolved_without_symlinks(path: &Path) -> Result<bool, String> {
    let abs = lexically_abs(path)?;
    let canon = fs::canonicalize(&abs).map_err(|e| format!("Path does not exist ({}): {e}", abs.display()))?;
    let clean = strip_verbatim_prefix(&canon);
    #[cfg(windows)]
    {
        Ok(clean.to_string_lossy().to_lowercase() == abs.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        Ok(clean == abs)
    }
}

pub fn assert_no_escaping_symlinks(root: &Path) -> Result<(), String> {
    let abs_root = lexically_abs(root)?;
    let root_resolved = strip_verbatim_prefix(
        &fs::canonicalize(&abs_root)
            .map_err(|e| format!("Failed to inspect {}: {e}", abs_root.display()))?,
    );
    walk_check_symlinks(&abs_root, &root_resolved)
}

fn walk_check_symlinks(dir: &Path, root_resolved: &Path) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to iterate {}: {e}", dir.display()))?;
        let path = entry.path();
        if is_link(&path) {
            let target = fs::canonicalize(&path).map_err(|_| {
                format!(
                    "Application directory contains a symlink escaping the app root: {}",
                    path.display()
                )
            })?;
            let clean_target = strip_verbatim_prefix(&target);
            if !clean_target.starts_with(root_resolved) {
                return Err(format!(
                    "Application directory contains a symlink escaping the app root: {}",
                    path.display()
                ));
            }
            continue;
        }
        let meta = fs::symlink_metadata(&path)
            .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?;
        if meta.is_dir() {
            walk_check_symlinks(&path, root_resolved)?;
        }
    }
    Ok(())
}

pub fn assert_app_path(app: &Path) -> Result<PathBuf, String> {
    let abs = lexically_abs(app)?;
    if is_link(&abs) || !resolved_without_symlinks(&abs).unwrap_or(false) {
        return Err("Symlink application paths are not supported".to_string());
    }
    let platform = app_platform(&abs);
    if platform == Platform::Darwin {
        if abs.extension().and_then(|e| e.to_str()) != Some("app") || !abs.is_dir() {
            return Err("Select an existing .app bundle".to_string());
        }
        for relative in ["Contents/Info.plist", "Contents/Resources/app.asar"] {
            let p = abs.join(relative);
            if !p.is_file() || is_link(&p) || !resolved_without_symlinks(&p).unwrap_or(false) {
                return Err(format!("Expected regular bundle files: {relative}"));
            }
        }
    } else {
        if !abs.is_dir() {
            return Err("Select an existing Antigravity application directory".to_string());
        }
        for relative in [platform.executable_rel(), "resources/app.asar"] {
            let p = abs.join(relative);
            if !p.is_file() || is_link(&p) || !resolved_without_symlinks(&p).unwrap_or(false) {
                return Err(format!("Expected regular application files: {relative}"));
            }
        }
        assert_no_escaping_symlinks(&abs)?;
    }
    Ok(abs)
}

pub fn permission_error_hint(platform: Platform) -> &'static str {
    if platform == Platform::Win32 {
        "run as Administrator or use a user-writable installation path"
    } else {
        "run with appropriate permissions (e.g. sudo) or use a user-writable installation path"
    }
}

pub fn ensure_writable(
    targets: &[&Path],
    platform: Platform,
    ops: &dyn SystemOps,
) -> Result<(), String> {
    for target in targets {
        let check = if target.exists() {
            (*target).to_path_buf()
        } else {
            target
                .parent()
                .ok_or_else(|| {
                    format!(
                        "Insufficient permissions to modify {}; {}",
                        target.display(),
                        permission_error_hint(platform)
                    )
                })?
                .to_path_buf()
        };
        if !check.exists() || !ops.is_writable(&check, check.is_dir()) {
            return Err(format!(
                "Insufficient permissions to modify {}; {}",
                target.display(),
                permission_error_hint(platform)
            ));
        }
    }
    Ok(())
}

pub fn is_inside_agent_session() -> bool {
    env::var_os("ANTIGRAVITY_SOURCE_METADATA").is_some()
        || env::var_os("ANTIGRAVITY_AGENT").is_some()
        || env::var_os("ORCA_AGENT_LAUNCH").is_some()
}

#[cfg(windows)]
mod win_process {
    use super::strip_verbatim_prefix;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    struct HandleGuard(HANDLE);

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    pub fn query_process_image_path(pid: u32) -> Option<String> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let guard = HandleGuard(handle);

        let mut cap = 512usize;
        while cap <= 32768 {
            let mut buf = vec![0u16; cap];
            let mut len = buf.len() as u32;
            let ok = unsafe { QueryFullProcessImageNameW(guard.0, 0, buf.as_mut_ptr(), &mut len) };
            if ok != 0 {
                let raw = OsString::from_wide(&buf[..len as usize]);
                return Some(
                    strip_verbatim_prefix(Path::new(&raw))
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            if std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_INSUFFICIENT_BUFFER as i32)
            {
                cap *= 2;
            } else {
                return None;
            }
        }
        None
    }

    pub fn list_process_images() -> Result<Vec<(u32, String)>, String> {
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap.is_null() || snap == INVALID_HANDLE_VALUE {
            return Err(format!(
                "CreateToolhelp32Snapshot failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let guard = HandleGuard(snap);

        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if unsafe { Process32FirstW(guard.0, &mut entry) } == 0 {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        loop {
            if entry.th32ProcessID != 0 {
                if let Some(path) = query_process_image_path(entry.th32ProcessID) {
                    out.push((entry.th32ProcessID, path));
                }
            }
            if unsafe { Process32NextW(guard.0, &mut entry) } == 0 {
                break;
            }
        }
        Ok(out)
    }

    pub fn terminate_self_binary_pid(pid: u32) -> bool {
        if pid == 0 || pid == std::process::id() {
            return false;
        }
        let Some(image_path) = query_process_image_path(pid) else {
            return false;
        };
        let is_self_bin = Path::new(&image_path)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("agy-zh.exe"));
        if !is_self_bin {
            return false;
        }
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if handle.is_null() {
            return false;
        }
        let guard = HandleGuard(handle);
        unsafe { TerminateProcess(guard.0, 0) != 0 }
    }
}

#[cfg(windows)]
pub fn terminate_daemon_pid_windows(pid: u32) -> bool {
    win_process::terminate_self_binary_pid(pid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessMatchScope {
    AllAppProcesses,
    MainExecutableOnly,
}

fn matches_win32_entry(
    norm: &str,
    exe_target: &str,
    res_target: &str,
    scope: ProcessMatchScope,
) -> bool {
    if norm.contains("patcher.py") || is_self_tool_command(norm) {
        return false;
    }
    let matches_exe = norm
        .strip_prefix(exe_target)
        .is_some_and(|tail| tail.is_empty() || tail.starts_with(' ') || tail.starts_with('"'));
    match scope {
        ProcessMatchScope::MainExecutableOnly => matches_exe,
        ProcessMatchScope::AllAppProcesses => matches_exe || norm.starts_with(res_target),
    }
}

pub fn is_app_running(
    app: &Path,
    platform: Platform,
    ops: &dyn SystemOps,
) -> Result<bool, String> {
    is_app_running_with_scope(app, platform, ProcessMatchScope::AllAppProcesses, ops)
}

pub fn is_main_app_running(
    app: &Path,
    platform: Platform,
    ops: &dyn SystemOps,
) -> Result<bool, String> {
    is_app_running_with_scope(app, platform, ProcessMatchScope::MainExecutableOnly, ops)
}

pub fn is_app_running_with_scope(
    app: &Path,
    platform: Platform,
    scope: ProcessMatchScope,
    ops: &dyn SystemOps,
) -> Result<bool, String> {
    let self_pid = std::process::id();
    match platform {
        Platform::Win32 => {
            let clean_app = strip_verbatim_prefix(app);
            let app_norm = clean_app.to_string_lossy().replace('\\', "/").to_lowercase();
            let app_trimmed = app_norm.trim_end_matches('/');
            let exe_target = format!("{app_trimmed}/antigravity.exe");
            let res_target = format!("{app_trimmed}/resources/");

            if let Some(procs) = ops.list_windows_processes()? {
                for (pid, image_path) in procs {
                    if pid == self_pid {
                        continue;
                    }
                    let clean_img = strip_verbatim_prefix(Path::new(&image_path));
                    let norm = clean_img
                        .to_string_lossy()
                        .trim_start()
                        .trim_start_matches('"')
                        .replace('\\', "/")
                        .to_lowercase();
                    if matches_win32_entry(&norm, &exe_target, &res_target, scope) {
                        return Ok(true);
                    }
                }
                return Ok(false);
            }

            let out = run_checked(
                ops,
                "powershell",
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId)|$($_.ExecutablePath) $($_.CommandLine)\" }",
                ],
            )?;
            for line in out.stdout.lines() {
                let (pid_opt, rest) = split_optional_pid(line, '|');
                if pid_opt == Some(self_pid) {
                    continue;
                }
                let norm = rest
                    .trim_start()
                    .trim_start_matches('"')
                    .replace('\\', "/")
                    .to_lowercase();
                if matches_win32_entry(&norm, &exe_target, &res_target, scope) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Platform::Darwin => {
            let out = run_checked(ops, "/bin/ps", &["-axo", "pid=,command="])?;
            let app_norm = app.to_string_lossy().replace('\\', "/");
            let macos_prefix = format!("{app_norm}/Contents/MacOS/");
            let frameworks_prefix = format!("{app_norm}/Contents/Frameworks/");
            let resources_prefix = format!("{app_norm}/Contents/Resources/");
            for line in out.stdout.lines() {
                let (pid_opt, rest) = split_optional_pid_ws(line);
                if pid_opt == Some(self_pid) {
                    continue;
                }
                let norm = rest.trim_start().replace('\\', "/");
                if is_self_tool_command(&norm) {
                    continue;
                }
                let matched = match scope {
                    ProcessMatchScope::MainExecutableOnly => norm.starts_with(&macos_prefix),
                    ProcessMatchScope::AllAppProcesses => {
                        norm.starts_with(&macos_prefix)
                            || norm.starts_with(&frameworks_prefix)
                            || norm.starts_with(&resources_prefix)
                    }
                };
                if matched {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Platform::Linux => {
            let out = run_checked(ops, "/bin/ps", &["-axo", "pid=,command="])?;
            let app_norm = app.to_string_lossy().replace('\\', "/");
            let exe_target = format!("{app_norm}/antigravity");
            let exe_space = format!("{exe_target} ");
            let res_target = format!("{app_norm}/resources/");
            for line in out.stdout.lines() {
                let (pid_opt, rest) = split_optional_pid_ws(line);
                if pid_opt == Some(self_pid) {
                    continue;
                }
                let norm = rest.trim_start().replace('\\', "/");
                if norm.contains("patcher.py") || is_self_tool_command(&norm) {
                    continue;
                }
                let matched = match scope {
                    ProcessMatchScope::MainExecutableOnly => {
                        norm == exe_target || norm.starts_with(&exe_space)
                    }
                    ProcessMatchScope::AllAppProcesses => {
                        norm == exe_target
                            || norm.starts_with(&exe_space)
                            || norm.starts_with(&res_target)
                    }
                };
                if matched {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

pub fn ensure_stopped(
    app: &Path,
    platform: Platform,
    ops: &dyn SystemOps,
) -> Result<(), String> {
    if is_app_running(app, platform, ops)? {
        if platform == Platform::Darwin {
            return Err("Quit Antigravity completely with Cmd+Q, then retry".to_string());
        }
        return Err(
            "Quit Antigravity completely before patching or restoring, then retry".to_string(),
        );
    }
    Ok(())
}

fn split_optional_pid(line: &str, sep: char) -> (Option<u32>, &str) {
    if let Some((head, tail)) = line.split_once(sep) {
        if let Ok(pid) = head.trim().parse::<u32>() {
            return (Some(pid), tail);
        }
    }
    (None, line)
}

fn split_optional_pid_ws(line: &str) -> (Option<u32>, &str) {
    let trimmed = line.trim_start();
    if let Some(idx) = trimmed.find(char::is_whitespace) {
        let (head, tail) = trimmed.split_at(idx);
        if let Ok(pid) = head.parse::<u32>() {
            return (Some(pid), tail.trim_start());
        }
    }
    (None, line)
}

fn is_self_tool_command(cmd_line: &str) -> bool {
    let first_token = cmd_line.split_whitespace().next().unwrap_or("");
    let base = first_token.rsplit('/').next().unwrap_or(first_token);
    base == "agy-zh" || base == "agy-zh.exe"
}

pub fn leave_app_dir_if_inside(app: &Path) {
    #[cfg(windows)]
    {
        if let (Ok(cwd), Ok(app_resolved)) = (env::current_dir(), fs::canonicalize(app)) {
            let cwd_canon = fs::canonicalize(&cwd).unwrap_or(cwd);
            let cwd_clean = strip_verbatim_prefix(&cwd_canon)
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase();
            let app_clean = strip_verbatim_prefix(&app_resolved)
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase();
            let app_trimmed = app_clean.trim_end_matches('/');
            let app_prefix = format!("{app_trimmed}/");
            if cwd_clean == app_trimmed || cwd_clean.starts_with(&app_prefix) {
                if let Some(parent) = app.parent() {
                    let _ = env::set_current_dir(parent);
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

#[cfg(test)]
pub(crate) struct TestTempDir {
    path: PathBuf,
}

#[cfg(test)]
impl TestTempDir {
    pub fn new() -> (Self, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let base = strip_verbatim_prefix(
            &fs::canonicalize(env::temp_dir()).unwrap_or_else(|_| env::temp_dir()),
        );
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = base.join(format!("agy-zh-test-{}-{}", std::process::id(), n));
            if fs::create_dir(&dir).is_ok() {
                let canon = strip_verbatim_prefix(&fs::canonicalize(&dir).unwrap());
                return (Self { path: canon.clone() }, canon);
            }
        }
    }
}

#[cfg(test)]
impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
pub(crate) fn canonical_temp_dir() -> (TestTempDir, PathBuf) {
    TestTempDir::new()
}

#[cfg(test)]
pub(crate) fn create_test_dir_link(target: &Path, link: &Path) {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).unwrap();
    }
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(target, link).is_err() {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let link_win = link.to_string_lossy().replace('/', "\\");
            let target_win = target.to_string_lossy().replace('/', "\\");
            let out = Command::new("cmd")
                .args(["/c", "mklink", "/J", &link_win, &target_win])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "mklink /J failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPsOps {
        stdout: String,
        writable: bool,
    }

    impl SystemOps for MockPsOps {
        fn run_cmd(&self, _program: &str, _args: &[&str]) -> Result<CmdOutput, String> {
            Ok(CmdOutput {
                status_code: 0,
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }

        fn clone_dir(&self, src: &Path, dst: &Path) -> Result<(), String> {
            copy_dir_recursive(src, dst)
        }

        fn is_writable(&self, _path: &Path, _is_dir: bool) -> bool {
            self.writable
        }
    }

    #[test]
    fn linux_default_app_detection_from_candidates() {
        let (_tmp, root) = canonical_temp_dir();
        let app = root.join("Antigravity-x64");
        fs::create_dir_all(app.join("resources")).unwrap();
        fs::write(app.join("resources/app.asar"), b"fake").unwrap();
        fs::write(app.join("antigravity"), b"elf").unwrap();

        let detected = detect_app_from_candidates(
            &[root.join("missing"), app.clone()],
            "antigravity",
            None,
            1,
        );
        assert_eq!(detected.as_ref(), Some(&app));
    }

    #[test]
    fn escaping_symlink_and_running_process_and_permissions_rejected() {
        let (_tmp, root) = canonical_temp_dir();
        let app = root.join("Antigravity-x64");
        fs::create_dir_all(app.join("resources")).unwrap();
        fs::write(app.join("antigravity"), b"elf").unwrap();
        fs::write(app.join("resources/app.asar"), b"asar").unwrap();

        let outside = root.join("outside-secret");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"secret").unwrap();
        let escape_link = app.join("resources/escape-link");
        create_test_dir_link(&outside, &escape_link);

        let err = assert_app_path(&app).unwrap_err();
        assert!(err.contains("symlink escaping"), "unexpected error: {err}");
        fs::remove_dir(&escape_link)
            .or_else(|_| fs::remove_file(&escape_link))
            .unwrap();

        let internal_sub = app.join("resources/sub");
        fs::create_dir_all(&internal_sub).unwrap();
        let internal_link = app.join("resources/internal-link");
        create_test_dir_link(&internal_sub, &internal_link);
        assert!(assert_app_path(&app).is_ok());
        fs::remove_dir(&internal_link)
            .or_else(|_| fs::remove_file(&internal_link))
            .unwrap();

        let running_linux = MockPsOps {
            stdout: format!("{}/antigravity --type=zygote\n", app.display()),
            writable: true,
        };
        let err = ensure_stopped(&app, Platform::Linux, &running_linux).unwrap_err();
        assert!(err.contains("Quit Antigravity completely"));

        let running_win = MockPsOps {
            stdout: format!(
                "9999|{}\\resources\\bin\\language_server.exe --standalone\n",
                app.display()
            ),
            writable: true,
        };
        let err = ensure_stopped(&app, Platform::Win32, &running_win).unwrap_err();
        assert!(err.contains("Quit Antigravity completely"));

        let denied = MockPsOps {
            stdout: String::new(),
            writable: false,
        };
        let err = ensure_writable(&[app.as_path()], Platform::Linux, &denied).unwrap_err();
        assert!(err.contains("Insufficient permissions"));
    }

    #[test]
    fn cli_args_mentioning_app_path_do_not_trigger_running_false_positive() {
        let darwin_app = PathBuf::from("/private/tmp/test/Antigravity.app");
        let darwin_unrelated = MockPsOps {
            stdout: concat!(
                "46777 agy --dangerously-skip-permissions --prompt-interactive \"check /private/tmp/test/Antigravity.app/Contents/MacOS/Antigravity\"\n",
                "89766 zsh -c 'ls /private/tmp/test/Antigravity.app/Contents/MacOS/Antigravity'\n",
                "12345 /Applications/Antigravity.app/Contents/MacOS/Antigravity\n"
            )
            .to_string(),
            writable: true,
        };
        assert!(!is_app_running(&darwin_app, Platform::Darwin, &darwin_unrelated).unwrap());

        for real_cmd in [
            "74988 /private/tmp/test/Antigravity.app/Contents/MacOS/Antigravity --remote-debugging-port=19222\n",
            "74990 /private/tmp/test/Antigravity.app/Contents/Frameworks/Antigravity Helper.app/Contents/MacOS/Antigravity Helper --type=gpu-process\n",
            "74991 /private/tmp/test/Antigravity.app/Contents/Resources/bin/language_server --standalone\n",
        ] {
            let darwin_running = MockPsOps {
                stdout: real_cmd.to_string(),
                writable: true,
            };
            assert!(
                is_app_running(&darwin_app, Platform::Darwin, &darwin_running).unwrap(),
                "expected running for: {real_cmd}"
            );
        }

        let linux_app = PathBuf::from("/opt/Antigravity");
        let linux_unrelated = MockPsOps {
            stdout: "1001 bash -c 'cat /opt/Antigravity/antigravity /opt/Antigravity/resources/app.asar'\n".to_string(),
            writable: true,
        };
        assert!(!is_app_running(&linux_app, Platform::Linux, &linux_unrelated).unwrap());
        let linux_running = MockPsOps {
            stdout: "1002 /opt/Antigravity/antigravity --type=renderer\n".to_string(),
            writable: true,
        };
        assert!(is_app_running(&linux_app, Platform::Linux, &linux_running).unwrap());

        let win_app = PathBuf::from(r"C:\Users\demo\AppData\Local\Programs\Antigravity");
        let win_unrelated = MockPsOps {
            stdout: r#"2001|C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -Command "Get-Item C:\Users\demo\AppData\Local\Programs\Antigravity\Antigravity.exe""#.to_string(),
            writable: true,
        };
        assert!(!is_app_running(&win_app, Platform::Win32, &win_unrelated).unwrap());
        let win_running = MockPsOps {
            stdout: r#"2002|"C:\Users\demo\AppData\Local\Programs\Antigravity\Antigravity.exe" --remote-debugging-port=9222"#.to_string(),
            writable: true,
        };
        assert!(is_app_running(&win_app, Platform::Win32, &win_running).unwrap());
    }

    struct MockNativeWinOps {
        procs: Vec<(u32, String)>,
    }

    impl SystemOps for MockNativeWinOps {
        fn run_cmd(&self, _program: &str, _args: &[&str]) -> Result<CmdOutput, String> {
            panic!("PowerShell should not be invoked when native enumeration is available");
        }

        fn clone_dir(&self, _src: &Path, _dst: &Path) -> Result<(), String> {
            Ok(())
        }

        fn is_writable(&self, _path: &Path, _is_dir: bool) -> bool {
            true
        }

        fn list_windows_processes(&self) -> Result<Option<Vec<(u32, String)>>, String> {
            Ok(Some(self.procs.clone()))
        }
    }

    #[test]
    fn main_app_running_excludes_resources_while_ensure_stopped_catches_them() {
        let win_app = PathBuf::from(r"\\?\C:\Users\Demo\AppData\Local\Programs\Antigravity\");
        let win_ls_ps = MockPsOps {
            stdout: r"9999|C:\Users\demo\AppData\Local\Programs\Antigravity\resources\bin\language_server.exe --standalone".to_string(),
            writable: true,
        };
        assert!(!is_main_app_running(&win_app, Platform::Win32, &win_ls_ps).unwrap());
        assert!(is_app_running(&win_app, Platform::Win32, &win_ls_ps).unwrap());
        assert!(ensure_stopped(&win_app, Platform::Win32, &win_ls_ps).is_err());

        let win_ls_native = MockNativeWinOps {
            procs: vec![
                (
                    std::process::id(),
                    r"C:\Users\demo\AppData\Local\Programs\Antigravity\Antigravity.exe".to_string(),
                ),
                (
                    8888,
                    r"\\?\C:\Users\demo\AppData\Local\Programs\Antigravity\resources\bin\language_server.exe"
                        .to_string(),
                ),
            ],
        };
        assert!(!is_main_app_running(&win_app, Platform::Win32, &win_ls_native).unwrap());
        assert!(is_app_running(&win_app, Platform::Win32, &win_ls_native).unwrap());
        assert!(ensure_stopped(&win_app, Platform::Win32, &win_ls_native).is_err());

        let win_main_native = MockNativeWinOps {
            procs: vec![(
                7777,
                r"\\?\C:\Users\DEMO\AppData\Local\Programs\Antigravity\Antigravity.exe".to_string(),
            )],
        };
        assert!(is_main_app_running(&win_app, Platform::Win32, &win_main_native).unwrap());
        assert!(is_app_running(&win_app, Platform::Win32, &win_main_native).unwrap());

        let darwin_app = PathBuf::from("/Applications/Antigravity.app");
        let darwin_ls = MockPsOps {
            stdout: "100 /Applications/Antigravity.app/Contents/Resources/bin/language_server --standalone\n".to_string(),
            writable: true,
        };
        assert!(!is_main_app_running(&darwin_app, Platform::Darwin, &darwin_ls).unwrap());
        assert!(is_app_running(&darwin_app, Platform::Darwin, &darwin_ls).unwrap());
        assert!(ensure_stopped(&darwin_app, Platform::Darwin, &darwin_ls).is_err());

        let darwin_main = MockPsOps {
            stdout: "101 /Applications/Antigravity.app/Contents/MacOS/Antigravity\n".to_string(),
            writable: true,
        };
        assert!(is_main_app_running(&darwin_app, Platform::Darwin, &darwin_main).unwrap());

        let linux_app = PathBuf::from("/opt/Antigravity");
        let linux_ls = MockPsOps {
            stdout: "200 /opt/Antigravity/resources/bin/language_server --standalone\n".to_string(),
            writable: true,
        };
        assert!(!is_main_app_running(&linux_app, Platform::Linux, &linux_ls).unwrap());
        assert!(is_app_running(&linux_app, Platform::Linux, &linux_ls).unwrap());
        assert!(ensure_stopped(&linux_app, Platform::Linux, &linux_ls).is_err());

        let linux_main = MockPsOps {
            stdout: "201 /opt/Antigravity/antigravity --type=renderer\n".to_string(),
            writable: true,
        };
        assert!(is_main_app_running(&linux_app, Platform::Linux, &linux_main).unwrap());
    }

    #[test]
    fn win32_native_process_list_skips_powershell() {
        let win_app = PathBuf::from(r"C:\Users\demo\AppData\Local\Programs\Antigravity");
        let ops = MockNativeWinOps {
            procs: vec![(4321, r"C:\Windows\System32\notepad.exe".to_string())],
        };
        assert!(!is_main_app_running(&win_app, Platform::Win32, &ops).unwrap());
        assert!(!is_app_running(&win_app, Platform::Win32, &ops).unwrap());
    }
}
