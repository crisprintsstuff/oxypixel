//! Manual update check against the public GitHub releases.
//!
//! Nothing in this module runs until the user asks. Downloads are accepted
//! only from this repository's release URLs, and only after the file's
//! SHA-256 matches `SHA256SUMS.txt` from the same release.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

const REPO: &str = "crisprintsstuff/oxypixel";
const LATEST_URL: &str = "https://api.github.com/repos/crisprintsstuff/oxypixel/releases/latest";
const MAX_DOWNLOAD: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn current() -> Self {
        parse_version(env!("CARGO_PKG_VERSION")).expect("package version is major.minor.patch")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Behind,
    Same,
    Ahead,
}

/// Which release file Check for updates downloads.
///
/// Lookup uses the full asset name. The Linux `.deb` is its own kind, so a
/// release that also publishes `oxypixel-<version>-linux-x86_64.deb` is not
/// treated as the portable tar.gz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageKind {
    WindowsPortable,
    WindowsInstaller,
    LinuxArchive,
    /// The `.deb` copy installed at `/usr/bin/oxypixel`.
    LinuxDeb,
}

impl PackageKind {
    pub fn for_current_process() -> Self {
        if cfg!(windows) {
            if installed_copy() {
                Self::WindowsInstaller
            } else {
                Self::WindowsPortable
            }
        } else {
            match std::env::current_exe() {
                Ok(path) => Self::for_linux_exe(&path),
                Err(_) => Self::LinuxArchive,
            }
        }
    }

    /// `/usr/bin/oxypixel` is the `.deb`. Any other path is the portable archive.
    fn for_linux_exe(path: &Path) -> Self {
        if is_deb_install(path) {
            Self::LinuxDeb
        } else {
            Self::LinuxArchive
        }
    }

    fn asset_name(self, version: Version) -> String {
        match self {
            Self::WindowsPortable => format!("oxypixel-{version}-windows-x86_64.exe"),
            Self::WindowsInstaller => format!("OxyPixel-{version}-Setup.exe"),
            Self::LinuxArchive => format!("oxypixel-{version}-linux-x86_64.tar.gz"),
            Self::LinuxDeb => format!("oxypixel-{version}-linux-x86_64.deb"),
        }
    }
}

fn is_deb_install(path: &Path) -> bool {
    let packaged = Path::new("/usr/bin/oxypixel");
    if path == packaged {
        return true;
    }
    match path.canonicalize() {
        Ok(canonical) => canonical == packaged,
        Err(_) => false,
    }
}

fn asset_for(release: &ReleaseInfo, kind: PackageKind) -> Option<ReleaseAsset> {
    let name = kind.asset_name(release.version);
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .cloned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: Version,
    pub notes: String,
    pub page_url: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateReport {
    pub current: Version,
    pub release: ReleaseInfo,
    pub relation: Relation,
    pub package: PackageKind,
    pub asset: Option<ReleaseAsset>,
    pub checksums: Option<ReleaseAsset>,
}

pub fn parse_version(text: &str) -> Option<Version> {
    let text = text.trim().trim_start_matches('v');
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Version {
        major,
        minor,
        patch,
    })
}

pub fn relation(current: Version, latest: Version) -> Relation {
    match current.cmp(&latest) {
        std::cmp::Ordering::Less => Relation::Behind,
        std::cmp::Ordering::Equal => Relation::Same,
        std::cmp::Ordering::Greater => Relation::Ahead,
    }
}

pub fn parse_release(json: &str) -> Result<ReleaseInfo, String> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|err| format!("GitHub returned unexpected data: {err}"))?;
    let tag = value
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "the release has no tag".to_string())?
        .to_string();
    let version = parse_version(&tag).ok_or_else(|| format!("cannot read a version from {tag}"))?;
    let notes = value
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let page_url = value
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if !page_url.is_empty() && !allowed_page(&page_url) {
        return Err("the release page is not on this GitHub repository".to_string());
    }
    let mut assets = Vec::new();
    for item in value
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "the release has no files".to_string())?
    {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let download_url = item
            .get("browser_download_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || !allowed_download(&download_url) {
            continue;
        }
        let size = item.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        assets.push(ReleaseAsset {
            name,
            download_url,
            size,
        });
    }
    Ok(ReleaseInfo {
        tag,
        version,
        notes,
        page_url,
        assets,
    })
}

