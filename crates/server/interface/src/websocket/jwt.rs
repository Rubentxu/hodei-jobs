//! JWT Token Management for WebSocket Authentication
//!
//! Provides JWT token validation for WebSocket connections.
//! Tokens are extracted from the `Authorization` header using the Bearer scheme.

use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};

/// JWT claims structure for Hodei Jobs platform
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JwtClaims {
    /// User identifier
    #[serde(rename = "sub")]
    pub subject: String,
    /// User roles/permissions
    pub roles: Vec<String>,
    /// Token expiration timestamp (Unix seconds)
    #[serde(default)]
    pub exp: Option<u64>,
    /// Token issued at timestamp (Unix seconds)
    #[serde(default)]
    pub iat: Option<u64>,
    /// Session identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Errors that can occur during JWT validation
#[derive(Debug, Error, PartialEq)]
pub enum JwtError {
    #[error("Missing Authorization header")]
    MissingHeader,

    #[error("Invalid Authorization header format")]
    InvalidHeaderFormat,

    #[error("Invalid token scheme (expected Bearer)")]
    InvalidScheme,

    #[error("Token validation failed: {0}")]
    ValidationFailed(String),

    #[error("Token expired")]
    ExpiredToken,

    #[error("Invalid token signature")]
    InvalidSignature,

    #[error("Invalid token issuer")]
    InvalidIssuer,

    #[error("Token decoding failed: {0}")]
    DecodeError(String),
}

/// JWT configuration for token validation
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// Secret key for validating tokens
    secret: Vec<u8>,
    /// Expected issuer
    issuer: Option<String>,
    /// Required algorithms
    algorithms: Vec<Algorithm>,
}

impl JwtConfig {
    /// Create a new JWT configuration
    pub fn new(secret: impl Into<Vec<u8>>, issuer: Option<String>) -> Self {
        Self {
            secret: secret.into(),
            issuer,
            algorithms: vec![Algorithm::HS256],
        }
    }

    /// Validate a JWT token and return the claims
    pub fn validate_token(&self, token: &str) -> Result<JwtClaims, JwtError> {
        debug!("Validating JWT token");

        // Decode header to get algorithm
        let header = decode_header(token)
            .map_err(|e| JwtError::DecodeError(format!("Failed to decode header: {}", e)))?;

        // Validate algorithm
        if !self.algorithms.contains(&header.alg) {
            return Err(JwtError::ValidationFailed(format!(
                "Unsupported algorithm: {:?}",
                header.alg
            )));
        }

        // Create validation config
        let mut validation = Validation::new(header.alg);

        // Set expected issuer if configured
        if let Some(ref issuer) = self.issuer {
            validation.set_issuer(&[issuer]);
        }

        // Decode and validate token
        let decoding_key = DecodingKey::from_secret(&self.secret);

        decode::<JwtClaims>(token, &decoding_key, &validation)
            .map(|token_data| {
                debug!(subject = %token_data.claims.subject, "Token validated successfully");
                token_data.claims
            })
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    warn!("Token has expired");
                    JwtError::ExpiredToken
                }
                jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                    warn!("Invalid token signature");
                    JwtError::InvalidSignature
                }
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => {
                    warn!("Invalid token issuer");
                    JwtError::InvalidIssuer
                }
                _ => {
                    let message = e.to_string();
                    warn!(error = %message, "Token validation failed");
                    JwtError::ValidationFailed(message)
                }
            })
    }
}

/// Extract JWT token from Authorization header
pub fn extract_token_from_header(auth_header: &str) -> Result<&str, JwtError> {
    // Check header format
    if !auth_header.starts_with("Bearer ") {
        warn!(header = %auth_header, "Invalid authorization scheme");
        return Err(JwtError::InvalidScheme);
    }

    // Extract token (remove "Bearer " prefix)
    let token = auth_header.trim_start_matches("Bearer ");

    // Ensure token is not empty
    if token.is_empty() {
        warn!("Empty token after Bearer prefix");
        return Err(JwtError::InvalidHeaderFormat);
    }

    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use std::time::{Duration, SystemTime};

    fn create_test_token(claims: &JwtClaims, secret: &str, expires_in_seconds: i64) -> String {
        let mut claims = claims.clone();

        // Set expiration (positive = future, negative = past)
        let exp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + expires_in_seconds;

        claims.exp = Some(if exp > 0 { exp as u64 } else { 0 });

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_extract_token_from_header_valid() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test";
        let header = format!("Bearer {}", token);

        let result = extract_token_from_header(&header);
        assert_eq!(result.unwrap(), token);
    }

    #[test]
    fn test_extract_token_from_header_missing_bearer() {
        let header = "InvalidToken";

        let result = extract_token_from_header(header);
        assert_eq!(result.unwrap_err(), JwtError::InvalidScheme);
    }

    #[test]
    fn test_extract_token_from_header_empty() {
        let header = "Bearer ";

        let result = extract_token_from_header(header);
        assert_eq!(result.unwrap_err(), JwtError::InvalidHeaderFormat);
    }

    #[test]
    fn test_validate_token_valid() {
        let secret = "test-secret";
        let claims = JwtClaims {
            subject: "user-123".to_string(),
            roles: vec!["admin".to_string()],
            exp: None,
            iat: None,
            session_id: None,
        };

        let token = create_test_token(&claims, secret, 3600);
        let config = JwtConfig::new(secret, None);

        let result = config.validate_token(&token);
        assert!(result.is_ok());

        let validated = result.unwrap();
        assert_eq!(validated.subject, "user-123");
        assert_eq!(validated.roles, vec!["admin"]);
    }

    #[test]
    fn test_validate_token_expired() {
        let secret = "test-secret";
        let claims = JwtClaims {
            subject: "user-123".to_string(),
            roles: vec![],
            exp: None,
            iat: None,
            session_id: None,
        };

        // Create token that expired 1 hour ago
        let token = create_test_token(&claims, secret, -3600);
        let config = JwtConfig::new(secret, None);

        let result = config.validate_token(&token);
        assert_eq!(result.unwrap_err(), JwtError::ExpiredToken);
    }

    #[test]
    fn test_validate_token_invalid_signature() {
        let claims = JwtClaims {
            subject: "user-123".to_string(),
            roles: vec![],
            exp: None,
            iat: None,
            session_id: None,
        };

        let token = create_test_token(&claims, "secret-1", 3600);
        let config = JwtConfig::new("secret-2", None);

        let result = config.validate_token(&token);
        assert_eq!(result.unwrap_err(), JwtError::InvalidSignature);
    }

    #[test]
    fn test_jwt_config_with_issuer() {
        let secret = "test-secret";
        let issuer = "hodei-platform";
        let claims = JwtClaims {
            subject: "user-123".to_string(),
            roles: vec![],
            exp: None,
            iat: None,
            session_id: None,
        };

        let token = create_test_token(&claims, secret, 3600);
        let config = JwtConfig::new(secret, Some(issuer.to_string()));

        let result = config.validate_token(&token);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jwt_config_wrong_issuer() {
        let secret = "test-secret";
        let claims = JwtClaims {
            subject: "user-123".to_string(),
            roles: vec![],
            exp: None,
            iat: None,
            session_id: None,
        };

        let token = create_test_token(&claims, secret, 3600);
        let config = JwtConfig::new(secret, Some("wrong-issuer".to_string()));

        let result = config.validate_token(&token);
        assert_eq!(result.unwrap_err(), JwtError::InvalidIssuer);
    }
}
