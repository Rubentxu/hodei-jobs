//! AES-256-GCM Encryption Module
//!
//! Provides secure encryption/decryption for secrets stored in PostgreSQL.
//!
//! # Security Features
//!
//! - AES-256-GCM authenticated encryption
//! - Unique random nonce per encryption operation
//! - Key material zeroed on drop
//! - Safe Debug implementation (never exposes keys)
//!
//! # Example
//!
//! ```ignore
//! let key = EncryptionKey::generate();
//! let plaintext = b"my secret data";
//! let encrypted = key.encrypt(plaintext)?;
//! let decrypted = key.decrypt(&encrypted)?;
//! assert_eq!(plaintext, &decrypted[..]);
//! ```

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use std::fmt;
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Size of AES-256 key in bytes
const KEY_SIZE: usize = 32;

/// Size of GCM nonce in bytes (96 bits)
const NONCE_SIZE: usize = 12;

/// Errors that can occur during encryption operations
#[derive(Debug, Error)]
pub enum EncryptionError {
    /// The provided key has an invalid length
    #[error("Invalid key length: expected {expected} bytes, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    /// The ciphertext is too short to contain a valid nonce
    #[error("Ciphertext too short: minimum {minimum} bytes required, got {actual}")]
    CiphertextTooShort { minimum: usize, actual: usize },

    /// Encryption operation failed
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    /// Decryption operation failed (invalid ciphertext or wrong key)
    #[error("Decryption failed: authentication tag verification failed")]
    DecryptionFailed,

    /// Invalid hex encoding
    #[error("Invalid hex encoding: {0}")]
    InvalidHex(String),
}

/// An AES-256-GCM encryption key with secure memory handling
///
/// # Security Features
///
/// - Key material is zeroed on drop
/// - Debug implementation never exposes key bytes
/// - Provides high-level encrypt/decrypt operations
///
/// # Example
///
/// ```ignore
/// // Generate a new random key
/// let key = EncryptionKey::generate();
///
/// // Or create from existing bytes
/// let key = EncryptionKey::from_bytes(&key_bytes)?;
///
/// // Encrypt data
/// let ciphertext = key.encrypt(b"secret")?;
///
/// // Decrypt data
/// let plaintext = key.decrypt(&ciphertext)?;
/// ```
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct EncryptionKey {
    key: [u8; KEY_SIZE],
}

impl EncryptionKey {
    /// Generates a new random encryption key using OS random number generator
    ///
    /// # Security
    ///
    /// Uses `OsRng` which provides cryptographically secure random bytes.
    pub fn generate() -> Self {
        let mut key = [0u8; KEY_SIZE];
        OsRng.fill_bytes(&mut key);
        Self { key }
    }

    /// Creates an encryption key from raw bytes
    ///
    /// # Errors
    ///
    /// Returns `EncryptionError::InvalidKeyLength` if the bytes are not exactly 32 bytes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let bytes = [0u8; 32]; // Your key bytes
    /// let key = EncryptionKey::from_bytes(&bytes)?;
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EncryptionError> {
        if bytes.len() != KEY_SIZE {
            return Err(EncryptionError::InvalidKeyLength {
                expected: KEY_SIZE,
                actual: bytes.len(),
            });
        }

        let mut key = [0u8; KEY_SIZE];
        key.copy_from_slice(bytes);
        Ok(Self { key })
    }

    /// Creates an encryption key from a hex-encoded string
    ///
    /// # Errors
    ///
    /// Returns an error if the hex string is invalid or has wrong length.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let key = EncryptionKey::from_hex("0123456789abcdef...")?;
    /// ```
    pub fn from_hex(hex_str: &str) -> Result<Self, EncryptionError> {
        let bytes = hex::decode(hex_str).map_err(|e| EncryptionError::InvalidHex(e.to_string()))?;
        Self::from_bytes(&bytes)
    }

    /// Exports the key as a hex-encoded string
    ///
    /// # Security Warning
    ///
    /// Use this method carefully. The returned string contains the raw key material.
    pub fn to_hex(&self) -> String {
        hex::encode(self.key)
    }

