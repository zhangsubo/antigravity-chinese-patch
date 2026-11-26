use crate::platform::{
    default_app_path, home_dir, is_link, lexically_abs, run_checked, strip_verbatim_prefix,
    user_data_dir, user_log_file, Platform, RealSystemOps, SystemOps,
};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const LAUNCHER_BUNDLE_ID: &str = "io.github.zhangsubo.antigravity-chinese-patch.launcher";
const WINDOWS_SHORTCUT_NAME: &str = "Antigravity 中文.lnk";
const WINDOWS_SHORTCUT_DESCRIPTION: &str = "Antigravity 中文 (CDP Dynamic Mode)";
const WINDOWS_SHORTCUT_SHOW_MINNOACTIVE: i32 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsShortcutSpec {
    pub lnk_path: PathBuf,
    pub target_path: PathBuf,
    pub arguments: String,
    pub working_directory: PathBuf,
    pub icon_path: PathBuf,
    pub icon_index: i32,
    pub show_cmd: i32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShortcutReport {
    pub action: String,
    pub platform: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_binary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

pub fn posix_single_quote(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "'\\''"))
}

pub fn desktop_exec_arg_quote(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' | '\r' => {}
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

pub fn format_windows_shortcut_args(app_path: &Path) -> String {
    let clean = strip_verbatim_prefix(app_path);
    let raw = clean.to_string_lossy();
    let mut escaped = String::with_capacity(raw.len());
    let mut pending_backslashes = 0usize;
    for ch in raw.chars() {
        match ch {
            '\\' => {
                pending_backslashes += 1;
            }
            '"' => {
                for _ in 0..(pending_backslashes * 2 + 1) {
                    escaped.push('\\');
                }
                escaped.push('"');
                pending_backslashes = 0;
            }
            _ => {
                for _ in 0..pending_backslashes {
                    escaped.push('\\');
                }
                pending_backslashes = 0;
                escaped.push(ch);
            }
        }
    }
    for _ in 0..(pending_backslashes * 2) {
        escaped.push('\\');
    }
    format!("run --app \"{escaped}\"")
}

fn win_parent_dir(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        path.parent().map(Path::to_path_buf).unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        let raw = path.to_string_lossy();
        if raw.contains('\\') {
            let trimmed = raw.trim_end_matches(['\\', '/']);
            match trimmed.rfind(['\\', '/']) {
                Some(idx) if idx > 0 && trimmed.as_bytes()[idx - 1] == b':' => {
                    PathBuf::from(&trimmed[..=idx])
                }
                Some(idx) => PathBuf::from(&trimmed[..idx]),
                None => PathBuf::new(),
            }
        } else {
            path.parent().map(Path::to_path_buf).unwrap_or_default()
        }
    }
}

fn win_join_file(dir: &Path, file: &str) -> PathBuf {
    #[cfg(windows)]
    {
        dir.join(file)
    }
    #[cfg(not(windows))]
    {
        let raw = dir.to_string_lossy();
        if raw.contains('\\') {
            PathBuf::from(format!("{}\\{file}", raw.trim_end_matches(['\\', '/'])))
        } else {
            dir.join(file)
        }
    }
}

pub fn build_windows_shortcut_spec(
    lnk_path: &Path,
    bin_path: &Path,
    app_path: &Path,
) -> WindowsShortcutSpec {
    let clean_lnk = strip_verbatim_prefix(lnk_path);
    let clean_bin = strip_verbatim_prefix(bin_path);
    let clean_app = strip_verbatim_prefix(app_path);
    WindowsShortcutSpec {
        lnk_path: clean_lnk,
        working_directory: win_parent_dir(&clean_bin),
        target_path: clean_bin,
        arguments: format_windows_shortcut_args(&clean_app),
        icon_path: win_join_file(&clean_app, "Antigravity.exe"),
        icon_index: 0,
        show_cmd: WINDOWS_SHORTCUT_SHOW_MINNOACTIVE,
        description: WINDOWS_SHORTCUT_DESCRIPTION.to_string(),
    }
}

