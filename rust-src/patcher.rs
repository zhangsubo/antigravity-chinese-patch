use crate::asar::{sha256_hex, AsarArchive};
use crate::compatibility::{
    resolve_version_plan, validate_desktop_identity, CompatibilityManifest, PatchPlan,
};
use crate::custom_dict::LoadedCustomDict;
use crate::cdp::fetch_targets;
use crate::daemon::{daemon_status_summary, CDP_RECOVERY_HINT};
use crate::platform::{
    app_platform, asar_path, assert_app_path, assert_no_escaping_symlinks, ensure_stopped,
    ensure_writable, is_link, leave_app_dir_if_inside, lexically_abs, paths, run_checked,
    same_app_path, Platform, RealSystemOps, SystemOps,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DESKTOP_RUNTIME_JS: &str = include_str!("../dist/desktop-runtime.js");
pub const MAIN_SWARM_HOOK_JS: &str = include_str!("../dist/main-swarm-hook.js");
pub const SWARM_RUNTIME_JS: &str = include_str!("../dist/swarm-runtime.js");
pub const PAYLOAD_SHA256: &str = env!("AGY_ZH_PAYLOAD_SHA256");
pub const TRANSLATION_COUNT_STR: &str = env!("AGY_ZH_TRANSLATION_COUNT");

pub const NEW_MARKER: &str = "/* Antigravity Chinese Patch Runtime";
pub const LEGACY_MARKER: &str = "/* TeacherXuAI Antigravity Chinese UI";
pub const SWARM_HOOK_MARKER: &str = "// --- Antigravity Chinese Patch Swarm Hook Start ---";

static STAGING_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn translation_count() -> usize {
    TRANSLATION_COUNT_STR.parse::<usize>().unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMetadata {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub app: String,
    pub version: String,
    pub original_asar: String,
    pub patched_asar: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_plist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_executable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub tool_version: String,
    pub payload_sha256: String,
    pub app: String,
    pub platform: String,
    pub version: String,
    pub compatibility_mode: String,
    pub patched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_generation: Option<String>,
    pub asar_sha256: String,
    pub translations: usize,
    pub backup: String,
    pub state_present: bool,
    pub native_ui_tested: bool,
    pub main_swarm_hook: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub applied_anchors: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped_anchors: Vec<String>,
    pub scope: String,
    pub signature_effect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_backup_version: Option<String>,
    pub cdp_daemon_running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdp_endpoint_reachable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdp_recovery_hint: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdp_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    pub restored: bool,
    pub app: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovered: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppInspection {
    pub app: PathBuf,
    pub platform: Platform,
    pub version: String,
    pub archive: AsarArchive,
    pub plist_val: Option<plist::Value>,
    pub asar_sha256: String,
    pub patched: bool,
    pub patch_generation: Option<String>,
    pub has_main_swarm_hook: bool,
    pub plan: PatchPlan,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InstallOptions {
    pub acknowledge_signature_change: bool,
    pub allow_local_library_loading: bool,
    pub replace_stale_backup: bool,
}

struct StagingGuard {
    dir: PathBuf,
    keep_on_drop: bool,
}

impl StagingGuard {
    fn create(parent: &Path, prefix: &str) -> Result<Self, String> {
        for _ in 0..64 {
            let seq = STAGING_SEQ.fetch_add(1, Ordering::Relaxed);
            let candidate = parent.join(format!("{prefix}{}-{seq}", std::process::id()));
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    return Ok(Self {
                        dir: candidate,
                        keep_on_drop: false,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(format!(
                        "Failed to create staging directory in {}: {e}",
                        parent.display()
                    ));
                }
            }
        }
        Err("Failed to allocate unique staging directory".to_string())
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if !self.keep_on_drop {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

pub fn verify_signature(app: &Path, ops: &dyn SystemOps) -> Result<(), String> {
    let app_str = app.to_string_lossy();
    run_checked(
        ops,
        "/usr/bin/codesign",
        &["--verify", "--deep", "--strict", app_str.as_ref()],
    )?;
    Ok(())
}

pub fn detect_patch_generation(preload_bytes: &[u8]) -> (bool, Option<String>) {
    let text = String::from_utf8_lossy(preload_bytes);
    if text.contains(NEW_MARKER) {
        (true, Some("v3".to_string()))
    } else if text.contains(LEGACY_MARKER) {
        (true, Some("legacy-v2".to_string()))
    } else {
        (false, None)
    }
}

pub fn inspect_app(app: &Path) -> Result<AppInspection, String> {
    let manifest = CompatibilityManifest::load_embedded()?;
    inspect_app_with_manifest(app, &manifest)
}

pub fn inspect_app_with_manifest(
    app: &Path,
    manifest: &CompatibilityManifest,
) -> Result<AppInspection, String> {
    let app = assert_app_path(app)?;
    let platform = app_platform(&app);
    let raw_asar = fs::read(asar_path(&app, platform))
        .map_err(|e| format!("Failed to read ASAR archive: {e}"))?;
    let archive = AsarArchive::parse(&raw_asar)?;
    let pkg_bytes = archive.read("package.json")?;
    let package_json: Value = serde_json::from_slice(&pkg_bytes)
        .map_err(|e| format!("Invalid package.json inside app.asar: {e}"))?;

    let (version, plist_val) = if platform == Platform::Darwin {
        let plist_bytes = fs::read(app.join("Contents/Info.plist"))
            .map_err(|e| format!("Failed to read Contents/Info.plist: {e}"))?;
        let pval = plist::from_bytes::<plist::Value>(&plist_bytes)
            .map_err(|e| format!("Invalid Info.plist: {e}"))?;
        let dict = pval
            .as_dictionary()
            .ok_or_else(|| "Info.plist root must be a dictionary".to_string())?;
        let bundle_id = dict.get("CFBundleIdentifier").and_then(|v| v.as_string());
        let bundle_ver = dict
            .get("CFBundleShortVersionString")
            .and_then(|v| v.as_string());
        let ver =
            validate_desktop_identity(manifest, platform, bundle_id, bundle_ver, &package_json)?;

        let integrity = dict
            .get("ElectronAsarIntegrity")
            .and_then(|v| v.as_dictionary())
            .and_then(|d| d.get("Resources/app.asar"))
            .and_then(|v| v.as_dictionary())
            .ok_or_else(|| "Bundle ASAR integrity does not match".to_string())?;
        let algo = integrity.get("algorithm").and_then(|v| v.as_string());
        let hash = integrity.get("hash").and_then(|v| v.as_string());
        if algo != Some("SHA256") || hash != Some(archive.raw_header_sha256().as_str()) {
            return Err("Bundle ASAR integrity does not match".to_string());
        }
        (ver, Some(pval))
    } else {
        let ver = validate_desktop_identity(manifest, platform, None, None, &package_json)?;
        (ver, None)
    };

    let entry_path = manifest
        .verified_rule(platform, &version)?
        .map(|r| r.entry.as_str())
        .unwrap_or(manifest.anchor_rules.entry.as_str());
    let preload_bytes = archive.read(entry_path)?;
    let (patched, patch_generation) = detect_patch_generation(&preload_bytes);

    let plan = resolve_version_plan(manifest, platform, &version, &archive, patched)?;
    let has_main_swarm_hook = if archive.has_regular_file(&plan.main_entry) {
        archive
            .read(&plan.main_entry)
            .ok()
            .is_some_and(|b| String::from_utf8_lossy(&b).contains(SWARM_HOOK_MARKER))
    } else {
        false
    };

    Ok(AppInspection {
        app,
        platform,
        version,
        archive,
        plist_val,
        asar_sha256: sha256_hex(&raw_asar),
        patched,
        patch_generation,
        has_main_swarm_hook,
        plan,
    })
}

pub fn build_doctor_report(info: &AppInspection) -> DoctorReport {
    let (backup, state) = paths(&info.app);
    let mut scope = if info.has_main_swarm_hook || info.plan.main_swarm_hook {
        "Settings, navigation, Swarm board, Automations, approval & panel toolbars".to_string()
    } else {
        "Settings, navigation, Automations, approval & panel toolbars".to_string()
    };
    if info.plan.has_native_text_patches {
        scope.push_str("; native quit dialog & tray");
    }

    let signature_effect = match info.platform {
        Platform::Darwin => {
            "local ad-hoc outer signature and explicit app-scoped library-validation exception"
        }
        Platform::Win32 => {
            "ASAR internal block integrity updated; no outer executable modification required on Windows"
        }
        Platform::Linux => {
            "ASAR internal block integrity updated; no outer binary signature modification required on Linux"
        }
    };

    let stale_backup_version = if !info.patched {
        stale_backup_version(&info.app, &info.version, &backup, &state)
    } else {
        None
    };

    let daemon_summary = daemon_status_summary();
    let cdp_endpoint_reachable = daemon_summary.port
        .filter(|_| daemon_summary.app.as_deref().is_some_and(|path| {
            same_app_path(path, &info.app, info.platform)
        }))
        .map(|port| fetch_targets(port).is_ok());

    DoctorReport {
        tool_version: TOOL_VERSION.to_string(),
        payload_sha256: PAYLOAD_SHA256.to_string(),
        app: info.app.to_string_lossy().into_owned(),
        platform: info.platform.as_str().to_string(),
        version: info.version.clone(),
        compatibility_mode: info.plan.mode.as_str().to_string(),
        patched: info.patched,
        patch_generation: info.patch_generation.clone(),
        asar_sha256: info.asar_sha256.clone(),
        translations: translation_count(),
        backup: backup.to_string_lossy().into_owned(),
        state_present: state.exists(),
        native_ui_tested: info.plan.native_ui_tested,
        main_swarm_hook: if info.patched {
            info.has_main_swarm_hook
        } else {
            info.plan.main_swarm_hook
        },
        applied_anchors: info.plan.applied_anchors.clone(),
        skipped_anchors: info.plan.skipped_anchors.clone(),
        scope,
        signature_effect: signature_effect.to_string(),
        stale_backup_version,
        cdp_daemon_running: daemon_summary.running,
        cdp_port: daemon_summary.port,
        cdp_endpoint_reachable,
        cdp_recovery_hint: (cdp_endpoint_reachable == Some(false)).then_some(CDP_RECOVERY_HINT),
    }
}

pub fn doctor_app(app: &Path) -> Result<DoctorReport, String> {
    let info = inspect_app(app)?;
    Ok(build_doctor_report(&info))
}

pub fn stale_backup_version(
    app: &Path,
    current_version: &str,
    backup: &Path,
    state: &Path,
) -> Option<String> {
    if is_link(backup) || is_link(state) || !backup.is_dir() || !state.is_file() {
        return None;
    }
    if !crate::platform::resolved_without_symlinks(backup).unwrap_or(false) {
        return None;
    }

    let platform = app_platform(app);
    let exe_rel = platform.executable_rel();
    let required = if platform == Platform::Darwin {
        ["Contents/Info.plist", "Contents/Resources/app.asar"]
    } else {
        [exe_rel, "resources/app.asar"]
    };
    for relative in required {
        let f = backup.join(relative);
        if !f.is_file()
            || is_link(&f)
            || !crate::platform::resolved_without_symlinks(&f).unwrap_or(false)
        {
            return None;
        }
    }

    let text = fs::read_to_string(state).ok()?;
    let meta: StateMetadata = serde_json::from_str(&text).ok()?;
    if (meta.schema != 1 && meta.schema != 2)
        || !same_app_path(&meta.app, app, platform)
        || meta.version.is_empty()
        || meta.version == current_version
    {
        return None;
    }

    if platform == Platform::Darwin {
        let asar_bytes = fs::read(backup.join("Contents/Resources/app.asar")).ok()?;
        let plist_bytes = fs::read(backup.join("Contents/Info.plist")).ok()?;
        if sha256_hex(&asar_bytes) != meta.original_asar
            || Some(sha256_hex(&plist_bytes)) != meta.original_plist
        {
            return None;
        }
    } else {
        let asar_bytes = fs::read(backup.join("resources/app.asar")).ok()?;
        let exe_bytes = fs::read(backup.join(exe_rel)).ok()?;
        if sha256_hex(&asar_bytes) != meta.original_asar
            || Some(sha256_hex(&exe_bytes)) != meta.original_executable
        {
            return None;
        }
    }

    Some(meta.version)
}

pub fn patch_bundle(
    app: &Path,
    manifest: &CompatibilityManifest,
    custom_dict: &LoadedCustomDict,
) -> Result<String, String> {
    let info = inspect_app_with_manifest(app, manifest)?;
    if info.patched {
        return Err("Already patched".to_string());
    }

    let mut updates: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (rel_path, replacements) in &info.plan.file_replacements {
        let raw = info.archive.read(rel_path)?;
        let mut text = String::from_utf8(raw)
            .map_err(|_| format!("Expected UTF-8 text in {rel_path}"))?;
        for pair in replacements {
            if text.matches(&pair[0]).count() != 1 {
                return Err(format!("Expected exact match in {rel_path}"));
            }
            text = text.replacen(&pair[0], &pair[1], 1);
        }
        updates.insert(rel_path.clone(), text.into_bytes());
    }

    // Build updated preload.js with mode flag, optional custom dictionary, and desktop runtime.
    let mut preload_bytes = match updates.get(&info.plan.entry) {
        Some(existing) => existing.clone(),
        None => info.archive.read(&info.plan.entry)?,
    };
    preload_bytes.extend_from_slice(b"\nglobalThis.__AGY_ZH_MODE__ = \"static\";\n");
    if !custom_dict.entries.is_empty() {
        let custom_json = serde_json::to_string(&custom_dict.entries)
            .map_err(|e| format!("Failed to encode custom translations: {e}"))?;
        preload_bytes.extend_from_slice(
            format!("globalThis.__agyZhCustomTranslations = {custom_json};\n").as_bytes(),
        );
    }
    preload_bytes.extend_from_slice(DESKTOP_RUNTIME_JS.as_bytes());
    preload_bytes.push(b'\n');
    updates.insert(info.plan.entry.clone(), preload_bytes);

    // Append Swarm main-frame hook if supported and absent.
    if info.plan.main_swarm_hook {
        let mut main_bytes = match updates.get(&info.plan.main_entry) {
            Some(existing) => existing.clone(),
            None => info.archive.read(&info.plan.main_entry)?,
        };
        if !String::from_utf8_lossy(&main_bytes).contains(SWARM_HOOK_MARKER) {
            main_bytes.push(b'\n');
            main_bytes.extend_from_slice(MAIN_SWARM_HOOK_JS.as_bytes());
            main_bytes.push(b'\n');
            updates.insert(info.plan.main_entry.clone(), main_bytes);
        }
    }

    let ordered_updates: Vec<(String, Vec<u8>)> = updates.into_iter().collect();
    let rebuilt_bytes = info.archive.replace_many(&ordered_updates)?;
    let rebuilt = AsarArchive::parse(&rebuilt_bytes)?;
    for (rel_path, payload) in &ordered_updates {
        if rebuilt.read(rel_path)? != *payload {
            return Err("Rebuilt archive verification failed".to_string());
        }
    }

    if info.platform == Platform::Darwin {
        let mut plist_val = info
            .plist_val
            .clone()
            .ok_or_else(|| "Missing parsed Info.plist".to_string())?;
        update_plist_asar_integrity(&mut plist_val, &rebuilt.raw_header_sha256())?;
        fs::write(app.join("Contents/Resources/app.asar"), &rebuilt_bytes)
            .map_err(|e| format!("Failed to write patched app.asar: {e}"))?;
        let mut xml_buf = Vec::new();
        plist::to_writer_xml(&mut xml_buf, &plist_val)
            .map_err(|e| format!("Failed to serialize Info.plist: {e}"))?;
        fs::write(app.join("Contents/Info.plist"), &xml_buf)
            .map_err(|e| format!("Failed to write Info.plist: {e}"))?;
    } else {
        fs::write(app.join("resources/app.asar"), &rebuilt_bytes)
            .map_err(|e| format!("Failed to write patched app.asar: {e}"))?;
    }

    Ok(sha256_hex(&rebuilt_bytes))
}

fn update_plist_asar_integrity(plist_val: &mut plist::Value, new_hash: &str) -> Result<(), String> {
    let root = plist_val
        .as_dictionary_mut()
        .ok_or_else(|| "Info.plist root must be a dictionary".to_string())?;
    let integrity = root
        .get_mut("ElectronAsarIntegrity")
        .and_then(|v| v.as_dictionary_mut())
        .ok_or_else(|| "Missing ElectronAsarIntegrity in Info.plist".to_string())?;
    let asar_entry = integrity
        .get_mut("Resources/app.asar")
        .and_then(|v| v.as_dictionary_mut())
        .ok_or_else(|| "Missing Resources/app.asar in ElectronAsarIntegrity".to_string())?;
    asar_entry.insert("hash".to_string(), plist::Value::String(new_hash.to_string()));
    Ok(())
}

pub fn install_app(
    app: &Path,
    opts: InstallOptions,
    custom_dict: &LoadedCustomDict,
) -> Result<DoctorReport, String> {
    let manifest = CompatibilityManifest::load_embedded()?;
    let ops = RealSystemOps;
    install_app_with(app, opts, custom_dict, &manifest, &ops)
}

pub fn install_app_with(
    app: &Path,
    opts: InstallOptions,
    custom_dict: &LoadedCustomDict,
    manifest: &CompatibilityManifest,
    ops: &dyn SystemOps,
) -> Result<DoctorReport, String> {
    let abs_app = lexically_abs(app)?;
    let platform = app_platform(&abs_app);
    let host = Platform::host();

    match platform {
        Platform::Darwin => {
            if !cfg!(test) && host != Platform::Darwin {
                return Err("macOS bundle installation requires running on macOS".to_string());
            }
            if !opts.acknowledge_signature_change {
                return Err(
                    "Read README first. Installation requires --acknowledge-signature-change"
                        .to_string(),
                );
            }
            if !opts.allow_local_library_loading {
                return Err(
                    "Local signing requires an app-scoped library-validation exception; read README and use --allow-local-library-loading"
                        .to_string(),
                );
            }
        }
        Platform::Linux => {
            if !cfg!(test) && host != Platform::Linux {
                return Err("Linux installation requires running on Linux".to_string());
            }
        }
        Platform::Win32 => {
            if !cfg!(test) && host != Platform::Win32 {
                return Err("Windows installation requires running on Windows".to_string());
            }
        }
    }

    let info = inspect_app_with_manifest(&abs_app, manifest)?;
    let app = info.app.clone();
    if info.patched {
        return Err("Already patched; restore before installing a different patch version".to_string());
    }

    let (backup, state) = paths(&app);
    let has_existing =
        backup.exists() || state.exists() || is_link(&backup) || is_link(&state);
    let stale_ver = if has_existing {
        stale_backup_version(&app, &info.version, &backup, &state)
    } else {
        None
    };

    if has_existing {
        let Some(old_ver) = stale_ver.as_deref() else {
            return Err("Backup/state already exists; refusing to overwrite it".to_string());
        };
        if !opts.replace_stale_backup {
            return Err(format!(
                "Backup/state already exists from {old_ver}; pass --replace-stale-backup after an app update"
            ));
        }
    }

    let app_parent = app
        .parent()
        .ok_or_else(|| "Invalid application parent directory".to_string())?;
    let asar_file = asar_path(&app, platform);
    let mut writable_targets: Vec<&Path> = vec![app_parent, &app, &asar_file];
    if stale_ver.is_some() {
        writable_targets.push(&backup);
        writable_targets.push(&state);
    }
    ensure_writable(&writable_targets, platform, ops)?;
    ensure_stopped(&app, platform, ops)?;

    if platform == Platform::Darwin {
        if stale_ver.is_some() {
            verify_signature(&backup, ops)?;
        }
        verify_signature(&app, ops)?;
        let app_str = app.to_string_lossy();
        run_checked(
            ops,
            "/usr/sbin/spctl",
            &["--assess", "--type", "execute", app_str.as_ref()],
        )?;
        let sig_out = run_checked(
            ops,
            "/usr/bin/codesign",
            &["-dv", "--verbose=2", app_str.as_ref()],
        )?;
        let combined_sig = format!("{}\n{}", sig_out.stdout, sig_out.stderr);
        if !combined_sig.contains("Authority=Developer ID Application: Google LLC (EQHXZ8M8AV)") {
            return Err("Original application must have a Google LLC signature".to_string());
        }
    } else if stale_ver.is_some() {
        assert_no_escaping_symlinks(&backup)?;
    }

    let staging = StagingGuard::create(app_parent, ".antigravity-zh-")?;
    let app_name = app
        .file_name()
        .ok_or_else(|| "Invalid application path".to_string())?;
    let staged = staging.dir.join(app_name);
    let old_backup = staging.dir.join("stale-backup.app");
    let old_state = staging.dir.join("stale-state.json");

    ops.clone_dir(&app, &staged)?;
    let patched_hash = patch_bundle(&staged, manifest, custom_dict)?;

    if platform == Platform::Darwin {
        let app_str = app.to_string_lossy();
        let ent_out = run_checked(
            ops,
            "/usr/bin/codesign",
            &["-d", "--entitlements", ":-", app_str.as_ref()],
        )?;
        let entitlements_plist = build_local_entitlements_plist(&ent_out.stdout)?;
        let ent_path = staging.dir.join("local-entitlements.plist");
        fs::write(&ent_path, &entitlements_plist)
            .map_err(|e| format!("Failed to write temporary entitlements: {e}"))?;

        let ent_str = ent_path.to_string_lossy();
        let staged_str = staged.to_string_lossy();
        run_checked(
            ops,
            "/usr/bin/codesign",
            &[
                "--force",
                "--sign",
                "-",
                "--preserve-metadata=flags,runtime",
                "--entitlements",
                ent_str.as_ref(),
                staged_str.as_ref(),
            ],
        )?;
        verify_signature(&staged, ops)?;
    }

    ensure_stopped(&app, platform, ops)?;
    let rechecked = inspect_app_with_manifest(&app, manifest)?;
    if rechecked.asar_sha256 != info.asar_sha256 {
        return Err("Original app changed during preparation; retry".to_string());
    }

    let metadata = if platform == Platform::Darwin {
        let plist_bytes = fs::read(app.join("Contents/Info.plist"))
            .map_err(|e| format!("Failed to read original Info.plist: {e}"))?;
        StateMetadata {
            schema: 2,
            platform: Some("darwin".to_string()),
            app: app.to_string_lossy().into_owned(),
            version: info.version.clone(),
            original_asar: info.asar_sha256.clone(),
            patched_asar: patched_hash,
            original_plist: Some(sha256_hex(&plist_bytes)),
            original_executable: None,
            compatibility_mode: Some(info.plan.mode.as_str().to_string()),
            tool_version: Some(TOOL_VERSION.to_string()),
        }
    } else {
        let exe_bytes = fs::read(app.join(platform.executable_rel()))
            .map_err(|e| format!("Failed to read original executable: {e}"))?;
        StateMetadata {
            schema: 2,
            platform: Some(platform.as_str().to_string()),
            app: app.to_string_lossy().into_owned(),
            version: info.version.clone(),
            original_asar: info.asar_sha256.clone(),
            patched_asar: patched_hash,
            original_plist: None,
            original_executable: Some(sha256_hex(&exe_bytes)),
            compatibility_mode: Some(info.plan.mode.as_str().to_string()),
            tool_version: Some(TOOL_VERSION.to_string()),
        }
    };

    let mut moved_stale_backup = false;
    let mut moved_stale_state = false;
    let mut state_created = false;

    let commit_result = (|| -> Result<(), String> {
        if stale_ver.is_some() {
            fs::rename(&backup, &old_backup)
                .map_err(|e| format!("Failed to move stale backup: {e}"))?;
            moved_stale_backup = true;
            fs::rename(&state, &old_state)
                .map_err(|e| format!("Failed to move stale state: {e}"))?;
            moved_stale_state = true;
        }

        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&state)
            .map_err(|e| format!("Failed to create recovery state file: {e}"))?;
        state_created = true;
        let json_bytes = serde_json::to_vec_pretty(&metadata)
            .map_err(|e| format!("Failed to encode state JSON: {e}"))?;
        f.write_all(&json_bytes)
            .and_then(|()| f.write_all(b"\n"))
            .and_then(|()| f.sync_all())
            .map_err(|e| format!("Failed to write recovery state file: {e}"))?;
        drop(f);

        leave_app_dir_if_inside(&app);
        fs::rename(&app, &backup)
            .map_err(|e| format!("Failed to move original app to backup: {e}"))?;
        if let Err(e) = fs::rename(&staged, &app) {
            let _ = fs::rename(&backup, &app);
            return Err(format!("Failed to move patched app into place: {e}"));
        }
        Ok(())
    })();

    if let Err(err) = commit_result {
        if state_created && state.is_file() {
            let _ = fs::remove_file(&state);
        }
        if moved_stale_state && !state.exists() {
            let _ = fs::rename(&old_state, &state);
        }
        if moved_stale_backup && !backup.exists() {
            let _ = fs::rename(&old_backup, &backup);
        }
        return Err(err);
    }

    drop(staging);
    let final_info = inspect_app_with_manifest(&app, manifest)?;
    Ok(build_doctor_report(&final_info))
}

fn build_local_entitlements_plist(codesign_stdout: &str) -> Result<Vec<u8>, String> {
    let trimmed = codesign_stdout.trim();
    let mut val = if trimmed.is_empty() {
        plist::Value::Dictionary(plist::Dictionary::new())
    } else {
        let xml_slice = if let Some(idx) = trimmed.find("<?xml") {
            &trimmed[idx..]
        } else if let Some(idx) = trimmed.find("<plist") {
            &trimmed[idx..]
        } else {
            trimmed
        };
        plist::from_bytes::<plist::Value>(xml_slice.as_bytes())
            .map_err(|e| format!("Failed to parse existing entitlements plist: {e}"))?
    };
    let dict = val
        .as_dictionary_mut()
        .ok_or_else(|| "Entitlements plist must be a dictionary".to_string())?;
    dict.insert(
        "com.apple.security.cs.disable-library-validation".to_string(),
        plist::Value::Boolean(true),
    );
    let mut out = Vec::new();
    plist::to_writer_xml(&mut out, &val)
        .map_err(|e| format!("Failed to encode local entitlements plist: {e}"))?;
    Ok(out)
}

pub fn restore_app(app: &Path) -> Result<RestoreReport, String> {
    let ops = RealSystemOps;
    restore_app_with(app, &ops)
}

pub fn restore_app_with(app: &Path, ops: &dyn SystemOps) -> Result<RestoreReport, String> {
    let app = lexically_abs(app)?;
    let platform = app_platform(&app);
    let host = Platform::host();
    match platform {
        Platform::Darwin if !cfg!(test) && host != Platform::Darwin => {
            return Err("macOS restoration requires running on macOS".to_string());
        }
        Platform::Linux if !cfg!(test) && host != Platform::Linux => {
            return Err("Linux restoration requires running on Linux".to_string());
        }
        Platform::Win32 if !cfg!(test) && host != Platform::Win32 => {
            return Err("Windows restoration requires running on Windows".to_string());
        }
        _ => {}
    }

    let (backup, state) = paths(&app);
    if is_link(&state)
        || is_link(&backup)
        || (backup.exists()
            && !crate::platform::resolved_without_symlinks(&backup).unwrap_or(false))
    {
        return Err("Invalid recovery path".to_string());
    }
    if !state.is_file() {
        return Err(format!("No recovery state file found for {}", app.display()));
    }

    let state_raw = fs::read_to_string(&state)
        .map_err(|e| format!("Failed to read recovery state file: {e}"))?;
    let metadata: StateMetadata = serde_json::from_str(&state_raw)
        .map_err(|e| format!("Invalid recovery state JSON: {e}"))?;
    if (metadata.schema != 1 && metadata.schema != 2)
        || !same_app_path(&metadata.app, &app, platform)
    {
        return Err("Recovery record does not match the application".to_string());
    }

    let app_parent = app
        .parent()
        .ok_or_else(|| "Invalid application parent directory".to_string())?;
    let mut targets: Vec<&Path> = vec![app_parent, &state];
    if backup.exists() {
        targets.push(&backup);
    }
    if app.exists() {
        targets.push(&app);
    }
    ensure_writable(&targets, platform, ops)?;
    ensure_stopped(&app, platform, ops)?;

    let exe_rel = platform.executable_rel();
    if !backup.exists() && app.exists() {
        assert_app_path(&app)?;
        if platform == Platform::Darwin {
            let cur_asar = fs::read(app.join("Contents/Resources/app.asar"))
                .map_err(|e| format!("Failed to read app.asar: {e}"))?;
            let cur_plist = fs::read(app.join("Contents/Info.plist"))
                .map_err(|e| format!("Failed to read Info.plist: {e}"))?;
            if sha256_hex(&cur_asar) == metadata.original_asar
                && Some(sha256_hex(&cur_plist)) == metadata.original_plist
            {
                verify_signature(&app, ops)?;
                fs::remove_file(&state)
                    .map_err(|e| format!("Failed to remove stale state file: {e}"))?;
                return Ok(RestoreReport {
                    restored: true,
                    app: app.to_string_lossy().into_owned(),
                    version: metadata.version,
                    recovered: Some("interrupted before original app was moved".to_string()),
                });
            }
        } else {
            let cur_asar = fs::read(app.join("resources/app.asar"))
                .map_err(|e| format!("Failed to read app.asar: {e}"))?;
            let cur_exe = fs::read(app.join(exe_rel))
                .map_err(|e| format!("Failed to read executable: {e}"))?;
            if sha256_hex(&cur_asar) == metadata.original_asar
                && Some(sha256_hex(&cur_exe)) == metadata.original_executable
            {
                fs::remove_file(&state)
                    .map_err(|e| format!("Failed to remove stale state file: {e}"))?;
                return Ok(RestoreReport {
                    restored: true,
                    app: app.to_string_lossy().into_owned(),
                    version: metadata.version,
                    recovered: Some("interrupted before original app was moved".to_string()),
                });
            }
        }
    }

    if platform == Platform::Darwin {
        if !backup.is_dir() {
            return Err("Backup directory is missing".to_string());
        }
        let bak_asar = fs::read(backup.join("Contents/Resources/app.asar"))
            .map_err(|e| format!("Failed to read backup app.asar: {e}"))?;
        if sha256_hex(&bak_asar) != metadata.original_asar {
            return Err("Backup archive checksum mismatch".to_string());
        }
        let bak_plist = fs::read(backup.join("Contents/Info.plist"))
            .map_err(|e| format!("Failed to read backup Info.plist: {e}"))?;
        if Some(sha256_hex(&bak_plist)) != metadata.original_plist {
            return Err("Backup Info.plist checksum mismatch".to_string());
        }
        verify_signature(&backup, ops)?;
    } else {
        if !backup.is_dir() {
            return Err("Backup directory is missing".to_string());
        }
        for relative in [exe_rel, "resources/app.asar"] {
            let bf = backup.join(relative);
            if !bf.is_file()
                || is_link(&bf)
                || !crate::platform::resolved_without_symlinks(&bf).unwrap_or(false)
            {
                return Err(format!("Expected regular backup files: {relative}"));
            }
        }
        assert_no_escaping_symlinks(&backup)?;
        let bak_asar = fs::read(backup.join("resources/app.asar"))
            .map_err(|e| format!("Failed to read backup app.asar: {e}"))?;
        if sha256_hex(&bak_asar) != metadata.original_asar {
            return Err("Backup archive checksum mismatch".to_string());
        }
        let bak_exe = fs::read(backup.join(exe_rel))
            .map_err(|e| format!("Failed to read backup executable: {e}"))?;
        if Some(sha256_hex(&bak_exe)) != metadata.original_executable {
            return Err("Backup executable checksum mismatch".to_string());
        }
    }

    if app.exists() {
        assert_app_path(&app)?;
        let current_asar = fs::read(asar_path(&app, platform))
            .map_err(|e| format!("Failed to read installed app.asar: {e}"))?;
        if sha256_hex(&current_asar) != metadata.patched_asar {
            return Err(
                "Installed app changed (possibly updated). Restore would overwrite it; stopped"
                    .to_string(),
            );
        }
    }

    let mut staging = StagingGuard::create(app_parent, ".antigravity-zh-restore-")?;
    staging.keep_on_drop = true;
    let app_name = app
        .file_name()
        .ok_or_else(|| "Invalid application path".to_string())?;
    let current = staging.dir.join(app_name);

    if app.exists() {
        leave_app_dir_if_inside(&app);
        fs::rename(&app, &current)
            .map_err(|e| format!("Failed to stage patched app for restore: {e}"))?;
    }
    if let Err(e) = fs::rename(&backup, &app) {
        if current.exists() && fs::rename(&current, &app).is_ok() {
            staging.keep_on_drop = false;
        }
        return Err(format!("Failed to restore backup app: {e}"));
    }

    staging.keep_on_drop = false;
    fs::remove_file(&state).map_err(|e| format!("Restored app, but failed to remove state file: {e}"))?;

    Ok(RestoreReport {
        restored: true,
        app: app.to_string_lossy().into_owned(),
        version: metadata.version,
        recovered: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asar::build_test_archive;
    use crate::compatibility::CompatibilityMode;
    use crate::platform::{canonical_temp_dir, copy_dir_recursive, CmdOutput};
    use serde_json::json;

    #[derive(Default)]
    struct MockInstallerOps {
        ps_stdout: String,
        fail_codesign_sign: bool,
        deny_write: bool,
    }

    impl SystemOps for MockInstallerOps {
        fn run_cmd(&self, program: &str, args: &[&str]) -> Result<CmdOutput, String> {
            if program == "/usr/bin/codesign" && args.contains(&"--force") && self.fail_codesign_sign
            {
                return Ok(CmdOutput {
                    status_code: 1,
                    stdout: String::new(),
                    stderr: "sign failure".to_string(),
                });
            }
            if program == "/usr/bin/codesign" && args.contains(&"--entitlements") && args.contains(&":-") {
                return Ok(CmdOutput {
                    status_code: 0,
                    stdout: r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>com.apple.security.cs.allow-jit</key><true/></dict></plist>"#.to_string(),
                    stderr: String::new(),
                });
            }
            if program == "/usr/bin/codesign" && args.contains(&"-dv") {
                return Ok(CmdOutput {
                    status_code: 0,
                    stdout: String::new(),
                    stderr: "Authority=Developer ID Application: Google LLC (EQHXZ8M8AV)\n"
                        .to_string(),
                });
            }
            Ok(CmdOutput {
                status_code: 0,
                stdout: self.ps_stdout.clone(),
                stderr: String::new(),
            })
        }

        fn clone_dir(&self, src: &Path, dst: &Path) -> Result<(), String> {
            copy_dir_recursive(src, dst)
        }

        fn is_writable(&self, _path: &Path, _is_dir: bool) -> bool {
            !self.deny_write
        }
    }

    fn fake_preload_bytes() -> &'static [u8] {
        b"/* test preload */\nshowContextMenu: (items) => electron_1.ipcRenderer.invoke('window:show-context-menu', items),\n"
    }

    fn fake_main_bytes() -> &'static [u8] {
        b"label: 'New Window',\nlabel: 'No agents running',\nlabel: `Open ${electron_1.app.getName()}`,\nlabel: 'Quit',\nbuttons: ['Cancel', 'Quit'],\ntitle: 'Confirm Quit',\nmessage: 'Are you sure you want to quit?',\ndetail: 'There may be agents or background tasks running.',\nshowErrorBox('Binary not found', '');\nshowErrorBox('Startup failed', '');\n"
    }

    fn fake_tray_bytes() -> &'static [u8] {
        b"(count > 0 ? `${count}` : 'No') +\n                    ' agent' +\n                    (count === 1 ? '' : 's') +\n                    ' running'"
    }

    fn test_manifest() -> CompatibilityManifest {
        let mut m = CompatibilityManifest::load_embedded().unwrap();
        let preload_hash = sha256_hex(fake_preload_bytes());
        let main_hash = sha256_hex(fake_main_bytes());
        let tray_hash = sha256_hex(fake_tray_bytes());
        for ver in [
            "2.22.0", "2.23.0", "2.29.1", "2.31.2", "2.32.1", "2.32.2", "2.33.0", "2.33.1",
            "2.34.0", "2.35.0", "2.35.2",
        ] {
            if let Some(rule) = m.verified_versions.get_mut(ver) {
                rule.preload_sha256 = preload_hash.clone();
                if let Some(tp) = rule.text_patches.get_mut("dist/preload.js") {
                    tp.sha256 = preload_hash.clone();
                }
                if let Some(tp) = rule.text_patches.get_mut("dist/main.js") {
                    tp.sha256 = main_hash.clone();
                }
                if let Some(tp) = rule.text_patches.get_mut("dist/tray.js") {
                    tp.sha256 = tray_hash.clone();
                }
            }
        }
        m
    }

    fn write_darwin_plist(app: &Path, version: &str, asar_bytes: &[u8]) {
        let arch = AsarArchive::parse(asar_bytes).unwrap();
        let mut dict = plist::Dictionary::new();
        dict.insert(
            "CFBundleIdentifier".to_string(),
            plist::Value::String("com.google.antigravity".to_string()),
        );
        dict.insert(
            "CFBundleShortVersionString".to_string(),
            plist::Value::String(version.to_string()),
        );
        let mut asar_info = plist::Dictionary::new();
        asar_info.insert(
            "algorithm".to_string(),
            plist::Value::String("SHA256".to_string()),
        );
        asar_info.insert(
            "hash".to_string(),
            plist::Value::String(arch.raw_header_sha256()),
        );
        let mut integrity = plist::Dictionary::new();
        integrity.insert(
            "Resources/app.asar".to_string(),
            plist::Value::Dictionary(asar_info),
        );
        dict.insert(
            "ElectronAsarIntegrity".to_string(),
            plist::Value::Dictionary(integrity),
        );
        plist::to_file_xml(app.join("Contents/Info.plist"), &plist::Value::Dictionary(dict))
            .unwrap();
    }

    fn create_darwin_app(root: &Path, version: &str) -> (PathBuf, Vec<u8>) {
        let app = root.join("Antigravity.app");
        fs::create_dir_all(app.join("Contents/Resources")).unwrap();
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(app.join("Contents/MacOS/Antigravity"), b"macho").unwrap();
        let pkg = json!({ "name": "antigravity", "version": version })
            .to_string()
            .into_bytes();
        let asar_bytes = build_test_archive(&[
            ("package.json", &pkg),
            ("dist/preload.js", fake_preload_bytes()),
            ("dist/main.js", fake_main_bytes()),
            ("dist/tray.js", fake_tray_bytes()),
            ("asset", b"unchanged"),
        ]);
        fs::write(app.join("Contents/Resources/app.asar"), &asar_bytes).unwrap();
        write_darwin_plist(&app, version, &asar_bytes);
        (app, asar_bytes)
    }

    #[test]
    fn verified_versions_all_install_and_restore_cleanly() {
        let m = test_manifest();
        let ops = MockInstallerOps::default();
        let dict = LoadedCustomDict::empty();

        for ver in [
            "2.22.0", "2.23.0", "2.29.1", "2.31.2", "2.32.1", "2.32.2", "2.33.0", "2.33.1",
            "2.34.0", "2.35.0", "2.35.2",
        ] {
            let (_tmp, root) = canonical_temp_dir();
            let (app, orig_asar) = create_darwin_app(&root, ver);

            let report = install_app_with(
                &app,
                InstallOptions {
                    acknowledge_signature_change: true,
                    allow_local_library_loading: true,
                    replace_stale_backup: false,
                },
                &dict,
                &m,
                &ops,
            )
            .unwrap();
            assert!(report.patched);
            assert_eq!(report.version, ver);
            assert_eq!(report.patch_generation.as_deref(), Some("v3"));

            let patched_info = inspect_app_with_manifest(&app, &m).unwrap();
            assert_eq!(patched_info.archive.read("asset").unwrap(), b"unchanged");
            if m.verified_versions[ver].main_swarm_hook {
                let main_text = String::from_utf8(patched_info.archive.read("dist/main.js").unwrap()).unwrap();
                assert!(main_text.contains("确认退出"));
                assert!(main_text.contains(SWARM_HOOK_MARKER));
            }

            let restored = restore_app_with(&app, &ops).unwrap();
            assert!(restored.restored);
            assert_eq!(
                fs::read(app.join("Contents/Resources/app.asar")).unwrap(),
                orig_asar
            );
            let (backup, state) = paths(&app);
            assert!(!backup.exists());
            assert!(!state.exists());
        }
    }

    #[test]
    fn signature_failure_leaves_original_intact_and_rejects_bad_flags() {
        let m = test_manifest();
        let (_tmp, root) = canonical_temp_dir();
        let (app, orig_asar) = create_darwin_app(&root, "2.32.1");
        let dict = LoadedCustomDict::empty();

        let ops_ok = MockInstallerOps::default();
        assert!(install_app_with(&app, InstallOptions::default(), &dict, &m, &ops_ok).is_err());

        let ops_fail = MockInstallerOps {
            fail_codesign_sign: true,
            ..Default::default()
        };
        assert!(install_app_with(
            &app,
            InstallOptions {
                acknowledge_signature_change: true,
                allow_local_library_loading: true,
                replace_stale_backup: false,
            },
            &dict,
            &m,
            &ops_fail,
        )
        .is_err());

        assert_eq!(
            fs::read(app.join("Contents/Resources/app.asar")).unwrap(),
            orig_asar
        );
        let (backup, state) = paths(&app);
        assert!(!backup.exists());
        assert!(!state.exists());
    }

    #[test]
    fn legacy_v2_schema_1_backup_and_marker_restore_seamlessly() {
        let m = test_manifest();
        let ops = MockInstallerOps::default();
        let (_tmp, root) = canonical_temp_dir();
        let (app, orig_asar) = create_darwin_app(&root, "2.32.1");
        let orig_plist = fs::read(app.join("Contents/Info.plist")).unwrap();
        let (backup, state) = paths(&app);
        copy_dir_recursive(&app, &backup).unwrap();

        // Simulate legacy v2 patched app with /* TeacherXuAI Antigravity Chinese UI */
        let orig_arch = AsarArchive::parse(&orig_asar).unwrap();
        let mut legacy_preload = orig_arch.read("dist/preload.js").unwrap();
        legacy_preload.extend_from_slice(b"\n/* TeacherXuAI Antigravity Chinese UI */\n");
        let patched_asar = orig_arch.replace("dist/preload.js", legacy_preload).unwrap();
        fs::write(app.join("Contents/Resources/app.asar"), &patched_asar).unwrap();
        write_darwin_plist(&app, "2.32.1", &patched_asar);

        let schema1 = json!({
            "schema": 1,
            "app": app.to_string_lossy(),
            "version": "2.32.1",
            "original_asar": sha256_hex(&orig_asar),
            "patched_asar": sha256_hex(&patched_asar),
            "original_plist": sha256_hex(&orig_plist),
        });
        fs::write(&state, serde_json::to_vec_pretty(&schema1).unwrap()).unwrap();

        let info = inspect_app_with_manifest(&app, &m).unwrap();
        assert!(info.patched);
        assert_eq!(info.patch_generation.as_deref(), Some("legacy-v2"));

        let restored = restore_app_with(&app, &ops).unwrap();
        assert!(restored.restored);
        assert_eq!(
            fs::read(app.join("Contents/Resources/app.asar")).unwrap(),
            orig_asar
        );
        assert!(!backup.exists());
        assert!(!state.exists());
    }

    #[test]
    fn windows_install_stale_backup_interrupted_recovery_and_update_guard() {
        let m = test_manifest();
        let ops = MockInstallerOps::default();
        let dict = LoadedCustomDict::empty();
        let (_tmp, root) = canonical_temp_dir();
        let app = root.join("Antigravity");
        fs::create_dir_all(app.join("resources")).unwrap();
        fs::write(app.join("Antigravity.exe"), b"MZ-fake-2.31.2").unwrap();

        let pkg_2312 = json!({
            "name": "antigravity",
            "productName": "Antigravity",
            "version": "2.31.2"
        })
        .to_string()
        .into_bytes();
        let asar_2312 = build_test_archive(&[
            ("package.json", &pkg_2312),
            ("dist/preload.js", fake_preload_bytes()),
            ("dist/main.js", fake_main_bytes()),
            ("dist/tray.js", fake_tray_bytes()),
            ("asset", b"win-2312"),
        ]);
        fs::write(app.join("resources/app.asar"), &asar_2312).unwrap();

        let rep = install_app_with(&app, InstallOptions::default(), &dict, &m, &ops).unwrap();
        assert_eq!(rep.platform, "win32");
        assert!(rep.patched);

        let (backup, state) = paths(&app);
        // Simulate official auto-update overwriting app with 2.32.1 while 2.31.2 backup stays
        let pkg_2321 = json!({
            "name": "antigravity",
            "productName": "Antigravity",
            "version": "2.32.1"
        })
        .to_string()
        .into_bytes();
        let asar_2321 = build_test_archive(&[
            ("package.json", &pkg_2321),
            ("dist/preload.js", fake_preload_bytes()),
            ("dist/main.js", fake_main_bytes()),
            ("dist/tray.js", fake_tray_bytes()),
            ("asset", b"win-2321"),
        ]);
        fs::write(app.join("resources/app.asar"), &asar_2321).unwrap();

        // Restore must refuse to overwrite updated app
        assert!(restore_app_with(&app, &ops).is_err());
        assert_eq!(fs::read(app.join("resources/app.asar")).unwrap(), asar_2321);

        let doc = build_doctor_report(&inspect_app_with_manifest(&app, &m).unwrap());
        assert_eq!(doc.stale_backup_version.as_deref(), Some("2.31.2"));

        // Install without --replace-stale-backup must fail
        assert!(install_app_with(&app, InstallOptions::default(), &dict, &m, &ops).is_err());

        // Install with --replace-stale-backup succeeds
        install_app_with(
            &app,
            InstallOptions {
                replace_stale_backup: true,
                ..Default::default()
            },
            &dict,
            &m,
            &ops,
        )
        .unwrap();

        // Simulate interrupted restore where app directory is missing between renames
        fs::remove_dir_all(&app).unwrap();
        restore_app_with(&app, &ops).unwrap();
        assert_eq!(fs::read(app.join("resources/app.asar")).unwrap(), asar_2321);
        assert!(!backup.exists());
        assert!(!state.exists());
    }

    #[test]
    fn anchor_fallback_patches_unknown_future_desktop_version() {
        let m = test_manifest();
        let ops = MockInstallerOps::default();
        let dict = LoadedCustomDict::empty();
        let (_tmp, root) = canonical_temp_dir();
        let (app, orig_asar) = create_darwin_app(&root, "2.99.0");

        let info = inspect_app_with_manifest(&app, &m).unwrap();
        assert_eq!(info.plan.mode, CompatibilityMode::AnchorFallback);

        let rep = install_app_with(
            &app,
            InstallOptions {
                acknowledge_signature_change: true,
                allow_local_library_loading: true,
                replace_stale_backup: false,
            },
            &dict,
            &m,
            &ops,
        )
        .unwrap();
        assert!(rep.patched);
        assert_eq!(rep.compatibility_mode, "anchor_fallback");
        assert!(rep.main_swarm_hook);

        restore_app_with(&app, &ops).unwrap();
        assert_eq!(
            fs::read(app.join("Contents/Resources/app.asar")).unwrap(),
            orig_asar
        );
    }

    #[test]
    fn interrupted_before_rename_cleans_only_recovery_record() {
        let ops = MockInstallerOps::default();
        let (_tmp, root) = canonical_temp_dir();
        let (app, orig_asar) = create_darwin_app(&root, "2.22.0");
        let orig_plist = fs::read(app.join("Contents/Info.plist")).unwrap();
        let (_backup, state) = paths(&app);
        fs::write(
            &state,
            serde_json::to_vec(&json!({
                "schema": 1,
                "app": app.to_string_lossy(),
                "version": "2.22.0",
                "original_asar": sha256_hex(&orig_asar),
                "original_plist": sha256_hex(&orig_plist),
                "patched_asar": "not-yet-installed"
            }))
            .unwrap(),
        )
        .unwrap();

        let res = restore_app_with(&app, &ops).unwrap();
        assert!(res.restored);
        assert_eq!(
            res.recovered.as_deref(),
            Some("interrupted before original app was moved")
        );
        assert!(!state.exists());
        assert_eq!(
            fs::read(app.join("Contents/Resources/app.asar")).unwrap(),
            orig_asar
        );
    }

    #[test]
    fn rejects_tampered_fingerprint_version_mismatch_and_symlink_app() {
        let m = test_manifest();
        let (_tmp, root) = canonical_temp_dir();
        let (app, _) = create_darwin_app(&root, "2.29.1");

        // Tamper dist/main.js for verified version 2.29.1
        let bad_asar = build_test_archive(&[
            ("package.json", br#"{"name":"antigravity","version":"2.29.1"}"#),
            ("dist/preload.js", fake_preload_bytes()),
            ("dist/main.js", b"tampered"),
            ("dist/tray.js", fake_tray_bytes()),
        ]);
        fs::write(app.join("Contents/Resources/app.asar"), &bad_asar).unwrap();
        write_darwin_plist(&app, "2.29.1", &bad_asar);
        assert!(inspect_app_with_manifest(&app, &m).is_err());

        // Mismatch Info.plist CFBundleShortVersionString vs package.json version
        let good_asar = build_test_archive(&[
            ("package.json", br#"{"name":"antigravity","version":"2.32.1"}"#),
            ("dist/preload.js", fake_preload_bytes()),
            ("dist/main.js", fake_main_bytes()),
            ("dist/tray.js", fake_tray_bytes()),
        ]);
        fs::write(app.join("Contents/Resources/app.asar"), &good_asar).unwrap();
        write_darwin_plist(&app, "999.0", &good_asar);
        assert!(inspect_app_with_manifest(&app, &m).is_err());

        write_darwin_plist(&app, "2.32.1", &good_asar);
        let alias = root.join("Alias.app");
        crate::platform::create_test_dir_link(&app, &alias);
        let err = inspect_app_with_manifest(&alias, &m).unwrap_err();
        assert!(err.contains("Symlink application paths"));
        let _ = fs::remove_dir(&alias).or_else(|_| fs::remove_file(&alias));
    }

    #[test]
    fn linux_install_and_restore_with_custom_translations() {
        let m = test_manifest();
        let ops = MockInstallerOps::default();
        let dict = LoadedCustomDict {
            path: None,
            entries: vec![["Settings".to_string(), "自定义设置".to_string()]],
            sha256: sha256_hex(b"custom"),
        };
        let (_tmp, root) = canonical_temp_dir();
        let app = root.join("Antigravity-x64");
        fs::create_dir_all(app.join("resources")).unwrap();
        fs::write(app.join("antigravity"), b"\x7fELF-fake-2.32.1").unwrap();
        let orig_asar = build_test_archive(&[
            (
                "package.json",
                br#"{"name":"antigravity","productName":"Antigravity","version":"2.32.1"}"#,
            ),
            ("dist/preload.js", fake_preload_bytes()),
            ("dist/main.js", fake_main_bytes()),
            ("dist/tray.js", fake_tray_bytes()),
            ("asset", b"linux-unchanged"),
        ]);
        fs::write(app.join("resources/app.asar"), &orig_asar).unwrap();

        let rep = install_app_with(&app, InstallOptions::default(), &dict, &m, &ops).unwrap();
        assert!(rep.patched);
        assert_eq!(rep.platform, "linux");

        let patched_info = inspect_app_with_manifest(&app, &m).unwrap();
        let preload_text =
            String::from_utf8(patched_info.archive.read("dist/preload.js").unwrap()).unwrap();
        assert!(preload_text.contains("__AGY_ZH_MODE__ = \"static\""));
        assert!(preload_text.contains("自定义设置"));

        restore_app_with(&app, &ops).unwrap();
        assert_eq!(fs::read(app.join("resources/app.asar")).unwrap(), orig_asar);
    }
}
