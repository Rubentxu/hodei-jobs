//! Error categorization for job execution errors.
//!
//! This module provides utilities to categorize raw error messages
//! from the operating system and executor into structured `JobFailureReason` types.

use hodei_shared::states::JobFailureReason;
use std::io::ErrorKind;

/// Categorizes a raw error message from the executor into a `JobFailureReason`.
///
/// This function analyzes error patterns and maps them to specific failure
/// categories, enabling structured error reporting and actionable suggestions.
///
/// # Arguments
///
/// * `error_message` - The raw error message from the executor
/// * `command` - The command that was being executed
/// * `exit_code` - The exit code from the process (if available, -1 for errors)
///
/// # Examples
///
/// ```
/// use hodei_worker_infrastructure::error_categorizer::categorize_error;
/// use hodei_shared::states::JobFailureReason;
///
/// let reason = categorize_error(
///     "No such file or directory: /app/script.sh",
///     "/app/script.sh",
///     -1
/// );
/// assert!(matches!(reason, JobFailureReason::FileNotFound { .. }));
/// ```
pub fn categorize_error(error_message: &str, command: &str, exit_code: i32) -> JobFailureReason {
    let error_lower = error_message.to_lowercase();

    // Check for common patterns in order of specificity

    // Permission denied patterns
    if error_lower.contains("permission denied")
        || error_lower.contains("access denied")
        || error_lower.contains("eacces")
    {
        // Extract path from error message if present
        let path = extract_path_from_error(error_message).unwrap_or_else(|| command.to_string());
        let operation = if error_lower.contains("execute") || error_lower.contains("x") {
            "execute"
        } else if error_lower.contains("read") || error_lower.contains("open") {
            "read"
        } else if error_lower.contains("write") || error_lower.contains("create") {
            "write"
        } else {
            "execute"
        };

        return JobFailureReason::PermissionDenied {
            path,
            operation: operation.to_string(),
        };
    }

    // File not found patterns
    if error_lower.contains("no such file")
        || error_lower.contains("cannot find")
        || error_lower.contains("does not exist")
        || error_lower.contains("enoent")
    {
        let path = extract_path_from_error(error_message).unwrap_or_else(|| command.to_string());
        return JobFailureReason::FileNotFound { path };
    }

    // Command not found patterns
    if error_lower.contains("command not found")
        || error_lower.contains("not found")
        || error_lower.contains("enoent")
    {
        // Check if it looks like a command not found (not a file)
        if !error_message.contains('/') && !error_message.contains('.') {
            return JobFailureReason::CommandNotFound {
                command: command.to_string(),
            };
        }
    }

    // Timeout patterns
    if error_lower.contains("timeout") || error_message.contains("TIMEOUT") {
        // Try to extract timeout duration
        let limit_secs = extract_timeout_secs(error_message).unwrap_or(0);
        return JobFailureReason::ExecutionTimeout { limit_secs };
    }

    // Signal received patterns
    if let Some(signal) = extract_signal(error_message) {
        return JobFailureReason::SignalReceived { signal };
    }

    // Process spawn failed patterns
    if error_lower.contains("spawn")
        || error_lower.contains("failed to spawn")
        || error_lower.contains("could not spawn")
        || error_lower.contains("exec format error")
        || error_lower.contains("no such file or directory")
    {
        return JobFailureReason::ProcessSpawnFailed {
            message: error_message.to_string(),
        };
    }

    // Non-zero exit code (application error)
    if exit_code > 0 {
        return JobFailureReason::NonZeroExitCode { exit_code };
    }

    // Infrastructure errors
    if error_lower.contains("connection")
        || error_lower.contains("network")
        || error_lower.contains("i/o error")
        || error_lower.contains("io error")
    {
        return JobFailureReason::InfrastructureError {
            message: error_message.to_string(),
        };
    }

    // Secret injection errors
    if error_lower.contains("secret")
        || error_lower.contains("environment variable")
        || error_lower.contains("env")
    {
        return JobFailureReason::SecretInjectionError {
            secret_name: extract_secret_name(error_message)
                .unwrap_or_else(|| "unknown".to_string()),
            message: error_message.to_string(),
        };
    }

    // I/O errors
    if error_lower.contains("i/o")
        || error_lower.contains("disk")
        || error_lower.contains("broken pipe")
        || error_lower.contains("eio")
    {
        let operation =
            extract_io_operation(error_message).unwrap_or_else(|| "read/write".to_string());
        let path = extract_path_from_error(error_message);
        return JobFailureReason::IoError {
            operation,
            path,
            message: error_message.to_string(),
        };
    }

    // Default to unknown
    JobFailureReason::Unknown {
        message: error_message.to_string(),
    }
}

