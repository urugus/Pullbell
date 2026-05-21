use crate::model::AvailableUpdate;
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/urugus/Pullbell/releases/latest";
pub const RELEASES_URL: &str = "https://github.com/urugus/Pullbell/releases";
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

pub fn is_homebrew_cask_installed() -> bool {
    brew_candidates().iter().any(|brew| {
        Command::new(brew)
            .args(["list", "--cask", "pullbell"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

pub fn start_homebrew_update() -> Result<()> {
    let brew = brew_candidates()
        .into_iter()
        .find(|candidate| {
            Command::new(candidate)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        })
        .ok_or_else(|| anyhow!("Homebrew was not found"))?;

    let shell_command = format!(
        "printf 'Updating Pullbell with Homebrew...\\n'; \
         osascript -e 'tell application id \"com.github.urugus.pullbell\" to quit' >/dev/null 2>&1; \
         {} update; \
         {} upgrade --cask pullbell; \
         open -a Pullbell",
        shell_quote(&brew),
        shell_quote(&brew)
    );
    let apple_script = format!(
        "tell application \"Terminal\" to do script \"{}\"",
        apple_script_string(&shell_command)
    );

    Command::new("osascript")
        .arg("-e")
        .arg("tell application \"Terminal\" to activate")
        .arg("-e")
        .arg(apple_script)
        .spawn()
        .context("starting the Homebrew update in Terminal")?;

    Ok(())
}

fn brew_candidates() -> Vec<String> {
    vec![
        "/opt/homebrew/bin/brew".to_string(),
        "/usr/local/bin/brew".to_string(),
        "brew".to_string(),
    ]
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

fn apple_script_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
            apple_script_string("a \"quoted\" value"),
            "a \\\"quoted\\\" value"
        );
    }
}
