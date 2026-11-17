use crate::asar::sha256_hex;
use crate::platform::{home_dir, is_link, lexically_abs, resolved_without_symlinks, Platform};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_CUSTOM_DICT_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Default)]
pub struct LoadedCustomDict {
    #[allow(dead_code)]
    pub path: Option<PathBuf>,
    pub entries: Vec<[String; 2]>,
    pub sha256: String,
}

impl LoadedCustomDict {
    pub fn empty() -> Self {
        Self {
            path: None,
            entries: Vec::new(),
            sha256: sha256_hex(b"[]"),
        }
    }
}

pub fn default_custom_dict_candidates(platform: Platform) -> Vec<PathBuf> {
    let home = home_dir();
    match platform {
        Platform::Darwin => {
            let mut list = Vec::new();
            if let Some(h) = &home {
                list.push(
                    h.join("Library/Application Support/antigravity-chinese-patch/custom-translations.txt"),
                );
                list.push(
                    h.join(".config/antigravity-chinese-patch/custom-translations.txt"),
                );
            }
            list
        }
        Platform::Linux => {
            let mut list = Vec::new();
            if let Some(xdg) = env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
                list.push(PathBuf::from(xdg).join("antigravity-chinese-patch/custom-translations.txt"));
            }
            if let Some(h) = &home {
                list.push(
                    h.join(".config/antigravity-chinese-patch/custom-translations.txt"),
                );
            }
            list
        }
        Platform::Win32 => {
            let mut list = Vec::new();
            if let Some(appdata) = env::var_os("APPDATA").filter(|s| !s.is_empty()) {
                list.push(
                    PathBuf::from(appdata).join("antigravity-chinese-patch/custom-translations.txt"),
                );
            } else if let Some(h) = &home {
                list.push(
                    h.join("AppData/Roaming/antigravity-chinese-patch/custom-translations.txt"),
                );
            }
            list
        }
    }
}

pub fn load_custom_translations(
    explicit_path: Option<&Path>,
    platform: Platform,
    trust_custom_translations: bool,
) -> Result<LoadedCustomDict, String> {
    let target = match explicit_path {
        Some(p) => {
            if !p.exists() && !is_link(p) {
                return Err(format!(
                    "Custom translations file not found: {}",
                    p.display()
                ));
            }
            p.to_path_buf()
        }
        None => {
            let found = default_custom_dict_candidates(platform)
                .into_iter()
                .find(|p| p.exists() || is_link(p));
            match found {
                Some(p) => p,
                None => return Ok(LoadedCustomDict::empty()),
            }
        }
    };

    load_custom_translations_from_path(&target, platform, trust_custom_translations)
}

pub fn load_custom_translations_from_path(
    path: &Path,
    platform: Platform,
    trust_custom_translations: bool,
) -> Result<LoadedCustomDict, String> {
    let abs = lexically_abs(path)?;
    if is_link(&abs) || !resolved_without_symlinks(&abs).unwrap_or(false) {
        return Err(format!(
            "Refusing symlinked custom translations path: {}",
            abs.display()
        ));
    }

    let meta = fs::symlink_metadata(&abs)
        .map_err(|e| format!("Failed to inspect {}: {e}", abs.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "Custom translations path must be a regular file: {}",
            abs.display()
        ));
    }
    if meta.len() > MAX_CUSTOM_DICT_BYTES {
        return Err(format!(
            "Custom translations file exceeds 512 KB limit ({} bytes): {}",
            meta.len(),
            abs.display()
        ));
    }

    if platform == Platform::Win32 && !trust_custom_translations {
        return Err(format!(
            "Refusing to load custom translations on Windows without explicit --trust-custom-translations ({})",
            abs.display()
        ));
    }

    #[cfg(unix)]
    verify_unix_file_ownership_and_mode(&meta, &abs)?;

    let content = fs::read_to_string(&abs)
        .map_err(|e| format!("Failed to read {}: {e}", abs.display()))?;
    let entries = parse_custom_translations_text(&content)?;
    let encoded = serde_json::to_vec(&entries)
        .map_err(|e| format!("Failed to serialize custom translations: {e}"))?;

    Ok(LoadedCustomDict {
        path: Some(abs),
        entries,
        sha256: sha256_hex(&encoded),
    })
}

#[cfg(unix)]
fn verify_unix_file_ownership_and_mode(meta: &fs::Metadata, path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let uid = meta.uid();
    let euid = unsafe { libc::geteuid() };
    if uid != euid && uid != 0 {
        return Err(format!(
            "Refusing custom translations file not owned by current user or root: {}",
            path.display()
        ));
    }
    let mode = meta.mode();
    if (mode & 0o022) != 0 {
        return Err(format!(
            "Refusing writable custom translations file (mode {:o}; expected no group/world write bits, run `chmod 600`): {}",
            mode & 0o777,
            path.display()
        ));
    }
    Ok(())
}

pub fn parse_custom_translations_text(content: &str) -> Result<Vec<[String; 2]>, String> {
    let stripped = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut entries = Vec::new();

    for (idx, raw_line) in stripped.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let Some((lhs, rhs)) = trimmed.split_once('=') else {
            return Err(format!(
                "Invalid custom translation syntax on line {line_no}: expected `Original = Translation`"
            ));
        };
        let key = lhs.trim();
        let val = rhs.trim();
        if key.is_empty() {
            return Err(format!(
                "Empty source key in custom translation on line {line_no}"
            ));
        }
        if val.contains('<') || val.contains('>') {
            return Err(format!(
                "Custom translation on line {line_no} contains forbidden `<` or `>` markup character"
            ));
        }
        entries.push([key.to_string(), val.to_string()]);
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::canonical_temp_dir;

    #[test]
    fn parses_overrides_comments_and_forced_english_retention() {
        let raw = "\u{feff}# user overrides\n\nSettings = 设置中心\n// keep English\nAuto =\nRequest Review = \n";
        let entries = parse_custom_translations_text(raw).unwrap();
        assert_eq!(
            entries,
            vec![
                ["Settings".to_string(), "设置中心".to_string()],
                ["Auto".to_string(), "".to_string()],
                ["Request Review".to_string(), "".to_string()],
            ]
        );
    }

    #[test]
    fn rejects_html_markup_symlinks_and_loose_unix_permissions() {
        assert!(parse_custom_translations_text("Settings = <b>设置</b>").is_err());

        let (_tmp, root) = canonical_temp_dir();
        let file = root.join("custom-translations.txt");
        fs::write(&file, "Settings = 设置\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o666)).unwrap();
            let err =
                load_custom_translations_from_path(&file, Platform::Darwin, false).unwrap_err();
            assert!(err.contains("mode"), "unexpected error: {err}");

            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
            let loaded =
                load_custom_translations_from_path(&file, Platform::Darwin, false).unwrap();
            assert_eq!(loaded.entries.len(), 1);

            let link = root.join("link.txt");
            std::os::unix::fs::symlink(&file, &link).unwrap();
            let link_err =
                load_custom_translations_from_path(&link, Platform::Darwin, false).unwrap_err();
            assert!(link_err.contains("symlink"), "unexpected error: {link_err}");
        }

        let win_err =
            load_custom_translations_from_path(&file, Platform::Win32, false).unwrap_err();
        assert!(win_err.contains("--trust-custom-translations"));
    }
}