pub fn report_from_release(release: ReleaseInfo) -> UpdateReport {
    let current = Version::current();
    let package = PackageKind::for_current_process();
    let asset = asset_for(&release, package);
    let checksums = release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS.txt")
        .cloned();
    let relation = relation(current, release.version);
    UpdateReport {
        current,
        release,
        relation,
        package,
        asset,
        checksums,
    }
}

pub fn check_latest() -> Result<UpdateReport, String> {
    let body = http_get(LATEST_URL, true)?;
    let body = String::from_utf8(body)
        .map_err(|_| "GitHub returned a response that is not text".to_string())?;
    let release = parse_release(&body)?;
    Ok(report_from_release(release))
}

pub fn download_verified(
    asset: &ReleaseAsset,
    checksums: &ReleaseAsset,
) -> Result<Vec<u8>, String> {
    let sums = http_get(&checksums.download_url, false)?;
    let sums = String::from_utf8(sums).map_err(|_| "checksum file is not text".to_string())?;
    let expected = expected_hash(&sums, &asset.name)
        .ok_or_else(|| format!("SHA256SUMS.txt has no entry for {}", asset.name))?;
    let bytes = http_get(&asset.download_url, false)?;
    if !hash_matches(&bytes, &expected) {
        return Err(format!(
            "{} does not match the checksum published with the release",
            asset.name
        ));
    }
    Ok(bytes)
}