    /// Encrypts plaintext using AES-256-GCM
    ///
    /// # Output Format
    ///
    /// The output is formatted as: `nonce (12 bytes) || ciphertext || tag (16 bytes)`
    ///
    /// # Security
    ///
    /// A new random nonce is generated for each encryption operation.
    ///
    /// # Errors
    ///
    /// Returns `EncryptionError::EncryptionFailed` if the encryption operation fails.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend(ciphertext);

        Ok(result)
    }

    /// Decrypts ciphertext that was encrypted with this key
    ///
    /// # Input Format
    ///
    /// Expects the format: `nonce (12 bytes) || ciphertext || tag (16 bytes)`
    ///
    /// # Errors
    ///
    /// - `EncryptionError::CiphertextTooShort` if the input is too short
    /// - `EncryptionError::DecryptionFailed` if the ciphertext is invalid or wrong key
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        // Minimum size: nonce (12) + tag (16) = 28 bytes
        const MIN_SIZE: usize = NONCE_SIZE + 16;

        if ciphertext.len() < MIN_SIZE {
            return Err(EncryptionError::CiphertextTooShort {
                minimum: MIN_SIZE,
                actual: ciphertext.len(),
            });
        }

        let cipher =
            Aes256Gcm::new_from_slice(&self.key).map_err(|_| EncryptionError::DecryptionFailed)?;

        // Extract nonce and ciphertext
        let nonce = Nonce::from_slice(&ciphertext[..NONCE_SIZE]);
        let encrypted_data = &ciphertext[NONCE_SIZE..];

        // Decrypt
        cipher
            .decrypt(nonce, encrypted_data)
            .map_err(|_| EncryptionError::DecryptionFailed)
    }
}

