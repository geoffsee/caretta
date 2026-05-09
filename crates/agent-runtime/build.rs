use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::{Builder, EntryType, Header};

#[path = "src/bundled_agents.rs"]
mod bundled_agents;

#[path = "src/utilities.rs"]
mod utilities;

#[path = "src/available_models.rs"]
mod available_models;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/available_models.rs");
    println!("cargo:rerun-if-changed=src/bundled_agents.rs");
    println!("cargo:rerun-if-changed=src/utilities.rs");
    // Cache key intentionally tracks ONLY `bun.lock`. We do not re-run when
    // `package.json` changes on its own (a lockfile update will reflect any
    // dependency change), and we do not invalidate on `BUN` binary path
    // changes — switching Bun versions does not require re-archiving the
    // agent runtime. This keeps incremental builds fast.
    println!("cargo:rerun-if-changed=bun.lock");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("freq-ai-agent-runtime should live at workspace/crates/agent-runtime");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS");
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH");
    let bundle_runtime = env::var_os("CARGO_FEATURE_BUNDLE_RUNTIME").is_some();
    let bun_path = resolve_bun_path();
    let install_stamp = out_dir.join("bun_install.stamp");

    let install_key =
        install_cache_key(&manifest_dir, &target_os, &target_arch).expect("compute install key");
    run_bun_install_if_needed(&manifest_dir, &bun_path, &install_stamp, &install_key);
    // Run after the gated bun-install too: the workflow's "Install Agents"
    // step performs its own `bun install`, and the stamp we wrote on a prior
    // cargo run can short-circuit `run_bun_install_if_needed` while leaving
    // the freshly-installed stub binary unrepaired in `node_modules/`.
    ensure_claude_native_binary(&manifest_dir, &bun_path);

    available_models::scan_available_models(repo_root, &manifest_dir).unwrap_or_else(|err| {
        panic!(
            "failed to write {}: {err}",
            repo_root.join("assets/available-models.json").display()
        );
    });

    let archive_name = format!("freq-ai-agents-{target_os}-{target_arch}.tar.gz");
    let generated_path = out_dir.join("agent_runtime_generated.rs");

    if bundle_runtime {
        let archive_path = out_dir.join(&archive_name);
        let archive_stamp = out_dir.join("runtime_archive.stamp");
        let archive_sha_file = out_dir.join("runtime_archive.sha256");
        let archive_key = archive_cache_key(&install_key).expect("compute archive cache key");
        let archive_sha256 = create_archive_if_needed(
            &manifest_dir,
            &archive_path,
            &bun_path,
            &archive_stamp,
            &archive_key,
            &archive_sha_file,
        )
        .expect("create/hash agent runtime archive");

        write_generated_bundled(
            &generated_path,
            &archive_name,
            &archive_path,
            &archive_sha256,
            &target_os,
            &target_arch,
        )
        .expect("write bundled agent runtime metadata");
    } else {
        // Dev / non-bundled build: skip the multi-hundred-MB archive. The
        // runtime will mount `node_modules` directly from the crate source.
        let install_short = &install_key[..12];
        write_generated_unbundled(
            &generated_path,
            &archive_name,
            &manifest_dir,
            &bun_path,
            install_short,
            &target_os,
            &target_arch,
        )
        .expect("write unbundled agent runtime metadata");
    }
}

fn run_bun_install_if_needed(manifest_dir: &Path, bun_path: &Path, stamp_path: &Path, key: &str) {
    if stamp_matches(stamp_path, key) && manifest_dir.join("node_modules").is_dir() {
        return;
    }

    let mut cmd = Command::new(bun_path);
    cmd.arg("install");
    cmd.arg("--trust");
    if manifest_dir.join("bun.lock").exists() {
        println!(
            "cargo:warning=Bun lockfile exists but --frozen-lockfile cannot be used; running `bun install --trust to trigger lifecycle scripts`"
        );
        // cmd.arg("--frozen-lockfile");
    }
    let status = cmd
        .current_dir(manifest_dir)
        .status()
        .unwrap_or_else(|err| panic!("failed to run `{}` install`: {err}", bun_path.display()));

    if !status.success() {
        panic!(
            "`{} install` failed with status {status}",
            bun_path.display()
        );
    }

    write_stamp(stamp_path, key).expect("write bun install stamp");
}

