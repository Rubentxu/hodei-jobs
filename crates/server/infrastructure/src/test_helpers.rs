// Shared test utilities for rustls initialization
// This module should be imported in all test modules that use rustls

/// Initialize rustls crypto provider for tests
/// Must be called once before any rustls operations
#[cfg(test)]
pub fn init_rustls() {
    use rustls::crypto::CryptoProvider;
    use rustls::crypto::aws_lc_rs;

    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        // Check if a provider is already installed
        if rustls::crypto::CryptoProvider::get_default().is_some() {
            return;
        }
        // Try to install aws_lc_rs as the crypto provider
        if let Err(e) = CryptoProvider::install_default(aws_lc_rs::default_provider()) {
            // Don't panic in ctor - just log the error
            eprintln!("Warning: Failed to install rustls crypto provider: {:?}", e);
        }
    });
}

/// Macro to use in test modules that need rustls
#[cfg(test)]
#[macro_export]
macro_rules! use_rustls_tests {
    () => {
        // Call initialization at module load
        fn _init_rustls() {
            $crate::test_helpers::init_rustls();
        }
    };
}
