//! CLI-level regression tests for `emb keil build`.
//!
//! An unknown target must be rejected while resolving the project, before any
//! toolchain is located — so these run without UV4.exe installed.

mod common;

use common::{emb, write_fixture};

#[test]
fn build_rejects_unknown_target_without_invoking_a_toolchain() {
    let proj = write_fixture("unknown_target.uvprojx");
    let out = emb(&["keil", "build", proj.to_str().unwrap(), "-t", "NoSuchTarget"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "unknown target must not exit 0; stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        stderr.contains("target 'NoSuchTarget' not found"),
        "expected a target-not-found error, got: {stderr}"
    );
    assert!(
        stderr.contains("OnlyTarget"),
        "error should list the available targets, got: {stderr}"
    );
}

#[test]
fn clean_rejects_unknown_target() {
    let proj = write_fixture("unknown_target_clean.uvprojx");
    let out = emb(&["keil", "clean", proj.to_str().unwrap(), "-t", "NoSuchTarget"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "unknown target must not exit 0");
    assert!(
        stderr.contains("target 'NoSuchTarget' not found"),
        "expected a target-not-found error, got: {stderr}"
    );
}

#[test]
fn rebuild_and_flash_reject_unknown_target() {
    let proj = write_fixture("unknown_target_more.uvprojx");
    for sub in ["rebuild", "flash"] {
        let out = emb(&["keil", sub, proj.to_str().unwrap(), "-t", "NoSuchTarget"]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{sub}: unknown target must not exit 0");
        assert!(
            stderr.contains("target 'NoSuchTarget' not found"),
            "{sub}: expected a target-not-found error, got: {stderr}"
        );
    }
}

#[test]
fn build_accepts_a_known_target() {
    // No UV4 on the test machine is fine: the point is that target validation
    // must not reject a target that does exist. Any failure must come later,
    // from toolchain discovery.
    let proj = write_fixture("known_target.uvprojx");
    let out = emb(&["keil", "build", proj.to_str().unwrap(), "-t", "OnlyTarget"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !stderr.contains("target 'OnlyTarget' not found"),
        "a valid target must pass validation, got: {stderr}"
    );
}
