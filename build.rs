use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn main() {
    let required = [
        "dist/desktop-runtime.js",
        "dist/swarm-runtime.js",
        "dist/main-swarm-hook.js",
        "dist/zh-CN.bundle.json",
        "tools/compatibility.json",
    ];
    println!("cargo:rerun-if-changed=build.rs");
    let mut hasher = Sha256::new();

    for rel in required {
        println!("cargo:rerun-if-changed={rel}");
        let path = Path::new(rel);
        let bytes = fs::read(path).unwrap_or_else(|e| {
            panic!("Missing required payload file `{rel}` ({e}). Run `npm run build:payloads` first.");
        });
        assert!(
            !bytes.is_empty(),
            "Required payload file `{rel}` is empty. Run `npm run build:payloads`."
        );
        hasher.update(rel.as_bytes());
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }

    let payload_sha256 = hex::encode(hasher.finalize());
    let bundle_text = fs::read_to_string("dist/zh-CN.bundle.json")
        .expect("Failed to read dist/zh-CN.bundle.json");
    let bundle_json: serde_json::Value = serde_json::from_str(&bundle_text)
        .expect("Invalid JSON in dist/zh-CN.bundle.json");
    let translation_count = bundle_json
        .get("exact")
        .and_then(|v| v.as_object())
        .map(|m| m.len())
        .expect("Missing `exact` dictionary object in dist/zh-CN.bundle.json");

    println!("cargo:rustc-env=AGY_ZH_PAYLOAD_SHA256={payload_sha256}");
    println!("cargo:rustc-env=AGY_ZH_TRANSLATION_COUNT={translation_count}");
}