/// Extracts a file path from an error message.
///
/// Looks for common patterns like `/path/to/file` in the error message.
fn extract_path_from_error(error_message: &str) -> Option<String> {
    // Common path patterns
    let patterns = [
        r"/[/\w.-]+",           // Unix paths
        r"[A-Za-z]:\\[/\w.-]+", // Windows paths
    ];

    for pattern in &patterns {
        if let Some(captures) = regex::Regex::new(pattern)
            .ok()
            .and_then(|re| re.captures(error_message))
        {
            if let Some(m) = captures.get(0) {
                return Some(m.as_str().to_string());
            }
        }
    }

    None
}

/// Extracts timeout duration from an error message.
fn extract_timeout_secs(error_message: &str) -> Option<u64> {
    // Look for patterns like "after 3600 seconds" or "3600s"
    let patterns = [r"after (\d+) seconds?", r"(\d+)s", r"timeout[:\s]+(\d+)"];

    for pattern in &patterns {
        if let Some(captures) = regex::Regex::new(pattern)
            .ok()
            .and_then(|re| re.captures(error_message))
        {
            if let Some(m) = captures.get(1) {
                return m.as_str().parse().ok();
            }
        }
    }

    None
}

/// Extracts a signal number from an error message.
fn extract_signal(error_message: &str) -> Option<i32> {
    // Look for signal patterns like "signal 15" or "SIGTERM"
    let patterns = [
        r"signal (\d+)",
        r"SIG(TERM|KILL|HUP|INT|QUIT|ABRT|SEGV|BUS|FPE)",
    ];

    for pattern in &patterns {
        if let Some(captures) = regex::Regex::new(pattern)
            .ok()
            .and_then(|re| re.captures(error_message))
        {
            if let Some(m) = captures.get(1) {
                let s = m.as_str();
                // Check if it's a number
                if let Ok(num) = s.parse() {
                    return Some(num);
                }
                // Map signal names to numbers
                return match s {
                    "TERM" => Some(15),
                    "KILL" => Some(9),
                    "HUP" => Some(1),
                    "INT" => Some(2),
                    "QUIT" => Some(3),
                    "ABRT" => Some(6),
                    "SEGV" => Some(11),
                    "BUS" => Some(7),
                    "FPE" => Some(8),
                    _ => None,
                };
            }
        }
    }

    None
}

/// Extracts a secret name from an error message.
fn extract_secret_name(error_message: &str) -> Option<String> {
    // Look for patterns like "secret 'API_KEY'" or "API_KEY not found"
    let patterns = [
        r"secret .*([A-Z_][A-Z0-9_]*)",
        r"([A-Z_][A-Z0-9_]*) not (found|available)",
        r"environment variable .*([A-Z_][A-Z0-9_]*)",
    ];

    for pattern in &patterns {
        if let Some(captures) = regex::Regex::new(pattern)
            .ok()
            .and_then(|re| re.captures(error_message))
        {
            if let Some(m) = captures.get(1) {
                return Some(m.as_str().to_string());
            }
        }
    }

    None
}

/// Extracts I/O operation type from an error message.
fn extract_io_operation(error_message: &str) -> Option<String> {
    let error_lower = error_message.to_lowercase();
    if error_lower.contains("read") {
        Some("read".to_string())
    } else if error_lower.contains("write") {
        Some("write".to_string())
    } else if error_lower.contains("open") {
        Some("open".to_string())
    } else if error_lower.contains("close") {
        Some("close".to_string())
    } else {
        None
    }
}

