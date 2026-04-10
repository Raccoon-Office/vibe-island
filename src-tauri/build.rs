use std::process::Command;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.parent().unwrap().to_path_buf();
    let bridge_manifest = workspace_root.join("src-bridge").join("Cargo.toml");

    if bridge_manifest.exists() {
        let out = Command::new("cargo")
            .args(["build", "--release", "--manifest-path"])
            .arg(&bridge_manifest)
            .output();

        if let Ok(output) = out {
            if output.status.success() {
                let src = workspace_root
                    .join("src-bridge")
                    .join("target")
                    .join("release")
                    .join("vibe-bridge");

                let dest_dir = manifest_dir.join("resources");
                let _ = std::fs::create_dir_all(&dest_dir);
                let dest = dest_dir.join("vibe-bridge");

                if src.exists() {
                    let _ = std::fs::copy(&src, &dest);
                    println!("cargo:warning=Copied vibe-bridge to resources/");
                }
            }
        }
    }

    tauri_build::build()
}
