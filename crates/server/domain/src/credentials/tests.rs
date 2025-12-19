//! Tests for credentials module
//!
//! Following TDD: Tests written first, then implementation

use super::*;

mod secret_value_tests {
    use super::*;

    #[test]
    fn test_secret_value_new_from_string() {
        let secret = SecretValue::new("my-secret-password");
        assert_eq!(secret.expose(), b"my-secret-password");
    }

    #[test]
    fn test_secret_value_new_from_bytes() {
        let bytes = vec![1, 2, 3, 4, 5];
        let secret = SecretValue::new(bytes.clone());
        assert_eq!(secret.expose(), &bytes[..]);
    }

    #[test]
    fn test_secret_value_expose_str() {
        let secret = SecretValue::new("utf8-string");
        assert_eq!(secret.expose_str(), Some("utf8-string"));
    }

    #[test]
    fn test_secret_value_expose_str_invalid_utf8() {
        let invalid_utf8 = vec![0xFF, 0xFE];
        let secret = SecretValue::new(invalid_utf8);
        assert!(secret.expose_str().is_none());
    }

    #[test]
    fn test_secret_value_debug_does_not_expose_value() {
        let secret = SecretValue::new("super-secret");
        let debug_output = format!("{:?}", secret);

        assert!(!debug_output.contains("super-secret"));
        assert!(debug_output.contains("REDACTED"));
    }

    #[test]
    fn test_secret_value_clone() {
        let original = SecretValue::new("cloneable");
        let cloned = original.clone();

        assert_eq!(original.expose(), cloned.expose());
    }

    #[test]
    fn test_secret_value_is_empty() {
        let empty = SecretValue::new("");
        let non_empty = SecretValue::new("value");

        assert!(empty.is_empty());
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_secret_value_len() {
        let secret = SecretValue::new("12345");
        assert_eq!(secret.len(), 5);
    }
}

mod secret_tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_secret_builder() {
        let now = Utc::now();
        let secret = Secret::builder("api_key")
            .value(SecretValue::new("secret123"))
            .version(1)
            .created_at(now)
            .build();

        assert_eq!(secret.key(), "api_key");
        assert_eq!(secret.value().expose_str(), Some("secret123"));
        assert_eq!(secret.version(), 1);
        assert_eq!(secret.created_at(), now);
        assert!(secret.expires_at().is_none());
    }

    #[test]
    fn test_secret_builder_with_expiration() {
        let now = Utc::now();
        let expires = now + chrono::Duration::hours(24);

        let secret = Secret::builder("temp_token")
            .value(SecretValue::new("token"))
            .expires_at(expires)
            .build();

        assert_eq!(secret.expires_at(), Some(expires));
    }

    #[test]
    fn test_secret_builder_with_metadata() {
        let secret = Secret::builder("db_password")
            .value(SecretValue::new("pass"))
            .metadata("environment", "production")
            .metadata("owner", "team-a")
            .build();

        assert_eq!(
            secret.metadata().get("environment"),
            Some(&"production".to_string())
        );
        assert_eq!(secret.metadata().get("owner"), Some(&"team-a".to_string()));
    }

    #[test]
    fn test_secret_is_expired() {
        let now = Utc::now();
        let past = now - chrono::Duration::hours(1);
        let future = now + chrono::Duration::hours(1);

        let expired_secret = Secret::builder("expired")
            .value(SecretValue::new("value"))
            .expires_at(past)
            .build();

        let valid_secret = Secret::builder("valid")
            .value(SecretValue::new("value"))
            .expires_at(future)
            .build();

        let no_expiry_secret = Secret::builder("permanent")
            .value(SecretValue::new("value"))
            .build();

        assert!(expired_secret.is_expired());
        assert!(!valid_secret.is_expired());
        assert!(!no_expiry_secret.is_expired());
    }
}

mod secret_ref_tests {
    use super::*;

    #[test]
    fn test_secret_ref_new() {
        let secret_ref = SecretRef::new("my_secret", "vault");

        assert_eq!(secret_ref.key(), "my_secret");
        assert_eq!(secret_ref.provider(), "vault");
        assert!(secret_ref.version().is_none());
    }

    #[test]
    fn test_secret_ref_with_version() {
        let secret_ref = SecretRef::new("my_secret", "postgres").with_version(5);

        assert_eq!(secret_ref.key(), "my_secret");
        assert_eq!(secret_ref.provider(), "postgres");
        assert_eq!(secret_ref.version(), Some(5));
    }

    #[test]
    fn test_secret_ref_debug_safe() {
        let secret_ref = SecretRef::new("password", "vault");
        let debug_output = format!("{:?}", secret_ref);

        // SecretRef can show the key name (it's just a reference, not the value)
        assert!(debug_output.contains("password"));
        assert!(debug_output.contains("vault"));
    }
}

mod credential_error_tests {
    use super::*;

    #[test]
    fn test_credential_error_not_found() {
        let error = CredentialError::not_found("missing_key");

        assert!(matches!(error, CredentialError::NotFound { .. }));
        assert!(error.to_string().contains("missing_key"));
    }

    #[test]
    fn test_credential_error_access_denied() {
        let error = CredentialError::access_denied("restricted_key");

        assert!(matches!(error, CredentialError::AccessDenied { .. }));
        assert!(error.to_string().contains("restricted_key"));
    }

    #[test]
    fn test_credential_error_expired() {
        let error = CredentialError::expired("old_token");

        assert!(matches!(error, CredentialError::Expired { .. }));
        assert!(error.to_string().contains("old_token"));
    }

    #[test]
    fn test_credential_error_provider_unavailable() {
        let error = CredentialError::provider_unavailable("vault");

        assert!(matches!(error, CredentialError::ProviderUnavailable { .. }));
        assert!(error.to_string().contains("vault"));
    }

    #[test]
    fn test_credential_error_is_retryable() {
        let not_found = CredentialError::not_found("key");
        let connection_error = CredentialError::connection("timeout");
        let provider_unavailable = CredentialError::provider_unavailable("vault");

        assert!(!not_found.is_retryable());
        assert!(connection_error.is_retryable());
        assert!(provider_unavailable.is_retryable());
    }
}

mod audit_action_tests {
    use super::*;

    #[test]
    fn test_audit_action_display() {
        assert_eq!(AuditAction::Read.to_string(), "read");
        assert_eq!(AuditAction::Write.to_string(), "write");
        assert_eq!(AuditAction::Delete.to_string(), "delete");
        assert_eq!(AuditAction::Rotate.to_string(), "rotate");
        assert_eq!(AuditAction::List.to_string(), "list");
    }
}