pub fn expected_hash(sums: &str, file_name: &str) -> Option<String> {
    for line in sums.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let mut name = parts.next()?;
        if let Some(stripped) = name.strip_prefix('*') {
            name = stripped;
        }
        if parts.next().is_some()
            || hash.len() != 64
            || !hash.chars().all(|c| c.is_ascii_hexdigit())
        {
            continue;
        }
        if name == file_name {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

pub fn hash_matches(bytes: &[u8], expected_hex: &str) -> bool {
    let digest = Sha256::digest(bytes);
    let mut actual = String::with_capacity(64);
    for byte in digest {
        actual.push_str(&format!("{byte:02x}"));
    }
    actual.eq_ignore_ascii_case(expected_hex.trim())
}

pub fn allowed_download(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    if url.contains('@') || rest.contains("..") || rest.contains('\\') {
        return false;
    }
    let prefix = format!("{REPO}/releases/download/");
    let Some(path) = rest.strip_prefix(&prefix) else {
        return false;
    };
    !path.is_empty() && !path.ends_with('/')
}

fn allowed_page(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    !url.contains('@') && rest.starts_with(&format!("{REPO}/"))
}

pub fn write_download(file_name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let name = Path::new(file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "the release file name is not usable".to_string())?;
    if name != file_name || name.contains("..") {
        return Err("the release file name is not usable".to_string());
    }
    let dir = std::env::temp_dir().join(format!("oxypixel-update-{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join(name);
    let mut file = fs::File::create(&path).map_err(|err| err.to_string())?;
    file.write_all(bytes).map_err(|err| err.to_string())?;
    Ok(path)
}

/// Extract the `oxypixel` executable from a release archive.
pub fn linux_binary_from_archive(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|err| format!("could not read the Linux archive: {err}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|err| format!("could not read the Linux archive: {err}"))?;
        let path = entry
            .path()
            .map_err(|err| format!("could not read the Linux archive: {err}"))?;
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("the Linux archive contains an unsafe path".to_string());
        }
        if path.file_name().and_then(|n| n.to_str()) != Some("oxypixel") {
            continue;
        }
        let mut program = Vec::new();
        entry
            .read_to_end(&mut program)
            .map_err(|err| format!("could not read the Linux archive: {err}"))?;
        if program.is_empty() {
            return Err("the Linux archive has an empty oxypixel program".to_string());
        }
        return Ok(program);
    }
    Err("the Linux archive does not contain the oxypixel program".to_string())
}

pub fn launch_installer(path: &Path) -> Result<(), String> {
    let mut command = std::process::Command::new(path);
    spawn_detached(&mut command)
}

/// Windows cannot replace the exe that is still running. A small command
/// script waits, then moves the downloaded program into place and starts it.
pub fn stage_windows_restart(
    current: &Path,
    new_bytes: &[u8],
    reopen: Option<&Path>,
) -> Result<(), String> {
    let parent = current
        .parent()
        .ok_or_else(|| "cannot find the folder OxyPixel is running from".to_string())?;
    let staged = parent.join("oxypixel-update.exe");
    let script = parent.join("oxypixel-update.cmd");
    fs::write(&staged, new_bytes)
        .map_err(|err| format!("cannot write an update beside the current program: {err}"))?;
    let reopen = reopen
        .map(|path| format!(" \"{}\"", path.display()))
        .unwrap_or_default();
    let body = format!(
        "@echo off\r\nping 127.0.0.1 -n 3 >nul\r\nmove /Y \"{}\" \"{}\"\r\nif errorlevel 1 exit /b 1\r\nstart \"\" \"{}\"{reopen}\r\ndel \"%~f0\"\r\n",
        staged.display(),
        current.display(),
        current.display(),
    );
    fs::write(&script, body).map_err(|err| err.to_string())?;
    let mut command = std::process::Command::new("cmd");
    command.arg("/C").arg(&script);
    spawn_detached(&mut command)
}

pub fn replace_and_restart(
    current: &Path,
    new_bytes: &[u8],
    reopen: Option<&Path>,
) -> Result<(), String> {
    let parent = current
        .parent()
        .ok_or_else(|| "cannot find the folder OxyPixel is running from".to_string())?;
    let staged = parent.join("oxypixel.new");
    {
        let mut file = fs::File::create(&staged)
            .map_err(|err| format!("cannot write an update beside the current program: {err}"))?;
        file.write_all(new_bytes).map_err(|err| err.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = file
                .metadata()
                .map_err(|err| err.to_string())?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&staged, perms).map_err(|err| err.to_string())?;
        }
    }
    fs::rename(&staged, current).map_err(|err| format!("could not replace OxyPixel: {err}"))?;
    let mut command = std::process::Command::new(current);
    if let Some(path) = reopen {
        command.arg(path);
    }
    spawn_detached(&mut command)?;
    Ok(())
}

fn installed_copy() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    exe.parent()
        .map(|dir| dir.join("Uninstall.exe").is_file())
        .unwrap_or(false)
}

fn http_get(url: &str, api: bool) -> Result<Vec<u8>, String> {
    if api {
        if url != LATEST_URL {
            return Err(
                "refusing to contact a URL that is not the OxyPixel release feed".to_string(),
            );
        }
    } else if !allowed_download(url) {
        return Err("refusing to download a file from outside the OxyPixel releases".to_string());
    }
    let response = agent()
        .get(url)
        .set(
            "User-Agent",
            &format!("OxyPixel/{}", env!("CARGO_PKG_VERSION")),
        )
        .set(
            "Accept",
            if api {
                "application/vnd.github+json"
            } else {
                "application/octet-stream"
            },
        )
        .call()
        .map_err(|err| format!("could not reach GitHub: {err}"))?;
    let mut reader = response.into_reader().take(MAX_DOWNLOAD as u64 + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| format!("the download stopped: {err}"))?;
    if bytes.len() > MAX_DOWNLOAD {
        return Err("the download is larger than 32 MiB".to_string());
    }
    Ok(bytes)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .build()
}

fn spawn_detached(command: &mut std::process::Command) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Detach from this console and keep the new process alive after we exit.
        command.creation_flags(0x0000_0008 | 0x0800_0000);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("could not start the updated OxyPixel: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_release() -> &'static str {
        r#"{
          "tag_name": "v0.2.1",
          "html_url": "https://github.com/crisprintsstuff/oxypixel/releases/tag/v0.2.1",
          "body": "A small update.",
          "assets": [
            {
              "name": "oxypixel-0.2.1-windows-x86_64.exe",
              "browser_download_url": "https://github.com/crisprintsstuff/oxypixel/releases/download/v0.2.1/oxypixel-0.2.1-windows-x86_64.exe",
              "size": 10
            },
            {
              "name": "OxyPixel-0.2.1-Setup.exe",
              "browser_download_url": "https://github.com/crisprintsstuff/oxypixel/releases/download/v0.2.1/OxyPixel-0.2.1-Setup.exe",
              "size": 11
            },
            {
              "name": "oxypixel-0.2.1-linux-x86_64.tar.gz",
              "browser_download_url": "https://github.com/crisprintsstuff/oxypixel/releases/download/v0.2.1/oxypixel-0.2.1-linux-x86_64.tar.gz",
              "size": 12
            },
            {
              "name": "SHA256SUMS.txt",
              "browser_download_url": "https://github.com/crisprintsstuff/oxypixel/releases/download/v0.2.1/SHA256SUMS.txt",
              "size": 13
            },
            {
              "name": "evil.exe",
              "browser_download_url": "https://evil.example/oxypixel.exe",
              "size": 1
            }
          ]
        }"#
    }

    #[test]
    fn versions_compare_numerically() {
        let older = parse_version("v0.2.0").unwrap();
        let same = parse_version("0.2.1").unwrap();
        let newer = parse_version("v0.3.0").unwrap();
        let current = parse_version("0.2.1").unwrap();
        assert_eq!(relation(current, older), Relation::Ahead);
        assert_eq!(relation(current, same), Relation::Same);
        assert_eq!(relation(current, newer), Relation::Behind);
        assert!(parse_version("1.2").is_none());
    }

    #[test]
    fn release_json_keeps_only_this_repositories_files() {
        let release = parse_release(sample_release()).unwrap();
        assert_eq!(
            release.version,
            Version {
                major: 0,
                minor: 2,
                patch: 1
            }
        );
        assert_eq!(release.assets.len(), 4);
        assert!(release
            .assets
            .iter()
            .all(|a| allowed_download(&a.download_url)));
        let report = report_from_release(release);
        assert!(report.checksums.is_some());
        let expected = match report.package {
            PackageKind::WindowsPortable => "oxypixel-0.2.1-windows-x86_64.exe",
            PackageKind::WindowsInstaller => "OxyPixel-0.2.1-Setup.exe",
            PackageKind::LinuxArchive => "oxypixel-0.2.1-linux-x86_64.tar.gz",
            PackageKind::LinuxDeb => "oxypixel-0.2.1-linux-x86_64.deb",
        };
        assert_eq!(report.asset.unwrap().name, expected);
    }

    #[test]
    fn checksum_must_match_the_named_file() {
        let bytes = b"oxypixel";
        let hash = {
            let digest = Sha256::digest(bytes);
            let mut text = String::new();
            for byte in digest {
                text.push_str(&format!("{byte:02x}"));
            }
            text
        };
        let sums = format!("{hash}  oxypixel-0.2.1-windows-x86_64.exe\n");
        assert_eq!(
            expected_hash(&sums, "oxypixel-0.2.1-windows-x86_64.exe").as_deref(),
            Some(hash.as_str())
        );
        assert!(hash_matches(bytes, &hash));
        assert!(!hash_matches(b"other", &hash));
        assert!(expected_hash("nope", "oxypixel-0.2.1-windows-x86_64.exe").is_none());
    }

    #[test]
    fn release_checksum_list_names_each_package() {
        // PackageKind::asset_name is checked against this package version.
        // A later bump does not have to edit version strings in this test.
        let version = Version::current();
        let portable = PackageKind::WindowsPortable.asset_name(version);
        let installer = PackageKind::WindowsInstaller.asset_name(version);
        let archive = PackageKind::LinuxArchive.asset_name(version);
        assert_eq!(portable, format!("oxypixel-{version}-windows-x86_64.exe"));
        assert_eq!(installer, format!("OxyPixel-{version}-Setup.exe"));
        assert_eq!(archive, format!("oxypixel-{version}-linux-x86_64.tar.gz"));
        let sums = format!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  {portable}\r\n\
             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  {installer}\n\
             cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  {archive}\n"
        );
        assert_eq!(
            expected_hash(&sums, &portable).as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            expected_hash(&sums, &installer).as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert_eq!(
            expected_hash(&sums, &archive).as_deref(),
            Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
        );
        assert!(expected_hash(&sums, &format!("dist/{installer}")).is_none());
    }

    #[test]
    fn foreign_urls_are_rejected() {
        assert!(!allowed_download(
            "http://github.com/crisprintsstuff/oxypixel/releases/download/v0.2.1/a.exe"
        ));
        assert!(!allowed_download(
            "https://github.com/other/oxypixel/releases/download/v0.2.1/a.exe"
        ));
        assert!(!allowed_download(
            "https://github.com/crisprintsstuff/oxypixel/archive/main.zip"
        ));
        assert!(allowed_download(
            "https://github.com/crisprintsstuff/oxypixel/releases/download/v0.2.1/SHA256SUMS.txt"
        ));
    }

    #[test]
    fn linux_archive_extracts_only_the_program() {
        let raw = b"#!/bin/oxypixel\n";
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header
            .set_path("oxypixel-0.2.1-linux-x86_64/oxypixel")
            .unwrap();
        header.set_size(raw.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, &raw[..]).unwrap();
        let tar_bytes = builder.into_inner().unwrap();
        let mut gz = Vec::new();
        {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            let mut encoder = GzEncoder::new(&mut gz, Compression::default());
            encoder.write_all(&tar_bytes).unwrap();
            encoder.finish().unwrap();
        }
        assert_eq!(linux_binary_from_archive(&gz).unwrap(), raw);
    }

    #[test]
    fn deb_asset_does_not_replace_the_linux_archive() {
        // Fixed v0.3.2 release payload. The names are the sample GitHub
        // response, not Version::current().
        let version = Version {
            major: 0,
            minor: 3,
            patch: 2,
        };
        assert_eq!(
            PackageKind::WindowsPortable.asset_name(version),
            "oxypixel-0.3.2-windows-x86_64.exe"
        );
        assert_eq!(
            PackageKind::WindowsInstaller.asset_name(version),
            "OxyPixel-0.3.2-Setup.exe"
        );
        assert_eq!(
            PackageKind::LinuxArchive.asset_name(version),
            "oxypixel-0.3.2-linux-x86_64.tar.gz"
        );
        assert_eq!(
            PackageKind::LinuxDeb.asset_name(version),
            "oxypixel-0.3.2-linux-x86_64.deb"
        );

        let sums = "\
            aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  oxypixel-0.3.2-windows-x86_64.exe\n\
            bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  OxyPixel-0.3.2-Setup.exe\n\
            dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd  oxypixel-0.3.2-linux-x86_64.deb\n\
            cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  oxypixel-0.3.2-linux-x86_64.tar.gz\n";
        assert_eq!(
            expected_hash(sums, &PackageKind::LinuxArchive.asset_name(version)).as_deref(),
            Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
        );
        assert_eq!(
            expected_hash(sums, &PackageKind::LinuxDeb.asset_name(version)).as_deref(),
            Some("dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd")
        );
        assert_eq!(
            expected_hash(sums, &PackageKind::WindowsPortable.asset_name(version)).as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            expected_hash(sums, &PackageKind::WindowsInstaller.asset_name(version)).as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );

        let release = parse_release(
            r#"{
              "tag_name": "v0.3.2",
              "html_url": "https://github.com/crisprintsstuff/oxypixel/releases/tag/v0.3.2",
              "body": "Adds a deb.",
              "assets": [
                {
                  "name": "oxypixel-0.3.2-linux-x86_64.deb",
                  "browser_download_url": "https://github.com/crisprintsstuff/oxypixel/releases/download/v0.3.2/oxypixel-0.3.2-linux-x86_64.deb",
                  "size": 14
                },
                {
                  "name": "oxypixel-0.3.2-linux-x86_64.tar.gz",
                  "browser_download_url": "https://github.com/crisprintsstuff/oxypixel/releases/download/v0.3.2/oxypixel-0.3.2-linux-x86_64.tar.gz",
                  "size": 12
                },
                {
                  "name": "SHA256SUMS.txt",
                  "browser_download_url": "https://github.com/crisprintsstuff/oxypixel/releases/download/v0.3.2/SHA256SUMS.txt",
                  "size": 13
                }
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            asset_for(&release, PackageKind::LinuxArchive).unwrap().name,
            "oxypixel-0.3.2-linux-x86_64.tar.gz"
        );
        assert_eq!(
            asset_for(&release, PackageKind::LinuxDeb).unwrap().name,
            "oxypixel-0.3.2-linux-x86_64.deb"
        );
        assert!(release
            .assets
            .iter()
            .any(|asset| asset.name == "SHA256SUMS.txt"));

        assert_eq!(
            PackageKind::for_linux_exe(Path::new("/usr/bin/oxypixel")),
            PackageKind::LinuxDeb
        );
        assert_eq!(
            PackageKind::for_linux_exe(Path::new("/home/me/oxypixel-0.3.2-linux-x86_64/oxypixel")),
            PackageKind::LinuxArchive
        );
    }
}