impl fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EncryptionKey([REDACTED, {} bytes])", KEY_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_creates_random_key() {
        let key1 = EncryptionKey::generate();
        let key2 = EncryptionKey::generate();

        // Keys should be different (extremely high probability)
        assert_ne!(key1.key, key2.key);
    }

    #[test]
    fn test_from_bytes_valid() {
        let bytes = [0x42u8; KEY_SIZE];
        let key = EncryptionKey::from_bytes(&bytes).unwrap();
        assert_eq!(key.key, bytes);
    }

    #[test]
    fn test_from_bytes_invalid_length_short() {
        let bytes = [0u8; 16];
        let result = EncryptionKey::from_bytes(&bytes);

        match result {
            Err(EncryptionError::InvalidKeyLength { expected, actual }) => {
                assert_eq!(expected, 32);
                assert_eq!(actual, 16);
            }
            _ => panic!("Expected InvalidKeyLength error"),
        }
    }

    #[test]
    fn test_from_bytes_invalid_length_long() {
        let bytes = [0u8; 64];
        let result = EncryptionKey::from_bytes(&bytes);

        match result {
            Err(EncryptionError::InvalidKeyLength { expected, actual }) => {
                assert_eq!(expected, 32);
                assert_eq!(actual, 64);
            }
            _ => panic!("Expected InvalidKeyLength error"),
        }
    }

    #[test]
    fn test_from_hex_valid() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let key = EncryptionKey::from_hex(hex).unwrap();

        let expected_bytes = hex::decode(hex).unwrap();
        assert_eq!(key.key, expected_bytes.as_slice());
    }

    #[test]
    fn test_from_hex_invalid_characters() {
        let hex = "not-valid-hex!0123456789abcdef0123456789abcdef0123456789";
        let result = EncryptionKey::from_hex(hex);

        assert!(matches!(result, Err(EncryptionError::InvalidHex(_))));
    }

    #[test]
    fn test_from_hex_wrong_length() {
        let hex = "0123456789abcdef"; // Only 8 bytes
        let result = EncryptionKey::from_hex(hex);

        assert!(matches!(
            result,
            Err(EncryptionError::InvalidKeyLength { .. })
        ));
    }

    #[test]
    fn test_to_hex_roundtrip() {
        let key = EncryptionKey::generate();
        let hex = key.to_hex();
        let restored = EncryptionKey::from_hex(&hex).unwrap();

        assert_eq!(key.key, restored.key);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = EncryptionKey::generate();
        let plaintext = b"Hello, World! This is a secret message.";

        let ciphertext = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&ciphertext).unwrap();

        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let key = EncryptionKey::generate();
        let plaintext = b"same message";

        let ciphertext1 = key.encrypt(plaintext).unwrap();
        let ciphertext2 = key.encrypt(plaintext).unwrap();

        // Same plaintext should produce different ciphertext (due to random nonce)
        assert_ne!(ciphertext1, ciphertext2);

        // But both should decrypt to the same plaintext
        assert_eq!(key.decrypt(&ciphertext1).unwrap(), plaintext);
        assert_eq!(key.decrypt(&ciphertext2).unwrap(), plaintext);
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let key = EncryptionKey::generate();
        let plaintext = b"";

        let ciphertext = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_large_plaintext() {
        let key = EncryptionKey::generate();
        let plaintext: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let ciphertext = key.encrypt(&plaintext).unwrap();
        let decrypted = key.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key1 = EncryptionKey::generate();
        let key2 = EncryptionKey::generate();

        let plaintext = b"secret";
        let ciphertext = key1.encrypt(plaintext).unwrap();

        let result = key2.decrypt(&ciphertext);
        assert!(matches!(result, Err(EncryptionError::DecryptionFailed)));
    }

    #[test]
    fn test_decrypt_corrupted_ciphertext_fails() {
        let key = EncryptionKey::generate();
        let plaintext = b"secret";

        let mut ciphertext = key.encrypt(plaintext).unwrap();
        // Corrupt the ciphertext
        if let Some(byte) = ciphertext.last_mut() {
            *byte ^= 0xFF;
        }

        let result = key.decrypt(&ciphertext);
        assert!(matches!(result, Err(EncryptionError::DecryptionFailed)));
    }

    #[test]
    fn test_decrypt_truncated_ciphertext_fails() {
        let key = EncryptionKey::generate();
        let plaintext = b"secret";

        let ciphertext = key.encrypt(plaintext).unwrap();
        let truncated = &ciphertext[..20]; // Less than minimum size (28)

        let result = key.decrypt(truncated);
        assert!(matches!(
            result,
            Err(EncryptionError::CiphertextTooShort { .. })
        ));
    }

    #[test]
    fn test_decrypt_too_short_input() {
        let key = EncryptionKey::generate();
        let short_input = [0u8; 10];

        let result = key.decrypt(&short_input);
        match result {
            Err(EncryptionError::CiphertextTooShort { minimum, actual }) => {
                assert_eq!(minimum, 28);
                assert_eq!(actual, 10);
            }
            _ => panic!("Expected CiphertextTooShort error"),
        }
    }

    #[test]
    fn test_ciphertext_format() {
        let key = EncryptionKey::generate();
        let plaintext = b"test";

        let ciphertext = key.encrypt(plaintext).unwrap();

        // Ciphertext should be: nonce (12) + encrypted data (4) + tag (16) = 32 bytes minimum
        assert!(ciphertext.len() >= NONCE_SIZE + 16);
        // For 4-byte plaintext: 12 + 4 + 16 = 32 bytes
        assert_eq!(ciphertext.len(), 32);
    }

    #[test]
    fn test_debug_does_not_expose_key() {
        let key = EncryptionKey::generate();
        let debug_output = format!("{:?}", key);

        assert!(debug_output.contains("REDACTED"));
        assert!(debug_output.contains("32 bytes"));
        // Should not contain actual key bytes
        assert!(!debug_output.contains(&key.to_hex()));
    }

    #[test]
    fn test_encryption_error_display() {
        let err = EncryptionError::InvalidKeyLength {
            expected: 32,
            actual: 16,
        };
        assert!(err.to_string().contains("32"));
        assert!(err.to_string().contains("16"));

        let err = EncryptionError::CiphertextTooShort {
            minimum: 28,
            actual: 10,
        };
        assert!(err.to_string().contains("28"));
        assert!(err.to_string().contains("10"));

        let err = EncryptionError::DecryptionFailed;
        assert!(err.to_string().contains("authentication tag"));
    }

    #[test]
    fn test_clone_creates_independent_copy() {
        let key1 = EncryptionKey::generate();
        let key2 = key1.clone();

        // Both should work independently
        let plaintext = b"test";
        let ciphertext = key1.encrypt(plaintext).unwrap();
        let decrypted = key2.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }
}