pub fn render_macos_launcher_script(bin_path: &Path, app_path: &Path, log_path: &Path) -> String {
    format!(
        "#!/bin/sh\nset -eu\nLOG_FILE={}\nmkdir -p \"$(dirname \"$LOG_FILE\")\"\nexec {} run --app {} >> \"$LOG_FILE\" 2>&1\n",
        posix_single_quote(&log_path.to_string_lossy()),
        posix_single_quote(&bin_path.to_string_lossy()),
        posix_single_quote(&app_path.to_string_lossy()),
    )
}

pub fn render_linux_desktop_entry(
    bin_path: &Path,
    app_path: &Path,
    icon_value: &str,
) -> String {
    let exec_line = format!(
        "{} run --app {}",
        desktop_exec_arg_quote(&bin_path.to_string_lossy()),
        desktop_exec_arg_quote(&app_path.to_string_lossy()),
    );
    format!(
        "[Desktop Entry]\nType=Application\nName=Antigravity 中文\nComment=Launch Google Antigravity Desktop with dynamic Simplified Chinese localization\nExec={exec_line}\nIcon={icon_value}\nTerminal=false\nCategories=Development;IDE;\nStartupNotify=true\n"
    )
}

pub fn install_stable_binary_in(bin_dir: &Path) -> Result<PathBuf, String> {
    let current_exe = env::current_exe()
        .map_err(|e| format!("Failed to locate current agy-zh executable: {e}"))?;
    let bin_name = if Platform::host() == Platform::Win32 {
        "agy-zh.exe"
    } else {
        "agy-zh"
    };
    fs::create_dir_all(bin_dir)
        .map_err(|e| format!("Failed to create {}: {e}", bin_dir.display()))?;
    let target = bin_dir.join(bin_name);

    let src_bytes = fs::read(&current_exe)
        .map_err(|e| format!("Failed to read {}: {e}", current_exe.display()))?;
    if let Ok(existing_bytes) = fs::read(&target) {
        if existing_bytes == src_bytes {
            return Ok(target);
        }
    }

    let tmp_target = bin_dir.join(format!(".{bin_name}.tmp.{}", std::process::id()));
    fs::write(&tmp_target, &src_bytes)
        .map_err(|e| format!("Failed to write {}: {e}", tmp_target.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_target, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to chmod {}: {e}", tmp_target.display()))?;
    }
    fs::rename(&tmp_target, &target)
        .map_err(|e| format!("Failed to install stable binary to {}: {e}", target.display()))?;
    Ok(target)
}

pub fn default_shortcut_output_dir(platform: Platform, ops: &dyn SystemOps) -> PathBuf {
    match platform {
        Platform::Darwin => {
            let sys_apps = PathBuf::from("/Applications");
            if ops.is_writable(&sys_apps, true) {
                sys_apps
            } else {
                home_dir()
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join("Applications")
            }
        }
        Platform::Linux => env::var_os("XDG_DATA_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".local/share")))
            .unwrap_or_else(env::temp_dir)
            .join("applications"),
        Platform::Win32 => home_dir()
            .unwrap_or_else(env::temp_dir)
            .join("Desktop"),
    }
}

pub fn manage_shortcut(
    app: Option<&Path>,
    output_dir: Option<&Path>,
    bin_dir: Option<&Path>,
    remove: bool,
) -> Result<ShortcutReport, String> {
    let ops = RealSystemOps;
    manage_shortcut_with(app, output_dir, bin_dir, remove, &ops)
}

pub fn manage_shortcut_with(
    app: Option<&Path>,
    output_dir: Option<&Path>,
    bin_dir: Option<&Path>,
    remove: bool,
    ops: &dyn SystemOps,
) -> Result<ShortcutReport, String> {
    let platform = Platform::host();
    let app_path = lexically_abs(app.unwrap_or(&default_app_path()))?;
    let out_dir = match output_dir {
        Some(dir) => lexically_abs(dir)?,
        None => default_shortcut_output_dir(platform, ops),
    };
    let resolved_bin_dir = match bin_dir {
        Some(dir) => lexically_abs(dir)?,
        None => user_data_dir().join("bin"),
    };

    match platform {
        Platform::Darwin => manage_macos_shortcut(&app_path, &out_dir, &resolved_bin_dir, remove, ops),
        Platform::Linux => manage_linux_shortcut(&app_path, &out_dir, &resolved_bin_dir, remove),
        Platform::Win32 => manage_windows_shortcut(&app_path, &out_dir, &resolved_bin_dir, remove),
    }
}

fn manage_macos_shortcut(
    app_path: &Path,
    out_dir: &Path,
    bin_dir: &Path,
    remove: bool,
    ops: &dyn SystemOps,
) -> Result<ShortcutReport, String> {
    let bundle = out_dir.join("Antigravity 中文.app");
    if remove {
        if bundle.exists() || is_link(&bundle) {
            if is_link(&bundle) {
                fs::remove_file(&bundle)
                    .map_err(|e| format!("Failed to remove symlink {}: {e}", bundle.display()))?;
            } else {
                fs::remove_dir_all(&bundle)
                    .map_err(|e| format!("Failed to remove {}: {e}", bundle.display()))?;
            }
        }
        return Ok(ShortcutReport {
            action: "removed".to_string(),
            platform: "darwin".to_string(),
            path: bundle.to_string_lossy().into_owned(),
            stable_binary: None,
            app: None,
        });
    }

    if !app_path.is_dir() {
        return Err(format!(
            "Target Antigravity.app not found: {}",
            app_path.display()
        ));
    }

    fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create output directory {}: {e}", out_dir.display()))?;
    let stable_bin = install_stable_binary_in(bin_dir)?;
    let contents = bundle.join("Contents");
    let macos_dir = contents.join("MacOS");
    let res_dir = contents.join("Resources");
    fs::create_dir_all(&macos_dir)
        .map_err(|e| format!("Failed to create {}: {e}", macos_dir.display()))?;
    fs::create_dir_all(&res_dir)
        .map_err(|e| format!("Failed to create {}: {e}", res_dir.display()))?;

    let mut has_icon = false;
    if let Ok(entries) = fs::read_dir(app_path.join("Contents/Resources")) {
        let mut icns: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("icns"))
            .collect();
        icns.sort();
        if let Some(first_icns) = icns.first() {
            fs::copy(first_icns, res_dir.join("app.icns"))
                .map_err(|e| format!("Failed to copy icon: {e}"))?;
            has_icon = true;
        }
    }

    let launcher_path = macos_dir.join("launcher");
    let script = render_macos_launcher_script(&stable_bin, app_path, &user_log_file());
    fs::write(&launcher_path, script.as_bytes())
        .map_err(|e| format!("Failed to write launcher script: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&launcher_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to chmod launcher script: {e}"))?;
    }

    let mut dict = plist::Dictionary::new();
    dict.insert(
        "CFBundleIdentifier".to_string(),
        plist::Value::String(LAUNCHER_BUNDLE_ID.to_string()),
    );
    dict.insert(
        "CFBundleName".to_string(),
        plist::Value::String("Antigravity 中文".to_string()),
    );
    dict.insert(
        "CFBundleDisplayName".to_string(),
        plist::Value::String("Antigravity 中文".to_string()),
    );
    dict.insert(
        "CFBundleExecutable".to_string(),
        plist::Value::String("launcher".to_string()),
    );
    dict.insert(
        "CFBundlePackageType".to_string(),
        plist::Value::String("APPL".to_string()),
    );
    dict.insert(
        "CFBundleShortVersionString".to_string(),
        plist::Value::String(env!("CARGO_PKG_VERSION").to_string()),
    );
    dict.insert("LSUIElement".to_string(), plist::Value::Boolean(true));
    if has_icon {
        dict.insert(
            "CFBundleIconFile".to_string(),
            plist::Value::String("app.icns".to_string()),
        );
    }
    plist::to_file_xml(contents.join("Info.plist"), &plist::Value::Dictionary(dict))
        .map_err(|e| format!("Failed to write shortcut Info.plist: {e}"))?;

    let bundle_str = bundle.to_string_lossy();
    run_checked(
        ops,
        "/usr/bin/codesign",
        &["--force", "--sign", "-", bundle_str.as_ref()],
    )?;

    Ok(ShortcutReport {
        action: "created".to_string(),
        platform: "darwin".to_string(),
        path: bundle.to_string_lossy().into_owned(),
        stable_binary: Some(stable_bin.to_string_lossy().into_owned()),
        app: Some(app_path.to_string_lossy().into_owned()),
    })
}

