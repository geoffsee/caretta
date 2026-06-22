use flate2::read::GzDecoder;
use sha2::{Digest, Sha512};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;
use tar::Archive;

#[path = "src/native_binaries.rs"]
mod native_binaries;

use native_binaries::{
    GROK_VERSION_LOCK, NATIVE_BINARIES, NativeBinaryLock, antigravity_manifest_url,
    antigravity_platform, grok_artifact_fallback_url, grok_artifact_url, grok_platform,
    native_binary_by_agent, parse_antigravity_manifest, runtime_binary_path,
};

pub fn ensure_native_binaries(
    manifest_dir: &Path,
    out_dir: &Path,
    target_os: &str,
    target_arch: &str,
    target_env: Option<&str>,
) {
    let lock_path = manifest_dir.join(GROK_VERSION_LOCK);
    let lock = read_lock(&lock_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", lock_path.display()));

    let stamp_path = out_dir.join("native_binaries.stamp");
    let key = native_cache_key(&lock_path, target_os, target_arch, target_env)
        .unwrap_or_else(|err| panic!("compute native binary cache key: {err}"));

    if stamp_matches(&stamp_path, &key) && native_binaries_present(manifest_dir) {
        return;
    }

    let agy_platform =
        antigravity_platform(target_os, target_arch, target_env).unwrap_or_else(|| {
            panic!("unsupported target for Antigravity CLI bundling: {target_os}-{target_arch}")
        });
    let grok_platform = grok_platform(target_os, target_arch).unwrap_or_else(|| {
        panic!("unsupported target for Grok CLI bundling: {target_os}-{target_arch}")
    });

    install_antigravity(manifest_dir, &agy_platform, &lock.antigravity)
        .unwrap_or_else(|err| panic!("install Antigravity CLI (agy): {err}"));
    install_grok(manifest_dir, &grok_platform, &lock.xai)
        .unwrap_or_else(|err| panic!("install Grok CLI: {err}"));

    write_stamp(&stamp_path, &key).expect("write native binaries stamp");
}

fn native_binaries_present(manifest_dir: &Path) -> bool {
    NATIVE_BINARIES
        .iter()
        .all(|spec| runtime_binary_path(manifest_dir, spec).is_file())
}

fn read_lock(path: &Path) -> io::Result<NativeBinaryLock> {
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn native_cache_key(
    lock_path: &Path,
    target_os: &str,
    target_arch: &str,
    target_env: Option<&str>,
) -> io::Result<String> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"native-binaries-v1");
    hasher.update(target_os.as_bytes());
    hasher.update(target_arch.as_bytes());
    hasher.update(target_env.unwrap_or("").as_bytes());
    hasher.update(file_sha256(lock_path)?.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

fn install_antigravity(
    manifest_dir: &Path,
    platform: &str,
    expected_version: &str,
) -> io::Result<()> {
    let spec = native_binary_by_agent("gemini").expect("gemini native spec");
    let dest = runtime_binary_path(manifest_dir, spec);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let manifest_json = curl_stdout(&antigravity_manifest_url(platform))?;
    let manifest = parse_antigravity_manifest(&manifest_json)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if manifest.version != expected_version {
        println!(
            "cargo:warning=Antigravity manifest version {} differs from lock pin {expected_version}; update native-binaries.lock.json",
            manifest.version
        );
    }

    let staging = manifest_dir.join(format!("agy-{platform}.staging"));
    curl_to_file(&manifest.url, &staging)?;
    let actual = sha512_file(&staging)?;
    if actual != manifest.sha512 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Antigravity checksum mismatch for {} (expected {}, got {actual})",
                manifest.version, manifest.sha512
            ),
        ));
    }

    let extract_dir = manifest_dir.join(format!("agy-{platform}.extract"));
    let _ = fs::remove_dir_all(&extract_dir);
    fs::create_dir_all(&extract_dir)?;
    let tar_gz = File::open(&staging)?;
    Archive::new(GzDecoder::new(tar_gz)).unpack(&extract_dir)?;
    let extracted = extract_dir.join("antigravity");
    if !extracted.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Antigravity archive missing `antigravity` binary",
        ));
    }

    let tmp = dest.with_extension("tmp");
    fs::copy(&extracted, &tmp)?;
    set_executable(&tmp)?;
    fs::rename(&tmp, &dest)?;

    let _ = fs::remove_file(&staging);
    let _ = fs::remove_dir_all(&extract_dir);
    Ok(())
}

fn install_grok(manifest_dir: &Path, platform: &str, version: &str) -> io::Result<()> {
    let spec = native_binary_by_agent("xai").expect("xai native spec");
    let dest = runtime_binary_path(manifest_dir, spec);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let primary = grok_artifact_url(version, platform);
    let fallback = grok_artifact_fallback_url(version, platform);
    let tmp = dest.with_extension("tmp");
    if curl_to_file(&primary, &tmp).is_err() {
        curl_to_file(&fallback, &tmp)?;
    }
    set_executable(&tmp)?;
    smoke_test_grok(&tmp)?;
    fs::rename(&tmp, &dest)?;
    Ok(())
}

fn smoke_test_grok(path: &Path) -> io::Result<()> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("failed to run `{} --version`: {err}", path.display()),
            )
        })?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "downloaded grok failed smoke test: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    Ok(())
}

fn curl_to_file(url: &str, dest: &Path) -> io::Result<()> {
    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(dest)
        .arg(url)
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!("curl download failed for {url}")));
    }
    Ok(())
}

fn curl_stdout(url: &str) -> io::Result<String> {
    let output = Command::new("curl").args(["-fsSL"]).arg(url).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!("curl fetch failed for {url}")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn sha512_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha512::new();
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

fn file_sha256(path: impl AsRef<Path>) -> io::Result<String> {
    let mut file = File::open(path.as_ref())?;
    let mut hasher = sha2::Sha256::new();
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

fn stamp_matches(path: &Path, expected: &str) -> bool {
    fs::read_to_string(path)
        .map(|contents| contents.trim() == expected)
        .unwrap_or(false)
}

fn write_stamp(path: &Path, value: &str) -> io::Result<()> {
    fs::write(path, value)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    let mut perms = metadata.permissions();
    perms.set_mode(perms.mode() | 0o755);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn append_native_binaries(
    archive: &mut tar::Builder<flate2::write::GzEncoder<File>>,
    manifest_dir: &Path,
) -> io::Result<()> {
    for spec in NATIVE_BINARIES {
        let path = runtime_binary_path(manifest_dir, spec);
        if path.is_file() {
            archive.append_path_with_name(&path, spec.runtime_path)?;
        }
    }
    Ok(())
}
