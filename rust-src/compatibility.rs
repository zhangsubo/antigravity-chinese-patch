use crate::asar::{sha256_hex, AsarArchive};
use crate::platform::Platform;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const COMPATIBILITY_JSON: &str = include_str!("../tools/compatibility.json");

#[derive(Debug, Clone, Deserialize)]
pub struct CompatibilityManifest {
    pub schema: u32,
    #[allow(dead_code)]
    pub target: String,
    pub bundle_id: String,
    #[serde(default)]
    pub verified_versions: BTreeMap<String, VerifiedVersionRule>,
    pub anchor_rules: AnchorRules,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VerifiedVersionRule {
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
    #[serde(default)]
    pub fallback_on_other_platforms: bool,
    pub preload_sha256: String,
    pub entry: String,
    #[serde(default)]
    pub native_ui_tested: bool,
    #[serde(default)]
    pub main_swarm_hook: bool,
    #[serde(default)]
    pub text_patches: BTreeMap<String, VerifiedTextPatch>,
    #[serde(default)]
    pub optional_anchor_patches: BTreeMap<String, Vec<[String; 2]>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VerifiedTextPatch {
    pub sha256: String,
    pub replacements: Vec<[String; 2]>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnchorRules {
    pub entry: String,
    #[serde(default = "default_main_entry")]
    pub main_entry: String,
    #[serde(default)]
    pub main_swarm_hook: bool,
    #[serde(default)]
    pub optional_text_patches: BTreeMap<String, Vec<[String; 2]>>,
}

fn default_main_entry() -> String {
    "dist/main.js".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityMode {
    Verified,
    AnchorFallback,
}

impl CompatibilityMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CompatibilityMode::Verified => "verified",
            CompatibilityMode::AnchorFallback => "anchor_fallback",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PatchPlan {
    #[allow(dead_code)]
    pub version: String,
    pub mode: CompatibilityMode,
    pub entry: String,
    pub main_entry: String,
    pub native_ui_tested: bool,
    pub main_swarm_hook: bool,
    pub file_replacements: BTreeMap<String, Vec<[String; 2]>>,
    pub applied_anchors: Vec<String>,
    pub skipped_anchors: Vec<String>,
    pub has_native_text_patches: bool,
}

impl CompatibilityManifest {
    pub fn verified_rule(
        &self,
        platform: Platform,
        version: &str,
    ) -> Result<Option<&VerifiedVersionRule>, String> {
        let Some(rule) = self.verified_versions.get(version) else {
            return Ok(None);
        };
        if let Some(platforms) = &rule.platforms {
            if !platforms.iter().any(|name| name == platform.as_str()) {
                if rule.fallback_on_other_platforms {
                    return Ok(None);
                }
                return Err(format!(
                    "Unsupported version on {}: {}; no files changed",
                    platform.display_label(), version
                ));
            }
        }
        Ok(Some(rule))
    }

    pub fn load_embedded() -> Result<Self, String> {
        Self::from_json_str(COMPATIBILITY_JSON)
    }

    pub fn from_json_str(raw: &str) -> Result<Self, String> {
        let manifest: Self =
            serde_json::from_str(raw).map_err(|e| format!("Invalid compatibility.json: {e}"))?;
        if manifest.schema != 2 {
            return Err(format!(
                "Unsupported compatibility.json schema {} (expected 2)",
                manifest.schema
            ));
        }
        Ok(manifest)
    }
}

pub fn validate_desktop_identity(
    manifest: &CompatibilityManifest,
    platform: Platform,
    bundle_id: Option<&str>,
    bundle_version: Option<&str>,
    package_json: &Value,
) -> Result<String, String> {
    if package_json
        .get("engines")
        .and_then(|e| e.get("vscode"))
        .is_some()
    {
        return Err("This is Antigravity IDE / VS Code, not Antigravity Desktop".to_string());
    }

    let pkg_name = package_json.get("name").and_then(Value::as_str);
    let pkg_version = package_json
        .get("version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if platform == Platform::Darwin {
        if bundle_id != Some(manifest.bundle_id.as_str()) {
            return Err("This is not Antigravity Desktop".to_string());
        }
        let version = bundle_version
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Missing CFBundleShortVersionString".to_string())?;
        if pkg_name != Some("antigravity") || pkg_version != Some(version) {
            return Err("Bundle version differs from package version".to_string());
        }
        return Ok(version.to_string());
    }

    let product_name = package_json
        .get("productName")
        .and_then(Value::as_str)
        .unwrap_or("Antigravity");
    if pkg_name != Some("antigravity") || product_name != "Antigravity" {
        return Err("This is not Antigravity Desktop".to_string());
    }

    let version = pkg_version.ok_or_else(|| "Missing package version".to_string())?;
    Ok(version.to_string())
}

pub fn resolve_version_plan(
    manifest: &CompatibilityManifest,
    platform: Platform,
    version: &str,
    archive: &AsarArchive,
    already_patched: bool,
) -> Result<PatchPlan, String> {
    if let Some(rule) = manifest.verified_rule(platform, version)? {
        let mut file_replacements: BTreeMap<String, Vec<[String; 2]>> = BTreeMap::new();
        let mut applied_anchors = Vec::new();
        let mut skipped_anchors = Vec::new();

        let preload_bytes = archive.read(&rule.entry)?;
        if !already_patched {
            if sha256_hex(&preload_bytes) != rule.preload_sha256 {
                return Err("Unknown preload fingerprint; refusing to patch".to_string());
            }
            for (path, spec) in &rule.text_patches {
                let raw_entry = archive.read(path)?;
                if sha256_hex(&raw_entry) != spec.sha256 {
                    return Err(format!("Unknown {path} fingerprint; refusing to patch"));
                }
                let text = String::from_utf8(raw_entry)
                    .map_err(|_| format!("Entry {path} is not valid UTF-8"))?;
                for pair in &spec.replacements {
                    if text.matches(&pair[0]).count() != 1 {
                        return Err(format!("Expected exact match in {path}; refusing to patch"));
                    }
                }
                file_replacements.insert(path.clone(), spec.replacements.clone());
                applied_anchors.push(format!("{path} ({} rules)", spec.replacements.len()));
            }
            collect_optional_anchors(
                archive,
                &rule.optional_anchor_patches,
                &mut file_replacements,
                &mut applied_anchors,
                &mut skipped_anchors,
            );
        } else {
            for path in rule.text_patches.keys() {
                archive.read(path)?;
            }
        }

        let has_native_text_patches = !rule.text_patches.is_empty()
            || !rule.optional_anchor_patches.is_empty()
            || !file_replacements.is_empty();

        return Ok(PatchPlan {
            version: version.to_string(),
            mode: CompatibilityMode::Verified,
            entry: rule.entry.clone(),
            main_entry: "dist/main.js".to_string(),
            native_ui_tested: rule.native_ui_tested,
            main_swarm_hook: rule.main_swarm_hook && archive.has_regular_file("dist/main.js"),
            file_replacements,
            applied_anchors,
            skipped_anchors,
            has_native_text_patches,
        });
    }

    if !is_valid_semver_like(version) {
        return Err(format!("Unsupported version: {version}; no files changed"));
    }

    let anchors = &manifest.anchor_rules;
    if !archive.has_regular_file(&anchors.entry) {
        return Err(format!(
            "Unsupported version: {version} (missing required anchor {}); no files changed",
            anchors.entry
        ));
    }
    archive.read(&anchors.entry)?;

    let mut file_replacements = BTreeMap::new();
    let mut applied_anchors = Vec::new();
    let mut skipped_anchors = Vec::new();

    if !already_patched {
        collect_optional_anchors(
            archive,
            &anchors.optional_text_patches,
            &mut file_replacements,
            &mut applied_anchors,
            &mut skipped_anchors,
        );
    }

    let main_exists = archive.has_regular_file(&anchors.main_entry);
    if anchors.main_swarm_hook && !main_exists {
        skipped_anchors.push(format!("{} (missing for swarm hook)", anchors.main_entry));
    }

    let has_native_text_patches = !file_replacements.is_empty();

    Ok(PatchPlan {
        version: version.to_string(),
        mode: CompatibilityMode::AnchorFallback,
        entry: anchors.entry.clone(),
        main_entry: anchors.main_entry.clone(),
        native_ui_tested: false,
        main_swarm_hook: anchors.main_swarm_hook && main_exists,
        file_replacements,
        applied_anchors,
        skipped_anchors,
        has_native_text_patches,
    })
}

fn collect_optional_anchors(
    archive: &AsarArchive,
    optional_map: &BTreeMap<String, Vec<[String; 2]>>,
    file_replacements: &mut BTreeMap<String, Vec<[String; 2]>>,
    applied_anchors: &mut Vec<String>,
    skipped_anchors: &mut Vec<String>,
) {
    for (path, pairs) in optional_map {
        if !archive.has_regular_file(path) {
            skipped_anchors.push(format!("{path} (file absent)"));
            continue;
        }
        let Ok(raw) = archive.read(path) else {
            skipped_anchors.push(format!("{path} (unreadable)"));
            continue;
        };
        let Ok(text) = String::from_utf8(raw) else {
            skipped_anchors.push(format!("{path} (non-UTF-8)"));
            continue;
        };
        let mut matched = Vec::new();
        let mut unmatched_count = 0usize;
        for pair in pairs {
            if text.matches(&pair[0]).count() == 1 {
                matched.push(pair.clone());
            } else {
                unmatched_count += 1;
            }
        }
        if !matched.is_empty() {
            let entry = file_replacements.entry(path.clone()).or_default();
            entry.extend(matched.clone());
            applied_anchors.push(format!("{path} ({} optional rules)", matched.len()));
        }
        if unmatched_count > 0 {
            skipped_anchors.push(format!("{path} ({unmatched_count} unmatched anchors)"));
        }
    }
}

fn is_valid_semver_like(version: &str) -> bool {
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    !major.is_empty()
        && major.chars().all(|c| c.is_ascii_digit())
        && version.contains('.')
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asar::build_test_archive;
    use serde_json::json;

    #[test]
    fn embedded_manifest_loads_all_verified_versions() {
        let m = CompatibilityManifest::load_embedded().unwrap();
        assert_eq!(m.schema, 2);
        for ver in ["2.22.0", "2.23.0", "2.29.1", "2.31.2", "2.32.1", "2.32.2"] {
            assert!(m.verified_versions.contains_key(ver), "missing {ver}");
        }
        assert_eq!(m.anchor_rules.entry, "dist/preload.js");
    }

    #[test]
    fn rejects_ide_and_supports_anchor_fallback_for_newer_desktop() {
        let m = CompatibilityManifest::load_embedded().unwrap();
        let ide_pkg = json!({
            "name": "antigravity",
            "version": "2.99.0",
            "engines": { "vscode": "^1.95.0" }
        });
        assert!(validate_desktop_identity(
            &m,
            Platform::Darwin,
            Some("com.google.antigravity"),
            Some("2.99.0"),
            &ide_pkg
        )
        .is_err());

        let desktop_pkg = json!({
            "name": "antigravity",
            "productName": "Antigravity",
            "version": "2.99.0"
        });
        let ver = validate_desktop_identity(
            &m,
            Platform::Darwin,
            Some("com.google.antigravity"),
            Some("2.99.0"),
            &desktop_pkg,
        )
        .unwrap();
        assert_eq!(ver, "2.99.0");

        let preload = b"showContextMenu: (items) => electron_1.ipcRenderer.invoke('window:show-context-menu', items),\n";
        let raw = build_test_archive(&[
            ("package.json", br#"{"name":"antigravity","version":"2.99.0"}"#),
            ("dist/preload.js", preload),
            ("dist/main.js", b"title: 'Confirm Quit',\n"),
        ]);
        let archive = AsarArchive::parse(&raw).unwrap();
        let plan = resolve_version_plan(&m, Platform::Darwin, "2.99.0", &archive, false).unwrap();
        let win_plan = resolve_version_plan(&m, Platform::Win32, "2.32.2", &archive, false).unwrap();
        assert_eq!(win_plan.mode, CompatibilityMode::AnchorFallback);
        for platform in [Platform::Darwin, Platform::Linux] {
            let err = resolve_version_plan(&m, platform, "2.32.2", &archive, false).unwrap_err();
            assert!(err.contains("Unknown preload fingerprint"));
        }
        assert_eq!(plan.mode, CompatibilityMode::AnchorFallback);
        assert!(plan.main_swarm_hook);
        assert!(plan.file_replacements.contains_key("dist/preload.js"));
        assert!(plan.file_replacements.contains_key("dist/main.js"));
        assert!(!plan.skipped_anchors.is_empty());
    }
}
