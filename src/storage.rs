use anyhow::{Context, Result};
use keyring::Entry;
use std::fs;
use std::path::PathBuf;

use crate::model::{AppSettings, LocalDonePrs};

const SERVICE: &str = "pullbell";
const ACCOUNT: &str = "github-oauth-token";
const DONE_PRS_FILE: &str = "done-prs.json";
const SETTINGS_FILE: &str = "settings.json";

pub fn load_token() -> Result<Option<String>> {
    match entry().get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("reading GitHub OAuth token from macOS Keychain"),
    }
}

pub fn save_token(token: &str) -> Result<()> {
    entry()
        .set_password(token)
        .context("saving GitHub OAuth token to macOS Keychain")
}

pub fn delete_token() -> Result<()> {
    match entry().delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error).context("deleting GitHub OAuth token from macOS Keychain"),
    }
}

pub fn load_done_prs() -> Result<LocalDonePrs> {
    let path = done_prs_path()?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .with_context(|| format!("decoding local done pull requests from {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LocalDonePrs::default()),
        Err(error) => Err(error)
            .with_context(|| format!("reading local done pull requests from {}", path.display())),
    }
}

pub fn save_done_prs(done_prs: &LocalDonePrs) -> Result<()> {
    let path = done_prs_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "creating local done pull requests directory {}",
                parent.display()
            )
        })?;
    }
    let contents =
        serde_json::to_string_pretty(done_prs).context("encoding local done pull requests")?;
    fs::write(&path, contents)
        .with_context(|| format!("saving local done pull requests to {}", path.display()))
}

pub fn delete_done_prs() -> Result<()> {
    let path = done_prs_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("deleting local done pull requests at {}", path.display())),
    }
}

pub fn load_settings() -> Result<AppSettings> {
    let path = settings_path()?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .with_context(|| format!("decoding Pullbell settings from {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(error) => {
            Err(error).with_context(|| format!("reading Pullbell settings from {}", path.display()))
        }
    }
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("creating Pullbell settings directory {}", parent.display())
        })?;
    }
    let contents = serde_json::to_string_pretty(settings).context("encoding Pullbell settings")?;
    fs::write(&path, contents)
        .with_context(|| format!("saving Pullbell settings to {}", path.display()))
}

fn entry() -> Entry {
    Entry::new(SERVICE, ACCOUNT).expect("static keychain service and account are valid")
}

fn done_prs_path() -> Result<PathBuf> {
    let data_dir = dirs::data_dir().context("locating Pullbell application support directory")?;
    Ok(data_dir.join("pullbell").join(DONE_PRS_FILE))
}

fn settings_path() -> Result<PathBuf> {
    let data_dir = dirs::data_dir().context("locating Pullbell application support directory")?;
    Ok(data_dir.join("pullbell").join(SETTINGS_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LocalDonePr;
    use chrono::{TimeZone, Utc};

    #[test]
    fn serializes_local_done_prs() {
        let done_prs = LocalDonePrs::from([(
            "owner/repo#42".to_string(),
            LocalDonePr {
                updated_at: Some(Utc.timestamp_opt(42, 0).unwrap()),
                item: None,
            },
        )]);

        let encoded = serde_json::to_string(&done_prs).expect("encode local done prs");
        let decoded: LocalDonePrs = serde_json::from_str(&encoded).expect("decode local done prs");

        assert_eq!(decoded, done_prs);
    }

    #[test]
    fn serializes_settings() {
        let settings = AppSettings {
            muted_repositories: ["owner/repo".to_string()].into(),
            known_repositories: ["owner/repo".to_string(), "owner/other".to_string()].into(),
        };

        let encoded = serde_json::to_string(&settings).expect("encode settings");
        let decoded: AppSettings = serde_json::from_str(&encoded).expect("decode settings");

        assert_eq!(decoded, settings);
    }
}
