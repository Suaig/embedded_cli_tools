//! Commands must not report success when they did nothing useful: an agent
//! reading the exit code cannot otherwise tell "nothing matched" or "no such
//! thing" apart from "here is your answer".

mod common;

use common::{emb, write_fixture};

#[test]
fn config_with_unknown_category_reports_no_match() {
    let proj = write_fixture("bad_category.uvprojx");
    let out = emb(&[
        "keil", "config", proj.to_str().unwrap(), "-t", "OnlyTarget", "nosuchcategory",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "an unmatched category must not exit 0");
    assert!(
        stderr.contains("nosuchcategory"),
        "error should name the category, got: {stderr}"
    );
    assert!(
        stderr.contains("ccompiler"),
        "error should list the categories that do exist, got: {stderr}"
    );
}

#[test]
fn config_with_a_known_category_still_succeeds() {
    let proj = write_fixture("good_category.uvprojx");
    let out = emb(&[
        "keil", "config", proj.to_str().unwrap(), "-t", "OnlyTarget", "ccompiler",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "stdout={stdout}");
    assert!(stdout.contains("ccompiler.optim"), "stdout={stdout}");
}

#[test]
fn files_with_unknown_group_reports_no_match() {
    let proj = write_fixture("bad_group.uvprojx");
    let out = emb(&[
        "keil", "files", proj.to_str().unwrap(), "-t", "OnlyTarget", "-g", "NoSuchGroup",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "an unmatched group must not exit 0");
    assert!(
        stderr.contains("NoSuchGroup"),
        "error should name the group, got: {stderr}"
    );
    assert!(
        stderr.contains("Application"),
        "error should list the groups that do exist, got: {stderr}"
    );
}

#[test]
fn files_with_a_known_group_still_succeeds() {
    let proj = write_fixture("good_group.uvprojx");
    let out = emb(&[
        "keil", "files", proj.to_str().unwrap(), "-t", "OnlyTarget", "-g", "Application",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("main.c"), "stdout={stdout}");
}

/// An id that was never started must not look like a daemon that simply has
/// nothing to report — `stop` and `send` already rejected it.
#[test]
fn daemon_read_and_history_reject_an_unknown_id() {
    for sub in ["read", "history"] {
        let out = emb(&["serial", "daemon", sub, "definitely-not-a-daemon"]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);

        assert!(
            !out.status.success(),
            "daemon {sub} on an unknown id must not exit 0; stdout={stdout}"
        );
        assert!(
            stderr.contains("definitely-not-a-daemon"),
            "daemon {sub}: error should name the id, got: {stderr}"
        );
    }
}

/// Adding a file that does not exist on disk is legitimate — Keil lets you
/// register a source before creating it — so this stays exit 0. But it has to
/// say so, otherwise an agent believes it got the path right.
#[test]
fn file_add_warns_about_a_path_that_does_not_exist_yet() {
    let proj = write_fixture("file_add_ghost.uvprojx");
    let out = emb(&[
        "keil", "file-add", proj.to_str().unwrap(), "-t", "OnlyTarget",
        "-g", "Application", "..\\ghost.c",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "a missing file is a warning, not an error: {stderr}"
    );
    assert!(
        stderr.contains("ghost.c"),
        "warning should name the path, got: {stderr}"
    );
}

#[test]
fn file_add_is_quiet_when_the_path_exists() {
    let proj = write_fixture("file_add_real.uvprojx");
    let real = proj.parent().unwrap().join("real_source.c");
    std::fs::write(&real, "int main(void) { return 0; }\n").unwrap();

    let out = emb(&[
        "keil", "file-add", proj.to_str().unwrap(), "-t", "OnlyTarget",
        "-g", "Application", "real_source.c",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "{stderr}");
    assert!(
        !stderr.contains("warning"),
        "an existing file must not warn, got: {stderr}"
    );
}
