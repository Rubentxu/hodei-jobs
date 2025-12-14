use crate::shared_kernel::{DomainError, Result, WorkerId};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OtpToken(pub Uuid);

impl OtpToken {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OtpToken {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OtpToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for OtpToken {
    type Err = DomainError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let id = Uuid::parse_str(s).map_err(|_| DomainError::InvalidOtpToken {
            message: "OTP token must be a UUID".to_string(),
        })?;
        Ok(Self(id))
    }
}

#[async_trait]
pub trait WorkerBootstrapTokenStore: Send + Sync {
    async fn issue(&self, worker_id: &WorkerId, ttl: Duration) -> Result<OtpToken>;

    async fn consume(&self, token: &OtpToken, worker_id: &WorkerId) -> Result<()>;

    async fn cleanup_expired(&self) -> Result<u64>;
}