/// `@anthropic-ai/claude-code` ships a no-shebang stub at `bin/claude.exe` and
/// expects its `install.cjs` postinstall to overwrite it with the platform
/// native binary from `@anthropic-ai/claude-code-{platform}`. Bun's handling
/// of `trustedDependencies` postinstalls is inconsistent across CI containers
/// — when it skips, the stub remains and `execve` returns ENOEXEC on Linux.
/// Run the install script ourselves so the binary is always materialised.
fn ensure_claude_native_binary(manifest_dir: &Path, bun_path: &Path) {
    let pkg_dir = manifest_dir.join("node_modules/@anthropic-ai/claude-code");
    let install_script = pkg_dir.join("install.cjs");
    if !install_script.is_file() {
        return;
    }
    // Bun runs CommonJS scripts in node-compat mode. We deliberately ignore
    // the exit status: the script self-reports an "unsupported platform" or
    // "native package missing" failure to stderr without exiting non-zero.
    let _ = Command::new(bun_path)
        .arg("install.cjs")
        .current_dir(&pkg_dir)
        .status();
}

fn create_archive_if_needed(
    manifest_dir: &Path,
    archive_path: &Path,
    bun_path: &Path,
    stamp_path: &Path,
    key: &str,
    sha_path: &Path,
) -> io::Result<String> {
    if stamp_matches(stamp_path, key)
        && archive_path.is_file()
        && sha_path.is_file()
        && let Ok(existing) = fs::read_to_string(sha_path)
    {
        let existing = existing.trim();
        if !existing.is_empty() {
            return Ok(existing.to_string());
        }
    }

    let archive_sha256 = create_archive(manifest_dir, archive_path, bun_path)?;
    fs::write(sha_path, &archive_sha256)?;
    write_stamp(stamp_path, key)?;
    Ok(archive_sha256)
}

fn create_archive(manifest_dir: &Path, archive_path: &Path, bun_path: &Path) -> io::Result<String> {
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tar_gz = File::create(archive_path)?;
    let level = match env::var("PROFILE").as_deref() {
        Ok("release") => Compression::default(),
        _ => Compression::fast(),
    };
    let encoder = GzEncoder::new(tar_gz, level);
    let mut archive = Builder::new(encoder);

    append_file(&mut archive, manifest_dir, "package.json")?;
    append_file(&mut archive, manifest_dir, "bun.lock")?;
    append_bun_runtime(&mut archive, bun_path)?;
    archive.append_dir_all("node_modules", manifest_dir.join("node_modules"))?;
    archive.finish()?;
    archive.into_inner()?.finish()?;
    sha256_file(archive_path)
}

fn append_file(
    archive: &mut Builder<GzEncoder<File>>,
    manifest_dir: &Path,
    relative: &str,
) -> io::Result<()> {
    archive.append_path_with_name(manifest_dir.join(relative), relative)
}

