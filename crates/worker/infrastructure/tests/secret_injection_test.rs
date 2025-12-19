use hodei_jobs::{
    CommandSpec, LogEntry, WorkerMessage, command_spec::CommandType as ProtoCommandType,
};
use hodei_worker_infrastructure::JobExecutor;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_secret_injection_valid_json() {
    let (tx, mut rx) = mpsc::channel::<WorkerMessage>(100);
    let metrics = Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new());
    let executor = JobExecutor::new(100, 250, metrics);

    let secrets_json = Some(r#"{"api_key":"secret123","db_password":"mysecret"}"#.to_string());

    let command_spec = Some(CommandSpec {
        command_type: Some(ProtoCommandType::Shell(hodei_jobs::ShellCommand {
            cmd: "bash".to_string(),
            args: vec![
                "-c".to_string(),
                "echo $SECRET_API_KEY $SECRET_DB_PASSWORD".to_string(),
            ],
        })),
    });

    let result = executor
        .execute_from_command(
            "test-job",
            command_spec,
            HashMap::new(),
            None,
            tx,
            Some(5),
            None,
            secrets_json,
        )
        .await;

    assert!(
        result.is_ok(),
        "Should execute successfully with valid secrets JSON"
    );
    let (exit_code, stdout, stderr) = result.unwrap();
    assert_eq!(exit_code, 0, "Command should succeed");
    assert!(
        stdout.contains("secret123"),
        "Should output SECRET_API_KEY value"
    );
    assert!(
        stdout.contains("mysecret"),
        "Should output SECRET_DB_PASSWORD value"
    );
}

#[tokio::test]
async fn test_secret_injection_invalid_json() {
    let (tx, _rx) = mpsc::channel::<WorkerMessage>(100);
    let metrics = Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new());
    let executor = JobExecutor::new(100, 250, metrics);

    let secrets_json = Some(r#"{"invalid": json}"#.to_string());

    let command_spec = Some(CommandSpec {
        command_type: Some(ProtoCommandType::Shell(hodei_jobs::ShellCommand {
            cmd: "echo".to_string(),
            args: vec!["test".to_string()],
        })),
    });

    let result = executor
        .execute_from_command(
            "test-job",
            command_spec,
            HashMap::new(),
            None,
            tx,
            Some(5),
            None,
            secrets_json,
        )
        .await;

    assert!(result.is_err(), "Should fail with invalid JSON");
    let error = result.unwrap_err();
    assert!(
        error.contains("Invalid secrets JSON"),
        "Should return JSON parsing error"
    );
}

#[tokio::test]
async fn test_secret_injection_no_secrets() {
    let (tx, mut rx) = mpsc::channel::<WorkerMessage>(100);
    let metrics = Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new());
    let executor = JobExecutor::new(100, 250, metrics);

    let secrets_json = None;

    let command_spec = Some(CommandSpec {
        command_type: Some(ProtoCommandType::Shell(hodei_jobs::ShellCommand {
            cmd: "echo".to_string(),
            args: vec!["test".to_string()],
        })),
    });

    let result = executor
        .execute_from_command(
            "test-job",
            command_spec,
            HashMap::new(),
            None,
            tx,
            Some(5),
            None,
            secrets_json,
        )
        .await;

    assert!(
        result.is_ok(),
        "Should execute successfully without secrets"
    );
    let (exit_code, stdout, _stderr) = result.unwrap();
    assert_eq!(exit_code, 0, "Command should succeed");
    assert_eq!(stdout.trim(), "test", "Should output expected result");
}