fn manage_linux_shortcut(
    app_path: &Path,
    out_dir: &Path,
    bin_dir: &Path,
    remove: bool,
) -> Result<ShortcutReport, String> {
    let desktop_file = out_dir.join("antigravity-zh.desktop");
    if remove {
        if desktop_file.exists() || is_link(&desktop_file) {
            fs::remove_file(&desktop_file)
                .map_err(|e| format!("Failed to remove {}: {e}", desktop_file.display()))?;
        }
        return Ok(ShortcutReport {
            action: "removed".to_string(),
            platform: "linux".to_string(),
            path: desktop_file.to_string_lossy().into_owned(),
            stable_binary: None,
            app: None,
        });
    }

    fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create {}: {e}", out_dir.display()))?;
    let stable_bin = install_stable_binary_in(bin_dir)?;
    let icon_candidate = app_path.join("resources/app/resources/linux/code.png");
    let icon = if icon_candidate.is_file() {
        icon_candidate.to_string_lossy().into_owned()
    } else {
        "antigravity".to_string()
    };
    let content = render_linux_desktop_entry(&stable_bin, app_path, &icon);
    fs::write(&desktop_file, content.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", desktop_file.display()))?;

    Ok(ShortcutReport {
        action: "created".to_string(),
        platform: "linux".to_string(),
        path: desktop_file.to_string_lossy().into_owned(),
        stable_binary: Some(stable_bin.to_string_lossy().into_owned()),
        app: Some(app_path.to_string_lossy().into_owned()),
    })
}

fn manage_windows_shortcut(
    app_path: &Path,
    out_dir: &Path,
    bin_dir: &Path,
    remove: bool,
) -> Result<ShortcutReport, String> {
    let lnk = out_dir.join(WINDOWS_SHORTCUT_NAME);
    if remove {
        if lnk.exists() || is_link(&lnk) {
            fs::remove_file(&lnk)
                .map_err(|e| format!("Failed to remove {}: {e}", lnk.display()))?;
        }
        return Ok(ShortcutReport {
            action: "removed".to_string(),
            platform: "win32".to_string(),
            path: lnk.to_string_lossy().into_owned(),
            stable_binary: None,
            app: None,
        });
    }

    fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create {}: {e}", out_dir.display()))?;
    let stable_bin = install_stable_binary_in(bin_dir)?;
    let spec = build_windows_shortcut_spec(&lnk, &stable_bin, app_path);
    #[cfg(target_os = "windows")]
    {
        win_shortcut::create_shortcut(&spec)?;
        Ok(ShortcutReport {
            action: "created".to_string(),
            platform: "win32".to_string(),
            path: lnk.to_string_lossy().into_owned(),
            stable_binary: Some(stable_bin.to_string_lossy().into_owned()),
            app: Some(app_path.to_string_lossy().into_owned()),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = &spec;
        Err("Windows shortcuts can only be created on Windows".to_string())
    }
}

#[cfg(target_os = "windows")]
mod win_shortcut {
    use super::WindowsShortcutSpec;
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    #[cfg(test)]
    use windows::Win32::System::Com::STGM_READ;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD;

    struct ComGuard {
        should_uninit: bool,
    }

    impl ComGuard {
        fn init() -> Result<Self, String> {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if hr.is_ok() {
                Ok(Self {
                    should_uninit: true,
                })
            } else if hr == RPC_E_CHANGED_MODE {
                Ok(Self {
                    should_uninit: false,
                })
            } else {
                Err(format!("Failed to initialize COM: {hr:?}"))
            }
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.should_uninit {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn to_wide_null(os: &OsStr) -> Vec<u16> {
        os.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn str_to_wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[cfg(test)]
    fn from_wide_null(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        OsString::from_wide(&buf[..len])
            .to_string_lossy()
            .into_owned()
    }

    pub fn create_shortcut(spec: &WindowsShortcutSpec) -> Result<(), String> {
        let _com = ComGuard::init()?;
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Failed to create IShellLinkW: {e}"))?;

            let target_w = to_wide_null(spec.target_path.as_os_str());
            link.SetPath(PCWSTR(target_w.as_ptr()))
                .map_err(|e| format!("Failed to set shortcut target path: {e}"))?;

            let args_w = str_to_wide_null(&spec.arguments);
            link.SetArguments(PCWSTR(args_w.as_ptr()))
                .map_err(|e| format!("Failed to set shortcut arguments: {e}"))?;

            let workdir_w = to_wide_null(spec.working_directory.as_os_str());
            link.SetWorkingDirectory(PCWSTR(workdir_w.as_ptr()))
                .map_err(|e| format!("Failed to set shortcut working directory: {e}"))?;

            let icon_w = to_wide_null(spec.icon_path.as_os_str());
            link.SetIconLocation(PCWSTR(icon_w.as_ptr()), spec.icon_index)
                .map_err(|e| format!("Failed to set shortcut icon location: {e}"))?;

            link.SetShowCmd(SHOW_WINDOW_CMD(spec.show_cmd))
                .map_err(|e| format!("Failed to set shortcut show command: {e}"))?;

            let desc_w = str_to_wide_null(&spec.description);
            link.SetDescription(PCWSTR(desc_w.as_ptr()))
                .map_err(|e| format!("Failed to set shortcut description: {e}"))?;

            let persist: IPersistFile = link
                .cast()
                .map_err(|e| format!("Failed to query IPersistFile: {e}"))?;
            let lnk_w = to_wide_null(spec.lnk_path.as_os_str());
            persist
                .Save(PCWSTR(lnk_w.as_ptr()), true)
                .map_err(|e| format!("Failed to save shortcut {}: {e}", spec.lnk_path.display()))?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn read_shortcut(lnk_path: &Path) -> Result<WindowsShortcutSpec, String> {
        let _com = ComGuard::init()?;
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Failed to create IShellLinkW: {e}"))?;
            let persist: IPersistFile = link
                .cast()
                .map_err(|e| format!("Failed to query IPersistFile: {e}"))?;
            let lnk_w = to_wide_null(lnk_path.as_os_str());
            persist
                .Load(PCWSTR(lnk_w.as_ptr()), STGM_READ)
                .map_err(|e| format!("Failed to load shortcut {}: {e}", lnk_path.display()))?;

            let mut buf = [0u16; 1024];
            link.GetPath(&mut buf, std::ptr::null_mut(), 0)
                .map_err(|e| format!("Failed to read shortcut target path: {e}"))?;
            let target_path = PathBuf::from(from_wide_null(&buf));

            buf.fill(0);
            link.GetArguments(&mut buf)
                .map_err(|e| format!("Failed to read shortcut arguments: {e}"))?;
            let arguments = from_wide_null(&buf);

            buf.fill(0);
            link.GetWorkingDirectory(&mut buf)
                .map_err(|e| format!("Failed to read shortcut working directory: {e}"))?;
            let working_directory = PathBuf::from(from_wide_null(&buf));

            buf.fill(0);
            let mut icon_index = 0i32;
            link.GetIconLocation(&mut buf, &mut icon_index)
                .map_err(|e| format!("Failed to read shortcut icon location: {e}"))?;
            let icon_path = PathBuf::from(from_wide_null(&buf));

            let show_cmd = link
                .GetShowCmd()
                .map_err(|e| format!("Failed to read shortcut show command: {e}"))?
                .0;

            buf.fill(0);
            link.GetDescription(&mut buf)
                .map_err(|e| format!("Failed to read shortcut description: {e}"))?;
            let description = from_wide_null(&buf);

            Ok(WindowsShortcutSpec {
                lnk_path: lnk_path.to_path_buf(),
                target_path,
                arguments,
                working_directory,
                icon_path,
                icon_index,
                show_cmd,
                description,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::canonical_temp_dir;
    #[cfg(target_os = "macos")]
    use crate::platform::CmdOutput;

    #[cfg(target_os = "macos")]
    struct MockShortcutOps;
    #[cfg(target_os = "macos")]
    impl SystemOps for MockShortcutOps {
        fn run_cmd(&self, _program: &str, _args: &[&str]) -> Result<CmdOutput, String> {
            Ok(CmdOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }

        fn clone_dir(&self, _src: &Path, _dst: &Path) -> Result<(), String> {
            Ok(())
        }

        fn is_writable(&self, _path: &Path, _is_dir: bool) -> bool {
            true
        }
    }

    #[test]
    fn shell_desktop_and_powershell_quoting_escapes_injection_characters() {
        let evil = Path::new("/tmp/o'reilly $HOME `id`/Antigravity.app");
        let script = render_macos_launcher_script(
            Path::new("/Users/me/bin/agy-zh"),
            evil,
            Path::new("/Users/me/Library/Logs/agy.log"),
        );
        assert!(script.contains("'\\''reilly $HOME `id`/Antigravity.app'"));

        let desktop = render_linux_desktop_entry(
            Path::new("/usr/local/bin/agy-zh"),
            Path::new("/opt/App \"$USER\" `id`"),
            "antigravity",
        );
        assert!(desktop.contains(r#"Exec="/usr/local/bin/agy-zh" run --app "/opt/App \"\$USER\" \`id\`""#));

        let spec = build_windows_shortcut_spec(
            Path::new(r"\\?\C:\Users\o'neil\Desktop\桌面测试\Antigravity 中文.lnk"),
            Path::new(r"\\?\C:\Users\o'neil\bin\agy-zh.exe"),
            Path::new(r"\\?\C:\Program Files\O'Neil 测试\Antigravity\"),
        );
        assert_eq!(
            spec.lnk_path,
            Path::new(r"C:\Users\o'neil\Desktop\桌面测试\Antigravity 中文.lnk")
        );
        assert_eq!(spec.target_path, Path::new(r"C:\Users\o'neil\bin\agy-zh.exe"));
        assert_eq!(
            spec.working_directory,
            Path::new(r"C:\Users\o'neil\bin")
        );
        assert_eq!(
            spec.arguments,
            r#"run --app "C:\Program Files\O'Neil 测试\Antigravity\\""#
        );
        assert_eq!(
            spec.icon_path,
            Path::new(r"C:\Program Files\O'Neil 测试\Antigravity\Antigravity.exe")
        );
        assert_eq!(spec.icon_index, 0);
        assert_eq!(spec.show_cmd, 7);
        assert_eq!(spec.description, WINDOWS_SHORTCUT_DESCRIPTION);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_shortcut_create_and_remove_in_sandbox() {
        let (_tmp, root) = canonical_temp_dir();
        let fake_app = root.join("Antigravity.app");
        fs::create_dir_all(fake_app.join("Contents/Resources")).unwrap();
        fs::write(fake_app.join("Contents/Resources/electron.icns"), b"ICNS").unwrap();
        let out_dir = root.join("OutApps");
        let bin_dir = root.join("bin");

        let ops = MockShortcutOps;
        let created = manage_macos_shortcut(&fake_app, &out_dir, &bin_dir, false, &ops).unwrap();
        assert_eq!(created.action, "created");
        assert!(created.stable_binary.as_deref().unwrap().starts_with(bin_dir.to_string_lossy().as_ref()));
        let bundle = out_dir.join("Antigravity 中文.app");
        assert!(bundle.join("Contents/Info.plist").is_file());
        assert!(bundle.join("Contents/MacOS/launcher").is_file());
        assert!(bundle.join("Contents/Resources/app.icns").is_file());

        let plist_val = plist::from_file::<_, plist::Value>(bundle.join("Contents/Info.plist")).unwrap();
        let dict = plist_val.as_dictionary().unwrap();
        assert_eq!(
            dict.get("LSUIElement").and_then(|v| v.as_boolean()),
            Some(true)
        );
        assert_eq!(
            dict.get("CFBundleDisplayName").and_then(|v| v.as_string()),
            Some("Antigravity 中文")
        );

        let removed = manage_macos_shortcut(&fake_app, &out_dir, &bin_dir, true, &ops).unwrap();
        assert_eq!(removed.action, "removed");
        assert!(!bundle.exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_shortcut_create_and_remove_in_sandbox() {
        let (_tmp, root) = canonical_temp_dir();
        let fake_app = root.join("Antigravity 测试");
        fs::create_dir_all(&fake_app).unwrap();
        fs::write(fake_app.join("Antigravity.exe"), b"MZ").unwrap();
        let out_dir = root.join("桌面测试");
        let bin_dir = root.join("bin");

        let created = manage_windows_shortcut(&fake_app, &out_dir, &bin_dir, false).unwrap();
        assert_eq!(created.action, "created");
        assert_eq!(created.platform, "win32");

        let lnk = out_dir.join(WINDOWS_SHORTCUT_NAME);
        let stable_bin = bin_dir.join("agy-zh.exe");
        assert!(lnk.is_file(), "expected shortcut at {}", lnk.display());
        assert!(
            stable_bin.is_file(),
            "expected stable binary at {}",
            stable_bin.display()
        );

        let read_back = win_shortcut::read_shortcut(&lnk).unwrap();
        assert_eq!(
            read_back.target_path.to_string_lossy().to_lowercase(),
            stable_bin.to_string_lossy().to_lowercase()
        );
        assert!(read_back
            .arguments
            .contains(&fake_app.to_string_lossy().to_string()));
        assert_eq!(
            read_back.working_directory.to_string_lossy().to_lowercase(),
            bin_dir.to_string_lossy().to_lowercase()
        );
        assert_eq!(
            read_back.icon_path.to_string_lossy().to_lowercase(),
            fake_app.join("Antigravity.exe").to_string_lossy().to_lowercase()
        );
        assert_eq!(read_back.icon_index, 0);
        assert_eq!(read_back.show_cmd, 7);
        assert_eq!(read_back.description, WINDOWS_SHORTCUT_DESCRIPTION);

        let removed = manage_windows_shortcut(&fake_app, &out_dir, &bin_dir, true).unwrap();
        assert_eq!(removed.action, "removed");
        assert!(!lnk.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_shortcut_create_and_remove_in_sandbox() {
        let (_tmp, root) = canonical_temp_dir();
        let fake_app = root.join("Antigravity");
        fs::create_dir_all(fake_app.join("resources/app/resources/linux")).unwrap();
        fs::write(fake_app.join("resources/app/resources/linux/code.png"), b"PNG").unwrap();
        let out_dir = root.join("applications");
        let bin_dir = root.join("bin");

        let created = manage_linux_shortcut(&fake_app, &out_dir, &bin_dir, false).unwrap();
        assert_eq!(created.action, "created");
        assert_eq!(created.platform, "linux");
        let desktop_file = out_dir.join("antigravity-zh.desktop");
        assert!(desktop_file.is_file());
        let content = fs::read_to_string(&desktop_file).unwrap();
        assert!(content.contains("Name=Antigravity 中文"));
        assert!(content.contains("code.png"));

        let removed = manage_linux_shortcut(&fake_app, &out_dir, &bin_dir, true).unwrap();
        assert_eq!(removed.action, "removed");
        assert!(!desktop_file.exists());
    }
}
