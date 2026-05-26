use crate::model::AvailableUpdate;
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/urugus/Pullbell/releases/latest";
pub const RELEASES_URL: &str = "https://github.com/urugus/Pullbell/releases";
const BUNDLE_IDENTIFIER: &str = "com.github.urugus.pullbell";
const USER_AGENT: &str = concat!("pullbell/", env!("CARGO_PKG_VERSION"));
const UPDATE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

static RELEASE_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(UPDATE_REQUEST_TIMEOUT)
        .build()
        .expect("release HTTP client configuration should be valid")
});

#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub async fn check_latest_release(current_version: &str) -> Result<Option<AvailableUpdate>> {
    let release = RELEASE_CLIENT
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .context("checking the latest Pullbell release")?
        .error_for_status()
        .context("GitHub rejected the latest release request")?
        .json::<LatestRelease>()
        .await
        .context("decoding the latest release response")?;

    let latest_version = release.tag_name.trim_start_matches('v');
    if is_newer_version(current_version, latest_version) {
        let asset_name = app_archive_asset_name(latest_version);
        let app_asset = release.assets.iter().find(|asset| asset.name == asset_name);
        let checksum_url = release
            .assets
            .iter()
            .find(|asset| asset.name == "checksums.txt")
            .map(|asset| asset.browser_download_url.clone());
        let download_url = app_asset
            .filter(|_| checksum_url.is_some())
            .map(|asset| asset.browser_download_url.clone());

        Ok(Some(AvailableUpdate {
            latest_version: latest_version.to_string(),
            release_url: release.html_url,
            download_url,
            download_name: app_asset
                .filter(|_| checksum_url.is_some())
                .map(|_| asset_name),
            checksum_url,
        }))
    } else {
        Ok(None)
    }
}

pub fn start_app_update(
    download_url: &str,
    download_name: &str,
    checksum_url: &str,
    latest_version: &str,
) -> Result<()> {
    let app_path = current_app_bundle_path()?;
    let temp_dir = std::env::temp_dir().join(format!("pullbell-update-{}", std::process::id()));
    let archive_path = temp_dir.join(download_name);
    let checksums_path = temp_dir.join("checksums.txt");
    let selected_checksum_path = temp_dir.join("selected-checksum.txt");
    let staging_dir = temp_dir.join("staging");
    let staged_app_path = staging_dir.join("Pullbell.app");
    let staged_info_plist_path = staged_app_path.join("Contents/Info.plist");
    let staged_executable_path = staged_app_path.join("Contents/MacOS/pullbell");
    let backup_path =
        app_path.with_file_name(format!(".Pullbell.app.backup.{}", std::process::id()));
    let shell_command = format!(
        "set -eu\n\
         cleanup() {{ /bin/rm -rf {}; }}\n\
         trap cleanup EXIT\n\
         /bin/rm -rf {}\n\
         /bin/mkdir -p {}\n\
         /usr/bin/curl -fL --retry 3 {} -o {}\n\
         /usr/bin/curl -fL --retry 3 {} -o {}\n\
         /usr/bin/grep -F {} {} > {}\n\
         (cd {} && /usr/bin/shasum -a 256 -c {})\n\
         /usr/bin/ditto -x -k {} {}\n\
         /usr/bin/test -x {}\n\
         /usr/bin/test \"$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' {})\" = {}\n\
         /usr/bin/test \"$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' {})\" = {}\n\
         /usr/bin/osascript -e 'tell application id \"com.github.urugus.pullbell\" to quit' >/dev/null 2>&1 || true\n\
         /bin/sleep 1\n\
         restore() {{ if [ -e {} ] || [ -L {} ]; then /bin/rm -rf {}; /bin/mv {} {}; /usr/bin/open {} >/dev/null 2>&1 || true; fi; }}\n\
         /bin/rm -rf {}\n\
         if ! /bin/mv {} {}; then /usr/bin/open {} >/dev/null 2>&1 || true; exit 1; fi\n\
         if ! /usr/bin/ditto {} {}; then restore; exit 1; fi\n\
         /usr/bin/xattr -dr com.apple.quarantine {} >/dev/null 2>&1 || true\n\
         if ! /usr/bin/open {}; then restore; exit 1; fi\n\
         /bin/rm -rf {}\n",
        shell_quote_path(&temp_dir),
        shell_quote_path(&temp_dir),
        shell_quote_path(&staging_dir),
        shell_quote(download_url),
        shell_quote_path(&archive_path),
        shell_quote(checksum_url),
        shell_quote_path(&checksums_path),
        shell_quote(&format!("  {download_name}")),
        shell_quote_path(&checksums_path),
        shell_quote_path(&selected_checksum_path),
        shell_quote_path(&temp_dir),
        shell_quote_path(&selected_checksum_path),
        shell_quote_path(&archive_path),
        shell_quote_path(&staging_dir),
        shell_quote_path(&staged_executable_path),
        shell_quote_path(&staged_info_plist_path),
        shell_quote(BUNDLE_IDENTIFIER),
        shell_quote_path(&staged_info_plist_path),
        shell_quote(latest_version),
        shell_quote_path(&backup_path),
        shell_quote_path(&backup_path),
        shell_quote_path(&app_path),
        shell_quote_path(&backup_path),
        shell_quote_path(&app_path),
        shell_quote_path(&app_path),
        shell_quote_path(&backup_path),
        shell_quote_path(&app_path),
        shell_quote_path(&backup_path),
        shell_quote_path(&app_path),
        shell_quote_path(&staged_app_path),
        shell_quote_path(&app_path),
        shell_quote_path(&app_path),
        shell_quote_path(&app_path),
        shell_quote_path(&backup_path)
    );

    Command::new("/bin/sh")
        .arg("-c")
        .arg(shell_command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("starting the app updater")?;

    Ok(())
}

fn current_app_bundle_path() -> Result<PathBuf> {
    let executable_path = std::env::current_exe().context("locating the running executable")?;
    executable_path
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("Pullbell is not running from an app bundle"))
}