/// Converts an io::Error to a JobFailureReason.
pub fn categorize_io_error(error: &std::io::Error, context: &str) -> JobFailureReason {
    match error.kind() {
        ErrorKind::PermissionDenied => JobFailureReason::PermissionDenied {
            path: context.to_string(),
            operation: "I/O".to_string(),
        },
        ErrorKind::NotFound => JobFailureReason::FileNotFound {
            path: context.to_string(),
        },
        ErrorKind::TimedOut => JobFailureReason::ExecutionTimeout { limit_secs: 0 },
        ErrorKind::Interrupted => JobFailureReason::SignalReceived { signal: 2 }, // SIGINT
        ErrorKind::UnexpectedEof => JobFailureReason::IoError {
            operation: "read".to_string(),
            path: Some(context.to_string()),
            message: error.to_string(),
        },
        ErrorKind::ResourceBusy => JobFailureReason::IoError {
            operation: "read/write".to_string(),
            path: Some(context.to_string()),
            message: "Resource is busy".to_string(),
        },
        ErrorKind::WriteZero => JobFailureReason::IoError {
            operation: "write".to_string(),
            path: Some(context.to_string()),
            message: "Write returned zero bytes".to_string(),
        },
        ErrorKind::StorageFull => JobFailureReason::IoError {
            operation: "write".to_string(),
            path: Some(context.to_string()),
            message: "No space left on device".to_string(),
        },
        ErrorKind::NotConnected => JobFailureReason::InfrastructureError {
            message: "I/O operation not connected".to_string(),
        },
        _ => JobFailureReason::IoError {
            operation: "I/O".to_string(),
            path: Some(context.to_string()),
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_permission_denied() {
        let reason = categorize_error("Permission denied: /app/script.sh", "/app/script.sh", -1);
        assert!(matches!(reason, JobFailureReason::PermissionDenied { .. }));
    }

    #[test]
    fn test_categorize_file_not_found() {
        let reason = categorize_error(
            "No such file or directory: /data/input.csv",
            "/data/input.csv",
            -1,
        );
        assert!(matches!(reason, JobFailureReason::FileNotFound { .. }));
    }

    #[test]
    fn test_categorize_command_not_found() {
        let reason = categorize_error("Command not found: python3", "python3", -1);
        assert!(matches!(reason, JobFailureReason::CommandNotFound { .. }));
    }

    #[test]
    fn test_categorize_timeout() {
        let reason = categorize_error(
            "TIMEOUT: Job timed out after 3600 seconds",
            "long-running-command",
            -1,
        );
        assert!(matches!(
            reason,
            JobFailureReason::ExecutionTimeout { limit_secs: 3600 }
        ));
    }

    #[test]
    fn test_categorize_signal() {
        let reason = categorize_error(
            "Process received signal: SIGTERM (15)",
            "some-command",
            143, // 128 + 15
        );
        assert!(matches!(
            reason,
            JobFailureReason::SignalReceived { signal: 15 }
        ));
    }

    #[test]
    fn test_categorize_signal_kill() {
        let reason = categorize_error(
            "Killed (SIGKILL)",
            "some-command",
            137, // 128 + 9
        );
        assert!(matches!(
            reason,
            JobFailureReason::SignalReceived { signal: 9 }
        ));
    }

    #[test]
    fn test_categorize_process_spawn_failed() {
        let reason = categorize_error(
            "Failed to spawn process: Exec format error",
            "binary-file",
            -1,
        );
        assert!(matches!(
            reason,
            JobFailureReason::ProcessSpawnFailed { .. }
        ));
    }

    #[test]
    fn test_categorize_non_zero_exit_code() {
        let reason = categorize_error("Application error occurred", "my-app", 1);
        assert!(matches!(
            reason,
            JobFailureReason::NonZeroExitCode { exit_code: 1 }
        ));
    }

    #[test]
    fn test_categorize_infrastructure_error() {
        let reason = categorize_error("Connection lost to storage backend", "sync-command", -1);
        assert!(matches!(
            reason,
            JobFailureReason::InfrastructureError { .. }
        ));
    }

    #[test]
    fn test_categorize_unknown_error() {
        let reason = categorize_error("Something unexpected happened", "mystery-command", -1);
        assert!(matches!(reason, JobFailureReason::Unknown { .. }));
    }

    #[test]
    fn test_extract_path_from_error() {
        let path = extract_path_from_error("Permission denied: /home/user/scripts/run.sh");
        assert_eq!(path, Some("/home/user/scripts/run.sh".to_string()));
    }

    #[test]
    fn test_extract_timeout_secs() {
        let secs = extract_timeout_secs("Job timed out after 3600 seconds");
        assert_eq!(secs, Some(3600));
    }

    #[test]
    fn test_extract_signal() {
        let signal = extract_signal("Process received signal: SIGTERM");
        assert_eq!(signal, Some(15));
    }

    #[test]
    fn test_categorize_io_permission_denied() {
        let io_err = std::io::Error::new(ErrorKind::PermissionDenied, "access denied");
        let reason = categorize_io_error(&io_err, "/data/file.txt");
        assert!(matches!(reason, JobFailureReason::PermissionDenied { .. }));
    }

    #[test]
    fn test_categorize_io_not_found() {
        let io_err = std::io::Error::new(ErrorKind::NotFound, "file not found");
        let reason = categorize_io_error(&io_err, "/missing/file.txt");
        assert!(matches!(reason, JobFailureReason::FileNotFound { .. }));
    }
}
