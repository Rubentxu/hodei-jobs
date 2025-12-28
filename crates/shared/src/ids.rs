use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Identificador único para jobs
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identificador único para providers
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub Uuid);

impl ProviderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ProviderId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identificador único para workers
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerId(pub Uuid);

impl WorkerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(s: &str) -> Option<Self> {
        Uuid::parse_str(s).ok().map(Self)
    }
}

impl Default for WorkerId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for WorkerId {
    fn from(s: String) -> Self {
        Self::from_string(&s).unwrap_or_default()
    }
}

/// Identificador de correlación para tracking de operaciones
#[derive(Debug, Clone, PartialEq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(pub Uuid);

impl CorrelationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Generates a new correlation ID (alias for new())
    pub fn generate() -> Self {
        Self::new()
    }

    /// Creates a CorrelationId from a string
    pub fn from_string(s: &str) -> Option<Self> {
        Uuid::parse_str(s).ok().map(Self)
    }

    /// Creates a CorrelationId from a String
    pub fn from_string_owned(s: String) -> Option<Self> {
        Self::from_string(&s)
    }

    /// Returns the correlation ID as a string slice
    ///
    /// Note: This allocates a new String. Prefer using `to_string_value()` if you need
    /// to use the value multiple times or in contexts where ownership is required.
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }

    /// Returns the correlation ID as an owned String
    pub fn to_string_value(&self) -> String {
        self.0.to_string()
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CorrelationId {
    fn from(s: String) -> Self {
        Self::from_string(&s).unwrap_or_default()
    }
}
