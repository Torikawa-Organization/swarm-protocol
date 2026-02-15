use std::path::PathBuf;

use walkdir::WalkDir;

fn main() -> std::io::Result<()> {
    // Re-run build script when any file in Protocol/ changes (including new files).
    let proto_dir = PathBuf::from("../../../Protocol");
    println!("cargo:rerun-if-changed={}", proto_dir.display());
    let protos: Vec<PathBuf> = WalkDir::new(&proto_dir)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.into_path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("proto"))
        .collect();

    // Generate a single include file that pulls in all generated modules.
    let mut cfg = prost_build::Config::new();
    cfg.include_file("includes.rs"); // writes into OUT_DIR :contentReference[oaicite:2]{index=2}

    // Compile everything (includes path is proto/)
    cfg.compile_protos(&protos, &[proto_dir])?;

    Ok(())
}
