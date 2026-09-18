//! CLI-level regression tests for `emb ioc generate`.
//!
//! The failure paths here must not launch STM32CubeMX: it is a heavyweight JVM,
//! and handing it a missing .ioc makes it die with a NullPointerException only
//! after the client has waited out the whole generation timeout.

mod common;

use common::emb;
use std::time::Instant;

#[test]
fn missing_ioc_is_rejected_without_launching_cubemx() {
    let started = Instant::now();
    let out = emb(&["ioc", "generate", "definitely_missing.ioc"]);
    let elapsed = started.elapsed();
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "missing .ioc must not exit 0");
    assert!(
        stderr.contains("definitely_missing.ioc"),
        "error should name the missing file, got: {stderr}"
    );
    assert!(
        !stderr.contains("timed out"),
        "must fail fast rather than waiting out the CubeMX timeout: {stderr}"
    );
    assert!(
        !stderr.contains("STM32CubeMX not found"),
        "the file check must run before CubeMX discovery: {stderr}"
    );
    assert!(
        elapsed.as_secs() < 30,
        "returned after {elapsed:?}; the generation timeout is 120s, so CubeMX was launched"
    );
}
