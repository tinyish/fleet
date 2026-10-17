//! `fleet supervise` is what the service manager runs, and takes nothing
//! after it (fleet-lc94.10).

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn fresh_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("fleet-supervise-{}-{nanos}", std::process::id()))
}

#[test]
fn supervise_with_an_argument_is_refused_and_serves_nothing() {
    for extra in ["--help", "now"] {
        let state_dir = fresh_dir();
        let child = Command::new(env!("CARGO_BIN_EXE_fleet"))
            .args(["supervise", extra])
            .env("FLEET_STATE_DIR", &state_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        assert!(child.is_ok(), "fleet runs: {child:?}");
        let Ok(mut child) = child else { return };
        let deadline = Instant::now() + Duration::from_secs(5);
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let served = state_dir.exists();
        let _ = std::fs::remove_dir_all(&state_dir);
        assert_eq!(
            code,
            Some(2),
            "`fleet supervise {extra}` (None: it kept running)"
        );
        assert!(!served, "`fleet supervise {extra}` made a state directory");
    }
}