#[tokio::test]
async fn test_secret_injection_uppercase_prefix() {
    let (tx, _rx) = mpsc::channel::<WorkerMessage>(100);
    let metrics = Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new());
    let executor = JobExecutor::new(100, 250, metrics);

    // Test lowercase and mixed-case keys are uppercased
    let secrets_json = Some(r#"{"myKey":"value1","another_key":"value2"}"#.to_string());

    let command_spec = Some(CommandSpec {
        command_type: Some(ProtoCommandType::Shell(hodei_jobs::ShellCommand {
            cmd: "bash".to_string(),
            args: vec!["-c".to_string(), "env | grep SECRET_ | sort".to_string()],
        })),
    });

    let result = executor
        .execute_from_command(
            "test-job",
            command_spec,
            HashMap::new(),
            None,
            tx,
            Some(5),
            None,
            secrets_json,
        )
        .await;

    assert!(result.is_ok(), "Should execute successfully");
    let (exit_code, stdout, _stderr) = result.unwrap();
    assert_eq!(exit_code, 0, "Command should succeed");

    // Verify uppercase transformation
    assert!(
        stdout.contains("SECRET_MYKEY="),
        "Should uppercase lowercase keys"
    );
    assert!(
        stdout.contains("SECRET_ANOTHER_KEY="),
        "Should uppercase snake_case keys"
    );
}

#[tokio::test]
async fn test_secret_injection_with_regular_env_vars() {
    let (tx, _rx) = mpsc::channel::<WorkerMessage>(100);
    let metrics = Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new());
    let executor = JobExecutor::new(100, 250, metrics);

    let mut env_vars = HashMap::new();
    env_vars.insert("REGULAR_VAR".to_string(), "regular_value".to_string());

    let secrets_json = Some(r#"{"secret_key":"secret_value"}"#.to_string());

    let command_spec = Some(CommandSpec {
        command_type: Some(ProtoCommandType::Shell(hodei_jobs::ShellCommand {
            cmd: "bash".to_string(),
            args: vec![
                "-c".to_string(),
                "echo $REGULAR_VAR $SECRET_SECRET_KEY".to_string(),
            ],
        })),
    });

    let result = executor
        .execute_from_command(
            "test-job",
            command_spec,
            env_vars,
            None,
            tx,
            Some(5),
            None,
            secrets_json,
        )
        .await;

    assert!(
        result.is_ok(),
        "Should execute successfully with both regular and secret env vars"
    );
    let (exit_code, stdout, _stderr) = result.unwrap();
    assert_eq!(exit_code, 0, "Command should succeed");
    assert!(
        stdout.contains("regular_value"),
        "Should preserve regular env vars"
    );
    assert!(
        stdout.contains("secret_value"),
        "Should include secret env vars"
    );
}

#[tokio::test]
async fn test_secret_injection_secret_override() {
    let (tx, _rx) = mpsc::channel::<WorkerMessage>(100);
    let metrics = Arc::new(hodei_worker_infrastructure::metrics::WorkerMetrics::new());
    let executor = JobExecutor::new(100, 250, metrics);

    // Regular env var with same name as secret (without prefix)
    let mut env_vars = HashMap::new();
    env_vars.insert("API_KEY".to_string(), "regular_value".to_string());

    let secrets_json = Some(r#"{"api_key":"secret_value"}"#.to_string());

    let command_spec = Some(CommandSpec {
        command_type: Some(ProtoCommandType::Shell(hodei_jobs::ShellCommand {
            cmd: "bash".to_string(),
            args: vec![
                "-c".to_string(),
                "echo $API_KEY $SECRET_API_KEY".to_string(),
            ],
        })),
    });

    let result = executor
        .execute_from_command(
            "test-job",
            command_spec,
            env_vars,
            None,
            tx,
            Some(5),
            None,
            secrets_json,
        )
        .await;

    assert!(result.is_ok(), "Should execute successfully");
    let (exit_code, stdout, _stderr) = result.unwrap();
    assert_eq!(exit_code, 0, "Command should succeed");
    assert!(
        stdout.contains("regular_value"),
        "Should keep original API_KEY"
    );
    assert!(
        stdout.contains("secret_value"),
        "Should inject SECRET_API_KEY"
    );
    // Both should appear
    let count = stdout.matches("regular_value").count() + stdout.matches("secret_value").count();
    assert_eq!(count, 2, "Both values should appear separately");
}
