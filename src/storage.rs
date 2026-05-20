use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "pullbell";
const ACCOUNT: &str = "github-oauth-token";

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

fn entry() -> Entry {
    Entry::new(SERVICE, ACCOUNT).expect("static keychain service and account are valid")
}
