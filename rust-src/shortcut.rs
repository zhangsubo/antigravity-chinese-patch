use crate::platform::{
    default_app_path, home_dir, is_link, lexically_abs, run_checked, user_data_dir, user_log_file,
    Platform, RealSystemOps, SystemOps,
};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const LAUNCHER_BUNDLE_ID: &str = "io.github.zhangsubo.antigravity-chinese-patch.launcher";

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

pub fn powershell_single_quote(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "''"))
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

pub fn render_windows_shortcut_ps(
    lnk_path: &Path,
    bin_path: &Path,
    app_path: &Path,
) -> String {
    let args = format!("run --app \"{}\"", app_path.to_string_lossy().replace('"', "`\""));
    format!(
        "$WshShell = New-Object -ComObject WScript.Shell; \
         $Shortcut = $WshShell.CreateShortcut({}); \
         $Shortcut.TargetPath = {}; \
         $Shortcut.Arguments = {}; \
         $Shortcut.WorkingDirectory = {}; \
         $Shortcut.IconLocation = {}; \
         $Shortcut.WindowStyle = 7; \
         $Shortcut.Description = 'Antigravity 中文 (CDP Dynamic Mode)'; \
         $Shortcut.Save()",
        powershell_single_quote(&lnk_path.to_string_lossy()),
        powershell_single_quote(&bin_path.to_string_lossy()),
        powershell_single_quote(&args),
        powershell_single_quote(
            &bin_path
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        powershell_single_quote(&format!("{},0", app_path.join("Antigravity.exe").to_string_lossy())),
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
        Platform::Win32 => manage_windows_shortcut(&app_path, &out_dir, &resolved_bin_dir, remove, ops),
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
    ops: &dyn SystemOps,
) -> Result<ShortcutReport, String> {
    let lnk = out_dir.join("Antigravity 中文.lnk");
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
    let ps = render_windows_shortcut_ps(&lnk, &stable_bin, app_path);
    run_checked(
        ops,
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", &ps],
    )?;

    Ok(ShortcutReport {
        action: "created".to_string(),
        platform: "win32".to_string(),
        path: lnk.to_string_lossy().into_owned(),
        stable_binary: Some(stable_bin.to_string_lossy().into_owned()),
        app: Some(app_path.to_string_lossy().into_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{canonical_temp_dir, CmdOutput};

    struct MockShortcutOps;
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

        let ps = render_windows_shortcut_ps(
            Path::new(r"C:\Users\o'neil\Desktop\Antigravity 中文.lnk"),
            Path::new(r"C:\Users\o'neil\bin\agy-zh.exe"),
            Path::new(r"C:\Program Files\O'Neil\Antigravity"),
        );
        assert!(ps.contains(r"'C:\Users\o''neil\Desktop\Antigravity 中文.lnk'"));
        assert!(ps.contains(r#"'run --app "C:\Program Files\O''Neil\Antigravity"'"#));
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
}
