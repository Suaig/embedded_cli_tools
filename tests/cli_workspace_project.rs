//! `-p <project>` must select the sub-project for every command that operates on
//! a workspace, not just the read-only ones.

mod common;

use common::{emb, write_workspace};

/// An unknown target makes the command fail *after* sub-project resolution, and
/// the "available" list names the targets of whichever sub-project was selected.
/// That is enough to prove which one `-p` picked, without invoking UV4.
fn available_targets(ws: &std::path::Path, sub: &str, extra: &[&str]) -> String {
    let ws = ws.to_str().unwrap().to_string();
    let mut args = vec!["keil", sub, ws.as_str()];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["-t", "NoSuchTarget"]);
    let out = emb(&args);
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn build_routes_to_the_project_named_by_p() {
    let ws = write_workspace("ws_build.uvmpw");

    let by_filename = available_targets(&ws, "build", &["-p", "b.uvprojx"]);
    assert!(
        by_filename.contains("TargetB"),
        "`-p b.uvprojx` should select SubB, got: {by_filename}"
    );

    let by_path = available_targets(&ws, "build", &["-p", r".\SubA\a.uvprojx"]);
    assert!(
        by_path.contains("TargetA"),
        "`-p .\\SubA\\a.uvprojx` should select SubA, got: {by_path}"
    );

    let default = available_targets(&ws, "build", &[]);
    assert!(
        default.contains("TargetA"),
        "without -p the first sub-project should be used, got: {default}"
    );
}

#[test]
fn rebuild_clean_and_flash_also_accept_p() {
    let ws = write_workspace("ws_more.uvmpw");
    for sub in ["rebuild", "clean", "flash"] {
        let out = available_targets(&ws, sub, &["-p", "b.uvprojx"]);
        assert!(
            !out.contains("unexpected argument"),
            "{sub}: -p should be accepted, got: {out}"
        );
        assert!(
            out.contains("TargetB"),
            "{sub}: -p b.uvprojx should select SubB, got: {out}"
        );
    }
}

#[test]
fn unknown_project_is_rejected() {
    let ws = write_workspace("ws_badproj.uvmpw");
    let ws_s = ws.to_str().unwrap().to_string();
    let out = emb(&["keil", "build", &ws_s, "-p", "missing.uvprojx", "-t", "TargetA"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "an unknown project must not exit 0");
    assert!(
        stderr.contains("missing.uvprojx"),
        "error should name the project, got: {stderr}"
    );
    assert!(
        stderr.contains("a.uvprojx") && stderr.contains("b.uvprojx"),
        "error should list the projects that do exist, got: {stderr}"
    );
}

#[test]
fn locate_routes_to_the_project_named_by_p() {
    let ws = write_workspace("ws_locate.uvmpw");
    let ws_s = ws.to_str().unwrap();

    let out = emb(&["keil", "locate", ws_s, "-p", "b.uvprojx", "-t", "TargetB", "defines"]);
    let sub_b = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "locate with -p should succeed, got: {sub_b}");
    assert!(sub_b.contains("DEFINE_B"), "should have read SubB, got: {sub_b}");

    let out = emb(&["keil", "locate", ws_s, "-t", "TargetA", "defines"]);
    let sub_a = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "locate without -p should succeed, got: {sub_a}");
    assert!(
        sub_a.contains("DEFINE_A"),
        "without -p the first sub-project should be used, got: {sub_a}"
    );
}