fn app_archive_asset_name(version: &str) -> String {
    format!("pullbell-{version}-{}.zip", macos_target_triple())
}

fn macos_target_triple() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64-apple-darwin"
    }

    #[cfg(target_arch = "x86_64")]
    {
        "x86_64-apple-darwin"
    }
}

fn is_newer_version(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let core = value
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;

    if parts.next().is_some() {
        return None;
    }

    Some((major, minor, patch))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn shell_quote_path(value: &std::path::Path) -> String {
    shell_quote(&value.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_versions() {
        assert!(is_newer_version("0.2.0", "0.2.1"));
        assert!(is_newer_version("0.2.9", "0.3.0"));
        assert!(is_newer_version("0.9.0", "1.0.0"));
        assert!(!is_newer_version("0.2.1", "0.2.1"));
        assert!(!is_newer_version("0.3.0", "0.2.9"));
    }

    #[test]
    fn strips_release_tag_prefix_and_prerelease_suffix() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3+build.1"), Some((1, 2, 3)));
    }

    #[test]
    fn does_not_update_when_version_format_is_unknown() {
        assert!(!is_newer_version("0.2.0", "release-0.3.0"));
        assert!(!is_newer_version("not-semver", "0.3.0"));
    }

    #[test]
    fn escapes_terminal_update_command_strings() {
        assert_eq!(
            shell_quote("/opt/homebrew/bin/brew"),
            "'/opt/homebrew/bin/brew'"
        );
        assert_eq!(
            shell_quote("a 'quoted' value"),
            "'a '\\''quoted'\\'' value'"
        );
    }

    #[test]
    fn builds_platform_release_archive_name() {
        assert_eq!(
            app_archive_asset_name("1.2.3"),
            format!("pullbell-1.2.3-{}.zip", macos_target_triple())
        );
    }
}
