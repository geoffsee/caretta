use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

fn main() {
    generate_asset_manifest();
    let dist = Path::new("dist");
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Tell server.rs where to find the web assets folder. When the real
    // dist/ exists (local dev build), use it. Otherwise create an empty
    // stub in OUT_DIR so RustEmbed compiles without error.
    if dist.exists() {
        let abs = std::fs::canonicalize(dist).expect("canonicalize dist/");
        println!("cargo::rustc-env=WEB_ASSETS_DIR={}", abs.display());
    } else {
        let stub = Path::new(&out_dir).join("dist-stub");
        std::fs::create_dir_all(&stub).expect("create dist-stub");
        println!("cargo::rustc-env=WEB_ASSETS_DIR={}", stub.display());
    }

    let wasm_path = Path::new("dist/wasm/caretta_bg.wasm");

    println!("cargo::rerun-if-changed={}", wasm_path.display());

    if !wasm_path.exists() {
        return;
    }

    let meta = std::fs::metadata(wasm_path).expect("failed to read wasm metadata");
    // Only optimize if larger than 10MB (i.e. not already optimized)
    if meta.len() <= 10 * 1024 * 1024 {
        return;
    }

    let wasm_opt = option_env!("WASM_OPT").unwrap_or("wasm-opt");
    let status = Command::new(wasm_opt)
        .args(["-Oz", "--strip-debug"])
        .arg(wasm_path)
        .arg("-o")
        .arg(wasm_path)
        .status();

    match status {
        Ok(s) if s.success() => {
            let new_size = std::fs::metadata(wasm_path)
                .map(|m| m.len() / (1024 * 1024))
                .unwrap_or(0);
            println!(
                "cargo::warning=wasm-opt: optimized caretta_bg.wasm to {}MB",
                new_size
            );
        }
        Ok(s) => println!("cargo::warning=wasm-opt exited with {s}, skipping optimization"),
        Err(_) => println!("cargo::warning=wasm-opt not found, skipping wasm optimization"),
    }
}

/// Walk `assets/skills/` and `assets/workflows/`, compute a SHA-256 hash for
/// every file, and write the results as a static Rust array to OUT_DIR so
/// `assets.rs` can include it for integrity verification.
fn generate_asset_manifest() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let manifest_dir_str = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let manifest_dir = Path::new(&manifest_dir_str);

    // Re-run when files are added to, removed from, or edited in these dirs.
    println!("cargo::rerun-if-changed={}", manifest_dir.join("assets/skills").display());
    println!("cargo::rerun-if-changed={}", manifest_dir.join("assets/workflows").display());

    let mut entries: Vec<(String, String)> = Vec::new();

    for prefix in ["skills", "workflows"] {
        let dir = manifest_dir.join("assets").join(prefix);
        if !dir.is_dir() {
            continue;
        }
        collect_hashes(&dir, prefix, &mut entries)
            .unwrap_or_else(|e| panic!("failed to hash asset files in {prefix}: {e}"));
    }

    // Hash individual assets embedded via include_str! so they are also
    // covered by the integrity manifest.
    for file in ["AGENTS.md", "labels.yml", "available-models.json"] {
        let abs = manifest_dir.join("assets").join(file);
        println!("cargo::rerun-if-changed={}", abs.display());
        if abs.is_file() {
            let hash = sha256_file(&abs)
                .unwrap_or_else(|e| panic!("failed to hash {file}: {e}"));
            entries.push((file.to_string(), hash));
        }
    }

    // Sort for deterministic output regardless of filesystem order.
    entries.sort();

    let mut source = String::from("pub static ASSET_MANIFEST: &[(&str, &str)] = &[\n");
    for (path, hash) in &entries {
        source.push_str(&format!("    ({path:?}, {hash:?}),\n"));
    }
    source.push_str("];\n");

    let out_path = Path::new(&out_dir).join("asset_manifest_generated.rs");
    fs::write(&out_path, source)
        .unwrap_or_else(|e| panic!("failed to write asset_manifest_generated.rs: {e}"));
}

fn collect_hashes(dir: &Path, prefix: &str, entries: &mut Vec<(String, String)>) -> io::Result<()> {
    for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(dir)
            .expect("strip assets prefix")
            .to_str()
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidData,
                format!("non-UTF-8 asset path: {:?}", entry.path()),
            ))?
            .replace('\\', "/");
        let manifest_key = format!("{prefix}/{rel}");
        let hash = sha256_file(entry.path())?;
        entries.push((manifest_key, hash));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
