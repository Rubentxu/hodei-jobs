use hodei_jobs::{CommandSpec, WorkerMessage, command_spec::CommandType as ProtoCommandType};
use hodei_worker_infrastructure::{InjectionStrategy, JobExecutor};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

// Integration tests for secret injection strategies
// These tests verify the InjectionStrategy enum and basic configuration

#[tokio::test]
async fn test_tmpfs_strategy_is_default_for_production_policy() {
    // Verify that production policy uses TmpfsFile as default
    let store = hodei_worker_infrastructure::secret_policy::RuntimeSecretStore::production();
    let policy = store.policy();

    assert_eq!(policy.default_strategy, InjectionStrategy::TmpfsFile);
    assert_eq!(policy.minimum_strategy, InjectionStrategy::Stdin);
}

#[tokio::test]
async fn test_stdin_strategy_is_allowed_in_production() {
    let store = hodei_worker_infrastructure::secret_policy::RuntimeSecretStore::production();
    let policy = store.policy();

    // Should allow Stdin (security level 1)
    assert!(policy.validate_strategy(InjectionStrategy::Stdin).is_ok());
}

#[tokio::test]
async fn test_tmpfs_strategy_is_allowed_in_production() {
    let store = hodei_worker_infrastructure::secret_policy::RuntimeSecretStore::production();
    let policy = store.policy();

    // Should allow TmpfsFile (security level 2)
    assert!(
        policy
            .validate_strategy(InjectionStrategy::TmpfsFile)
            .is_ok()
    );
}

#[tokio::test]
async fn test_development_policy_uses_tmpfs_by_default() {
    let store = hodei_worker_infrastructure::secret_policy::RuntimeSecretStore::development();
    let policy = store.policy();

    assert_eq!(policy.default_strategy, InjectionStrategy::TmpfsFile);
}

#[tokio::test]
async fn test_audit_logging_disabled_in_development() {
    let store = hodei_worker_infrastructure::secret_policy::RuntimeSecretStore::development();
    let policy = store.policy();

    assert!(!policy.should_audit());
}

#[tokio::test]
async fn test_audit_logging_enabled_in_production() {
    let store = hodei_worker_infrastructure::secret_policy::RuntimeSecretStore::production();
    let policy = store.policy();

    assert!(policy.should_audit());
}

#[tokio::test]
async fn test_stdin_injection_strategy_enum_exists() {
    // Verify Stdin variant exists and can be used
    let strategy = InjectionStrategy::Stdin;
    assert!(!format!("{:?}", strategy).is_empty());
}

#[tokio::test]
async fn test_tmpfs_file_injection_strategy_enum_exists() {
    // Verify TmpfsFile variant exists and can be used
    let strategy = InjectionStrategy::TmpfsFile;
    assert!(!format!("{:?}", strategy).is_empty());
}
