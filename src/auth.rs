use anyhow::{Context, Result, anyhow, bail};
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, Instant};

pub const DEFAULT_SCOPES: &str = "repo read:org notifications";

#[derive(Debug, Clone)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub struct AccessToken {
    pub token: String,
    pub scope: String,
    pub token_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    Pending,
    SlowDown,
    AccessDenied,
    Expired,
}

#[derive(Clone)]
pub struct OAuthDeviceClient {
    client_id: String,
    http: Client,
}

impl OAuthDeviceClient {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            http: Client::new(),
        }
    }

    pub async fn request_device_code(&self) -> Result<DeviceCode> {
        #[derive(Deserialize)]
        struct Response {
            device_code: String,
            user_code: String,
            verification_uri: String,
            expires_in: u64,
            interval: Option<u64>,
        }

        let response = self
            .http
            .post("https://github.com/login/device/code")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", DEFAULT_SCOPES),
            ])
            .send()
            .await
            .context("requesting GitHub device code")?
            .error_for_status()
            .context("GitHub rejected the device code request")?
            .json::<Response>()
            .await
            .context("decoding GitHub device code response")?;

        Ok(DeviceCode {
            device_code: response.device_code,
            user_code: response.user_code,
            verification_uri: response.verification_uri,
            expires_in: response.expires_in,
            interval: response.interval.unwrap_or(5),
        })
    }

    pub async fn poll_once(&self, device_code: &str) -> Result<Result<AccessToken, PollOutcome>> {
        #[derive(Deserialize)]
        struct Response {
            access_token: Option<String>,
            token_type: Option<String>,
            scope: Option<String>,
            error: Option<String>,
            error_description: Option<String>,
        }

        let response = self
            .http
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .context("polling GitHub OAuth device flow")?
            .error_for_status()
            .context("GitHub rejected the OAuth polling request")?
            .json::<Response>()
            .await
            .context("decoding GitHub OAuth polling response")?;

        if let Some(token) = response.access_token {
            return Ok(Ok(AccessToken {
                token,
                scope: response.scope.unwrap_or_default(),
                token_type: response.token_type.unwrap_or_else(|| "bearer".to_string()),
            }));
        }

        let outcome = match response.error.as_deref() {
            Some("authorization_pending") => PollOutcome::Pending,
            Some("slow_down") => PollOutcome::SlowDown,
            Some("access_denied") => PollOutcome::AccessDenied,
            Some("expired_token") => PollOutcome::Expired,
            Some(other) => {
                let detail = response.error_description.unwrap_or_default();
                bail!("GitHub OAuth error: {other} {detail}");
            }
            None => {
                return Err(anyhow!(
                    "GitHub OAuth response did not include a token or error"
                ));
            }
        };

        Ok(Err(outcome))
    }

    pub async fn wait_for_token(&self, code: &DeviceCode) -> Result<AccessToken> {
        let deadline = Instant::now() + Duration::from_secs(code.expires_in);
        let mut interval = Duration::from_secs(code.interval);

        loop {
            if Instant::now() >= deadline {
                bail!("GitHub OAuth device code expired");
            }

            tokio::time::sleep(interval).await;
            match self.poll_once(&code.device_code).await? {
                Ok(token) => return Ok(token),
                Err(PollOutcome::Pending) => {}
                Err(PollOutcome::SlowDown) => interval += Duration::from_secs(5),
                Err(PollOutcome::AccessDenied) => bail!("GitHub OAuth authorization was denied"),
                Err(PollOutcome::Expired) => bail!("GitHub OAuth device code expired"),
            }
        }
    }
}