fn append_bun_runtime(archive: &mut Builder<GzEncoder<File>>, bun_path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        archive.append_path_with_name(bun_path, "bin/bun.exe")?;
        archive.append_path_with_name(bun_path, "bin/node.exe")?;
    }

    #[cfg(not(windows))]
    {
        archive.append_path_with_name(bun_path, "bin/bun")?;
        let mut header = Header::new_gnu();
        header.set_entry_type(EntryType::Symlink);
        header.set_mode(0o755);
        header.set_size(0);
        archive.append_link(&mut header, "bin/node", "bun")?;
    }

    Ok(())
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 1024 * 64];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_generated_bundled(
    path: &Path,
    archive_name: &str,
    archive_path: &Path,
    archive_sha256: &str,
    target_os: &str,
    target_arch: &str,
) -> io::Result<()> {
    let archive_path = archive_path.display().to_string().replace('\\', "\\\\");
    let short_hash = &archive_sha256[..12];
    let source = format!(
        r##"pub const TARGET_OS: &str = {target_os:?};
pub const TARGET_ARCH: &str = {target_arch:?};
pub const ARCHIVE_NAME: &str = {archive_name:?};
pub const ARCHIVE_SHA256: &str = {archive_sha256:?};
pub const ARCHIVE_SHORT_SHA256: &str = {short_hash:?};

#[cfg(feature = "bundle-runtime")]
pub const ARCHIVE_BYTES: &[u8] = include_bytes!(r#"{archive_path}"#);
"##
    );
    File::create(path)?.write_all(source.as_bytes())
}

fn write_generated_unbundled(
    path: &Path,
    archive_name: &str,
    manifest_dir: &Path,
    bun_path: &Path,
    install_short: &str,
    target_os: &str,
    target_arch: &str,
) -> io::Result<()> {
    let manifest_dir_lit = manifest_dir.display().to_string().replace('\\', "\\\\");
    let bun_path_lit = bun_path.display().to_string().replace('\\', "\\\\");
    let source = format!(
        r##"pub const TARGET_OS: &str = {target_os:?};
pub const TARGET_ARCH: &str = {target_arch:?};
pub const ARCHIVE_NAME: &str = {archive_name:?};
// Without the `bundle-runtime` feature there is no embedded archive. We still
// expose a stable identifier so consumers (e.g. `default_runtime_root`) can
// scope their working directory; it is derived from the install cache key.
pub const ARCHIVE_SHA256: &str = {install_short:?};
pub const ARCHIVE_SHORT_SHA256: &str = {install_short:?};

/// Absolute path to the `agent-runtime` crate source directory at build time.
/// Used in non-bundled builds to reach the locally installed `node_modules`.
pub const MANIFEST_DIR: &str = r#"{manifest_dir_lit}"#;
/// Absolute path to the resolved Bun binary at build time. Used in non-bundled
/// builds to expose `bun`/`node` without unpacking an embedded archive.
pub const BUN_PATH: &str = r#"{bun_path_lit}"#;
"##
    );
    File::create(path)?.write_all(source.as_bytes())
}

fn install_cache_key(
    manifest_dir: &Path,
    target_os: &str,
    target_arch: &str,
) -> io::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(target_os.as_bytes());
    hasher.update(target_arch.as_bytes());
    // Only the lockfile contributes to the cache key — package.json edits or
    // Bun binary upgrades do not invalidate the bundled runtime on their own.
    hasher.update(file_sha256(manifest_dir.join("bun.lock"))?.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

fn archive_cache_key(install_key: &str) -> io::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"archive-v1");
    hasher.update(install_key.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

fn file_sha256(path: impl AsRef<Path>) -> io::Result<String> {
    sha256_file(path.as_ref())
}

fn stamp_matches(path: &Path, expected: &str) -> bool {
    fs::read_to_string(path)
        .map(|contents| contents.trim() == expected)
        .unwrap_or(false)
}

fn write_stamp(path: &Path, value: &str) -> io::Result<()> {
    fs::write(path, value)
}

fn resolve_bun_path() -> PathBuf {
    let bun = env::var_os("BUN").unwrap_or_else(|| OsString::from("bun"));
    let bun_path = PathBuf::from(&bun);

    if is_path_like(&bun_path) {
        return fs::canonicalize(&bun_path).unwrap_or_else(|err| {
            panic!("failed to resolve BUN path `{}`: {err}", bun_path.display())
        });
    }

    let Some(path) = env::var_os("PATH") else {
        panic!("PATH is not set, and BUN was not an explicit path");
    };

    for dir in env::split_paths(&path) {
        for candidate in executable_candidates(&bun) {
            let path = dir.join(candidate);
            if path.is_file() {
                return fs::canonicalize(&path)
                    .unwrap_or_else(|err| panic!("failed to resolve `{}`: {err}", path.display()));
            }
        }
    }

    panic!("failed to find Bun executable; install Bun or set BUN=/path/to/bun");
}

fn is_path_like(path: &Path) -> bool {
    path.is_absolute() || path.components().count() > 1
}

#[cfg(windows)]
fn executable_candidates(program: &OsString) -> Vec<OsString> {
    let mut candidates = vec![program.clone()];
    let program = program.to_string_lossy();
    if !program.to_ascii_lowercase().ends_with(".exe") {
        candidates.push(OsString::from(format!("{program}.exe")));
    }
    candidates
}

#[cfg(not(windows))]
fn executable_candidates(program: &OsString) -> Vec<OsString> {
    vec![program.clone()]
}
