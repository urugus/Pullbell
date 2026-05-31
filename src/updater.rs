use crate::model::AvailableUpdate;
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/urugus/Pullbell/releases/latest";
pub const RELEASES_URL: &str = "https://github.com/urugus/Pullbell/releases";
const HOMEBREW_CASK: &str = "pullbell";
const UPDATE_LOG_FILE_NAME: &str = "homebrew-update.log";
const BREW_CANDIDATES: [&str; 2] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];
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
        Ok(Some(AvailableUpdate {
            latest_version: latest_version.to_string(),
            release_url: release.html_url,
        }))
    } else {
        Ok(None)
    }
}

pub fn start_app_update(latest_version: &str) -> Result<Child> {
    let app_path = current_app_bundle_path()?;
    let app_dir = app_path
        .parent()
        .context("locating the Pullbell app directory")?;
    let brew_path = find_brew_executable()?;

    verify_homebrew_cask_installed(&brew_path)?;

    let log_path = homebrew_update_log_path()?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating update log directory {}", parent.display()))?;
    }
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("opening update log {}", log_path.display()))?;
    let stderr_log = log_file
        .try_clone()
        .with_context(|| format!("opening update log {}", log_path.display()))?;

    Command::new("/bin/zsh")
        .arg("-lc")
        .arg(HOMEBREW_UPDATE_SCRIPT)
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("PULLBELL_APPDIR", app_dir)
        .env("PULLBELL_APP_PATH", &app_path)
        .env("PULLBELL_BREW_PATH", brew_path)
        .env("PULLBELL_EXPECTED_VERSION", latest_version)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(stderr_log))
        .spawn()
        .context("starting the Homebrew app updater")
}

pub fn homebrew_update_log_path() -> Result<PathBuf> {
    let data_dir = dirs::data_dir().context("locating Pullbell application support directory")?;
    Ok(data_dir.join("pullbell").join(UPDATE_LOG_FILE_NAME))
}

pub fn update_failed_message(exit_code: Option<i32>, latest_version: &str) -> String {
    let log_path = homebrew_update_log_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "the Pullbell update log".to_string());

    if exit_code == Some(20) {
        format!(
            "Homebrew did not install Pullbell {latest_version}. Check that the Homebrew cask has been updated. See {log_path}."
        )
    } else {
        format!("Homebrew update failed. See {log_path}.")
    }
}

fn current_app_bundle_path() -> Result<PathBuf> {
    let executable_path = std::env::current_exe().context("locating the running executable")?;
    executable_path
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("Pullbell is not running from an app bundle"))
}

fn find_brew_executable() -> Result<PathBuf> {
    BREW_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .ok_or_else(|| {
            anyhow!(
                "Homebrew was not found. Install Pullbell with Homebrew cask first: brew install --cask urugus/tap/pullbell"
            )
        })
}

fn verify_homebrew_cask_installed(brew_path: &std::path::Path) -> Result<()> {
    let output = Command::new(brew_path)
        .args(["list", "--cask", HOMEBREW_CASK])
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .output()
        .context("checking the Pullbell Homebrew cask installation")?;

    if output.status.success() {
        return Ok(());
    }

    Err(anyhow!(
        "{}",
        command_output_message(&output)
            .unwrap_or_else(|| "Pullbell Homebrew cask is not installed. Install it first with: brew install --cask urugus/tap/pullbell".to_string())
    ))
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

fn command_output_message(output: &std::process::Output) -> Option<String> {
    [output.stderr.as_slice(), output.stdout.as_slice()]
        .into_iter()
        .filter_map(|bytes| std::str::from_utf8(bytes).ok())
        .flat_map(str::lines)
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

const HOMEBREW_UPDATE_SCRIPT: &str = r#"
set -e
echo "==== Pullbell Homebrew update started $(date -u +%Y-%m-%dT%H:%M:%SZ) ===="
trap 'exit_code=$?; if [ "$exit_code" -ne 0 ]; then echo "__PULLBELL_HOMEBREW_UPDATE_FAILED__ status=$exit_code"; open "$PULLBELL_APP_PATH" >/dev/null 2>&1 || true; fi' EXIT
mkdir -p "$PULLBELL_APPDIR"
"$PULLBELL_BREW_PATH" update
"$PULLBELL_BREW_PATH" upgrade --cask --appdir "$PULLBELL_APPDIR" pullbell
installed_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PULLBELL_APP_PATH/Contents/Info.plist" 2>/dev/null || true)"
echo "Installed Pullbell version: ${installed_version:-unknown}"
if [ "$installed_version" != "$PULLBELL_EXPECTED_VERSION" ]; then
  echo "__PULLBELL_HOMEBREW_UPDATE_VERSION_MISMATCH__ expected=$PULLBELL_EXPECTED_VERSION actual=${installed_version:-unknown}"
  exit 20
fi
echo "__PULLBELL_HOMEBREW_UPDATE_SUCCESS__ $(date -u +%Y-%m-%dT%H:%M:%SZ)"
osascript -e 'display dialog "Pullbell was updated. Restart now to use the new version." buttons {"Restart"} default button "Restart" with title "Pullbell update"' >/dev/null 2>&1 || true
osascript -e 'tell application id "com.github.urugus.pullbell" to quit' >/dev/null 2>&1 || true
sleep 1
if ! open -n "$PULLBELL_APP_PATH"; then
  echo "__PULLBELL_HOMEBREW_REOPEN_FAILED__"
fi
"#;

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
    fn reports_homebrew_version_mismatch_with_expected_version() {
        assert!(
            update_failed_message(Some(20), "1.2.3")
                .contains("Homebrew did not install Pullbell 1.2.3")
        );
    }

    #[test]
    fn homebrew_update_script_uses_cask_and_restart_commands() {
        assert!(HOMEBREW_UPDATE_SCRIPT.contains("Homebrew update started"));
        assert!(HOMEBREW_UPDATE_SCRIPT.contains("upgrade --cask --appdir"));
        assert!(HOMEBREW_UPDATE_SCRIPT.contains("pullbell"));
        assert!(HOMEBREW_UPDATE_SCRIPT.contains("com.github.urugus.pullbell"));
    }
}
