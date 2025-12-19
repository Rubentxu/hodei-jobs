use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
fn test_worker_graceful_shutdown_during_registration() {
    // Build the binary first to ensure we are running the latest version
    // Build step removed to avoid deadlock. Cargo handles this.

    // Path to the binary
    // Note: Cargo sets CARGO_BIN_EXE_<name> only for integration tests inside the crate.
    // Since we created crates/worker/bin/tests, this should work.
    let bin_path = env!("CARGO_BIN_EXE_hodei-worker-bin");

    // Start the worker
    let mut child = Command::new(bin_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()) // Capture stderr too as logs might go there
        .spawn()
        .expect("Failed to spawn worker");

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);

    // Wait for registration retry log
    let mut started = false;
    let mut handle = child; // Move child to keep it alive

    // Read logs in a separate thread to avoid blocking if the buffer fills up?
    // Actually, we just need to read until we see the "Retrying" message.

    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        println!("[Worker Output] {}", line);

        // Match either the connection attempt or the retry log
        if line.contains("Retrying registration") || line.contains("Connecting to server") {
            started = true;
            break;
        }
    }
    assert!(
        started,
        "Worker did not start correctly or reach connection phase"
    );

    // Give it a moment to ensure it enters the sleep loop
    thread::sleep(Duration::from_secs(2));

    println!("Sending SIGINT to worker...");
    // Send SIGINT
    // Using "kill -2 <pid>" approach for portability on Linux/Unix
    let pid = handle.id();
    let kill_status = Command::new("kill")
        .arg("-s")
        .arg("INT")
        .arg(pid.to_string())
        .status()
        .expect("Failed to execute kill command");

    assert!(kill_status.success(), "Failed to send SIGINT");

    // Wait for exit
    println!("Waiting for worker to exit...");
    let status = handle.wait().expect("Failed to wait on child");

    println!("Worker exited with status: beep {}", status);
    assert!(
        status.success(),
        "Worker failed to exit successfully (code 0)"
    );
}
