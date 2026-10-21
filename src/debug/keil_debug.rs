//! Keil debug via UV4 -d + .ini script (batch / non-interactive).
//!
//! Flow per invocation:
//!   1. build a temp .ini (RESET + LOG > dump + printf reads + LOG OFF + EXIT)
//!   2. patch <tIfile> in the matching target's <DebugOpt> in .uvoptx
//!   3. with --no-download, also clear <UpdateFlashBeforeDebugging> for that
//!      target (.uvprojx, Utilities/Flash1)
//!   4. run `UV4 -d -j0 -t <target> <proj> -o log` with a timeout
//!   5. ALWAYS restore every field patched in 2 and 3
//!   6. read the dump file, strip command echoes, return
//!
//! This is batch-mode (one .ini runs to completion then exits). It is NOT an
//! interactive step-by-step debugger — for that use openocd + gdb.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use roxmltree::{Document, Node};

/// Parsed `emb debug keil` arguments.
pub struct DebugArgs {
    pub read: Option<String>,
    pub dump: Option<String>,
    pub regs: bool,
    pub break_: Option<String>,
    pub run_to: Option<String>,
    pub step: Option<usize>,
    pub pstep: Option<usize>,
    pub ini: Option<PathBuf>,
    pub timeout: u64,
    pub no_reset: bool,
    /// Leave the target's flash alone: read registers/memory from whatever is
    /// already running instead of reprogramming it. Implemented by clearing
    /// the project's own download flags for the duration of the session, since
    /// UV4 has no command-line switch for it.
    pub no_download: bool,
}

pub struct DebugResult {
    pub exit_code: u32,
    pub timed_out: bool,
    pub dump: String,
    pub uv4_log: String,
}

/// Run a Keil debug session (UV4 -d) with a generated (or user) .ini script.
pub fn run(
    proj: &Path,
    target: &str,
    args: &DebugArgs,
    cfg: &crate::config::EmbConfig,
) -> Result<DebugResult> {
    let uv4 = crate::config::resolve_uv4(cfg).context("locate UV4.exe")?;
    let uvoptx = uvoptx_of(proj)?;

    let tmp = tempfile::tempdir().context("create temp dir")?;
    let dump_path = tmp.path().join("emb_dump.txt");

    // 1. ini (custom or generated)
    let ini_path: PathBuf = match &args.ini {
        Some(p) => p.clone(),
        None => {
            let p = tmp.path().join("emb_debug.ini");
            std::fs::write(&p, build_ini(args, &dump_path))
                .with_context(|| format!("write ini {}", p.display()))?;
            p
        }
    };

    // 2. patch <tIfile> in .uvoptx (backup original)
    let orig_text = std::fs::read_to_string(&uvoptx)
        .with_context(|| format!("read {}", uvoptx.display()))?;
    let ini_abs = std::path::absolute(&ini_path)?;
    let ini_str = path_str(&ini_abs);
    let (patched, orig_tifile) = patch_tag(&orig_text, target, "tIfile", &ini_str)?;
    let Some(orig_tifile) = orig_tifile else {
        bail!("target '{}' or its <tIfile> not found in .uvoptx", target);
    };

    // 2a. --no-download: <UpdateFlashBeforeDebugging> is "Update target before
    //     Debugging" (Utilities tab) and it lives in the .uvprojx, not the
    //     .uvoptx. UV4 has no command-line switch for it, so it is cleared here
    //     and restored below, exactly like <tIfile>.
    //
    //     <tLdApp> ("Load Application at Startup") is deliberately left alone:
    //     measured on this project it does NOT gate the download (it still
    //     erased and programmed with tLdApp=0), but turning it off does drop
    //     the symbol table - `BS main` then fails with "undefined identifier",
    //     which silently breaks --break / --run-to.
    let orig_project_flag: Option<String> = if args.no_download {
        let proj_text = std::fs::read_to_string(proj)
            .with_context(|| format!("read {}", proj.display()))?;
        let (patched_proj, orig_flag) =
            patch_tag(&proj_text, target, "UpdateFlashBeforeDebugging", "0")?;
        match orig_flag {
            Some(orig) => {
                std::fs::write(proj, &patched_proj)
                    .with_context(|| format!("write {}", proj.display()))?;
                Some(orig)
            }
            None => None,
        }
    } else {
        None
    };
    std::fs::write(&uvoptx, &patched)
        .with_context(|| format!("write {}", uvoptx.display()))?;

    // 3. run UV4 -d with timeout
    let log_path = tmp.path().join("emb_uv4.log");
    let run_res = run_uv4_debug(&uv4, proj, target, &log_path, args.timeout);

    // 4. ALWAYS restore what was patched (Keil may have rewritten other fields,
    //    so re-read the current text and only replace the nodes we touched)
    let restore_text = std::fs::read_to_string(&uvoptx).unwrap_or_else(|_| patched.clone());
    let restored = restore_tag(&restore_text, target, "tIfile", &orig_tifile);
    match restored {
        Ok(r) => {
            let _ = std::fs::write(&uvoptx, &r);
        }
        Err(_) => {
            // last-resort: put back the exact bytes we read before patching
            let _ = std::fs::write(&uvoptx, &orig_text);
        }
    }

    if let Some(orig) = &orig_project_flag {
        let current = std::fs::read_to_string(proj).unwrap_or_default();
        if let Ok(back) = restore_tag(&current, target, "UpdateFlashBeforeDebugging", orig) {
            let _ = std::fs::write(proj, back);
        }
    }

    // 5. collect outputs
    let uv4_log = std::fs::read_to_string(&log_path).unwrap_or_default();
    let dump = std::fs::read_to_string(&dump_path).unwrap_or_default();

    let (exit_code, timed_out) = match run_res {
        Ok(c) => (c, false),
        Err(e) => {
            if format!("{e}").contains("timeout") {
                (124, true)
            } else {
                return Err(e);
            }
        }
    };

    Ok(DebugResult {
        exit_code,
        timed_out,
        dump,
        uv4_log,
    })
}

// ---------------------------------------------------------------------------
// .ini generation
// ---------------------------------------------------------------------------

fn build_ini(args: &DebugArgs, dump_path: &Path) -> String {
    let mut l: Vec<String> = Vec::new();
    if !args.no_reset {
        l.push("RESET".into());
    }
    l.push(format!("LOG > {}", path_str(dump_path)));

    if let Some(b) = &args.break_ {
        l.push(format!("BS {}", b));
    }
    if let Some(rt) = &args.run_to {
        // `G start,stop` runs with a *temporary* breakpoint at `stop`.
        //
        // Do not rewrite this as BS/G/BC: on a real target the breakpoint put
        // in the list with BS was never hit (a BS at `main`, or at an ITCM
        // function, ran for the full timeout), while `G ,main` stopped at main
        // immediately. A breakpoint that silently never fires is worse than no
        // breakpoint at all, so the working form is the one used here.
        l.push(format!("G ,{}", rt));
    }
    if let Some(n) = args.step {
        for _ in 0..n {
            l.push("Tstep".into());
        }
    }
    if let Some(n) = args.pstep {
        for _ in 0..n {
            l.push("Pstep".into());
        }
    }
    if let Some(r) = &args.read {
        let (addr, size) = parse_read(r);
        emit_read(&mut l, addr, size);
    }
    if let Some(d) = &args.dump {
        let (start, end) = parse_dump(d);
        l.push(format!("DISPLAY {:#X}, {:#X}", start, end));
    }
    if args.regs {
        for r in ["R0", "R1", "R2", "R3", "R12", "R13", "R14", "R15", "xPSR"] {
            l.push(format!("printf(\"{} = 0x%08X\\n\", {})", r, r));
        }
    }

    l.push("LOG OFF".into());
    l.push("EXIT".into());
    l.join("\n") + "\n"
}

fn emit_read(l: &mut Vec<String>, addr: u32, size: usize) {
    let words = (size / 4).clamp(1, 64);
    for i in 0..words {
        let a = addr + (i as u32) * 4;
        // Rust formats the address literal into the string; the single %08X is
        // consumed by Keil's printf at runtime with _RDWORD()'s return value.
        l.push(format!(
            "printf(\"[{:#010X}] = 0x%08X\\n\", _RDWORD({:#X}))",
            a, a
        ));
    }
}

fn parse_read(s: &str) -> (u32, usize) {
    match s.split_once('%') {
        Some((a, sz)) => (parse_addr(a), sz.parse().unwrap_or(4)),
        None => (parse_addr(s), 4),
    }
}

fn parse_dump(s: &str) -> (u32, u32) {
    match s.split_once(',') {
        Some((a, b)) => (parse_addr(a), parse_addr(b)),
        None => {
            let a = parse_addr(s);
            (a, a + 64)
        }
    }
}

fn parse_addr(s: &str) -> u32 {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(h, 16).unwrap_or(0)
    } else {
        s.parse::<u32>()
            .unwrap_or_else(|_| u32::from_str_radix(s, 16).unwrap_or(0))
    }
}

// ---------------------------------------------------------------------------
// .uvoptx / .uvprojx field patch / restore
// ---------------------------------------------------------------------------

/// Replace `<tag>…</tag>` inside `target`'s `<Target>` block.
///
/// Returns the new text plus the value that was there before, so the caller can
/// put it back. `None` means the tag is not present at all — nothing was
/// changed, and there is nothing to restore.
///
/// The edit is a byte-range replacement on the original text rather than a
/// re-serialisation: a debug session must not leave a thousands-of-lines
/// `git diff` behind in a file the user only asked emb to read.
fn patch_tag(
    text: &str,
    target: &str,
    tag: &str,
    new_value: &str,
) -> Result<(String, Option<String>)> {
    let doc = Document::parse(text)?;
    let target_node = find_target(&doc, target)?;

    let Some(node) = walk(target_node, tag) else {
        return Ok((text.to_string(), None));
    };

    let orig = node.text().unwrap_or("").to_string();
    let range = node.range();
    let mut s = text.to_string();
    s.replace_range(range.start..range.end, &format!("<{tag}>{new_value}</{tag}>"));
    Ok((s, Some(orig)))
}

fn restore_tag(text: &str, target: &str, tag: &str, orig: &str) -> Result<String> {
    Ok(patch_tag(text, target, tag, orig)?.0)
}

/// Find the `<Target>` element whose `<TargetName>` is `target`.
///
/// `<Target>` is a direct child of the root in a .uvoptx but sits under
/// `<Targets>` in a .uvprojx, so this has to recurse rather than scan one level.
fn find_target<'a, 'input>(doc: &'a Document<'input>, target: &str) -> Result<Node<'a, 'input>> {
    fn search<'a, 'input>(node: Node<'a, 'input>, target: &str) -> Option<Node<'a, 'input>> {
        if node.has_tag_name("Target") {
            let name = node
                .children()
                .find(|c| c.has_tag_name("TargetName"))
                .and_then(|n| n.text())
                .unwrap_or("");
            if name == target {
                return Some(node);
            }
        }
        for child in node.children().filter(|c| c.is_element()) {
            if let Some(found) = search(child, target) {
                return Some(found);
            }
        }
        None
    }

    match search(doc.root_element(), target) {
        Some(n) => Ok(n),
        None => bail!("target '{}' not found in the project options file", target),
    }
}

fn walk<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    for c in node.children() {
        if c.is_element() && c.has_tag_name(tag) {
            return Some(c);
        }
        if let Some(n) = walk(c, tag) {
            return Some(n);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn uvoptx_of(uvprojx: &Path) -> Result<PathBuf> {
    let stem = uvprojx.file_stem().context("no file stem in path")?;
    let mut p = uvprojx.with_file_name(stem);
    p.set_extension("uvoptx");
    if !p.is_file() {
        bail!("{} not found next to {}", p.display(), uvprojx.display());
    }
    Ok(p)
}

/// Render a path for Keil's .ini LOG/INCLUDE on Windows (backslashes).
fn path_str(p: &Path) -> String {
    p.display().to_string()
}

fn run_uv4_debug(
    uv4: &Path,
    proj: &Path,
    target: &str,
    log_path: &Path,
    timeout_s: u64,
) -> Result<u32> {
    let abs_proj = std::path::absolute(proj)?;
    let mut child = Command::new(uv4)
        .arg("-j0")
        .arg("-d")
        .arg(&abs_proj)
        .arg("-t")
        .arg(target)
        .arg("-o")
        .arg(log_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawn UV4 at {}", uv4.display()))?;

    let deadline = Instant::now() + Duration::from_secs(timeout_s);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code().unwrap_or(1) as u32);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("UV4 -d timeout after {}s", timeout_s);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Strip Keil command echoes so only actual output remains. Keil's LOG records
/// both each command line and its result; we drop the command lines.
pub fn filter_dump(s: &str) -> String {
    s.lines()
        .filter(|l| !is_command_echo(l.trim_start()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_command_echo(t: &str) -> bool {
    const CMDS: [&str; 11] = [
        "printf(", "LOG ", "RESET", "DISPLAY", "BS ", "BC ", "G", "Tstep", "Pstep", "EXIT",
        "LOG OFF",
    ];
    CMDS.iter().any(|c| t == *c || t.starts_with(c))
}

/// Last few non-empty lines of UV4's own log (`-o <file>`).
///
/// That log records what UV4 itself did — a failed debugger connect, a target
/// error — which the .ini dump does not capture, so on a timeout it is often
/// the only clue. Returns `None` when the log is empty or missing.
pub fn log_tail(log: &str) -> Option<String> {
    const TAIL_LINES: usize = 20;
    let lines: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }
    let start = lines.len().saturating_sub(TAIL_LINES);
    Some(format!(
        "UV4 log (last {} lines):\n{}",
        lines.len() - start,
        lines[start..].join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_tail_is_none_for_an_empty_log() {
        assert_eq!(log_tail(""), None);
        assert_eq!(log_tail("\n  \n\n"), None);
    }

    #[test]
    fn log_tail_keeps_short_logs_whole() {
        let t = log_tail("a\n\nb\n").unwrap();
        assert!(t.contains("last 2 lines"), "{t}");
        assert!(t.ends_with("a\nb"), "{t}");
    }

    #[test]
    fn log_tail_truncates_from_the_front() {
        let log: String = (1..=30).map(|i| format!("line {i}\n")).collect();
        let t = log_tail(&log).unwrap();
        assert!(t.contains("last 20 lines"), "{t}");
        assert!(t.contains("line 11"), "{t}");
        assert!(!t.contains("line 10\n"), "{t}");
        assert!(t.ends_with("line 30"), "{t}");
    }

    /// `.uvoptx` shape: `<Target>` is a direct child of the root.
    const UVOPTX: &str = "\
<ProjectOpt>
  <SchemaVersion>1.1</SchemaVersion>
  <Target>
    <TargetName>TX_SlotA</TargetName>
    <DebugOpt>
      <tIfile>..\\LinkScript\\Debug_script.ini</tIfile>
      <tLdApp>1</tLdApp>
    </DebugOpt>
  </Target>
  <Target>
    <TargetName>TX_SlotB</TargetName>
    <DebugOpt>
      <tIfile></tIfile>
      <tLdApp>0</tLdApp>
    </DebugOpt>
  </Target>
</ProjectOpt>
";

    /// `.uvprojx` shape: `<Target>` sits under `<Targets>`, and the download
    /// flag is in `<TargetOption><Utilities><Flash1>`.
    const UVPROJX: &str = "\
<Project>
  <SchemaVersion>2.1</SchemaVersion>
  <Targets>
    <Target>
      <TargetName>TX_SlotA</TargetName>
      <TargetOption>
        <Utilities>
          <Flash1>
            <UseTargetDll>1</UseTargetDll>
            <UpdateFlashBeforeDebugging>1</UpdateFlashBeforeDebugging>
          </Flash1>
        </Utilities>
      </TargetOption>
    </Target>
  </Targets>
</Project>
";

    /// The two files disagree about where `<Target>` lives, which is what the
    /// recursive search is for.
    #[test]
    fn patches_the_right_target_in_both_file_shapes() {
        let (patched, orig) = patch_tag(UVOPTX, "TX_SlotA", "tLdApp", "0").unwrap();
        assert_eq!(orig.as_deref(), Some("1"));
        assert!(patched.contains("<tLdApp>0</tLdApp>"));

        let (patched, orig) =
            patch_tag(UVPROJX, "TX_SlotA", "UpdateFlashBeforeDebugging", "0").unwrap();
        assert_eq!(orig.as_deref(), Some("1"));
        assert!(patched.contains("<UpdateFlashBeforeDebugging>0</UpdateFlashBeforeDebugging>"));
    }

    /// Only the matching target's flag may change: the second target's `tLdApp`
    /// is already 0, and the first one must not leak into it.
    #[test]
    fn the_other_target_is_left_alone() {
        let (patched, _) = patch_tag(UVOPTX, "TX_SlotA", "tLdApp", "9").unwrap();
        let tx_slot_b = patched.split("<TargetName>TX_SlotB</TargetName>").nth(1).unwrap();
        assert!(tx_slot_b.contains("<tLdApp>0</tLdApp>"), "{tx_slot_b}");
    }

    /// The edit is a range replacement, so every byte outside the field is
    /// untouched. Anything else would show up as a huge diff in the user's
    /// project file.
    #[test]
    fn nothing_but_the_field_changes() {
        let (patched, _) = patch_tag(UVPROJX, "TX_SlotA", "UpdateFlashBeforeDebugging", "0").unwrap();
        assert_eq!(
            patched,
            UVPROJX.replace(
                "<UpdateFlashBeforeDebugging>1</UpdateFlashBeforeDebugging>",
                "<UpdateFlashBeforeDebugging>0</UpdateFlashBeforeDebugging>"
            )
        );
    }

    #[test]
    fn restoring_puts_the_original_value_back() {
        let (patched, orig) = patch_tag(UVOPTX, "TX_SlotA", "tLdApp", "0").unwrap();
        let restored = restore_tag(&patched, "TX_SlotA", "tLdApp", orig.as_deref().unwrap()).unwrap();
        assert_eq!(restored, UVOPTX);
    }

    /// A project without the flag is not an error: there is simply nothing to
    /// restore, and the session runs with whatever the project already says.
    #[test]
    fn a_missing_tag_changes_nothing() {
        let (patched, orig) = patch_tag(UVOPTX, "TX_SlotA", "UpdateFlashBeforeDebugging", "0").unwrap();
        assert_eq!(patched, UVOPTX);
        assert_eq!(orig, None);
    }

    #[test]
    fn an_unknown_target_is_an_error() {
        assert!(patch_tag(UVOPTX, "NoSuchTarget", "tLdApp", "0").is_err());
    }

    fn args_run_to(sym: &str) -> DebugArgs {
        DebugArgs {
            read: None,
            dump: None,
            regs: false,
            break_: None,
            run_to: Some(sym.to_string()),
            step: None,
            pstep: None,
            ini: None,
            timeout: 60,
            no_reset: false,
            no_download: false,
        }
    }

    /// `G start,stop` is the form that actually stops on a real target. The
    /// obvious BS/G/BC encoding left the program running for the whole
    /// timeout: `G` with no stop address never honoured the listed breakpoint
    /// (measured on STM32H750 + ST-Link; `G ,main` stopped instantly).
    #[test]
    fn run_to_uses_the_temporary_breakpoint_form() {
        let ini = build_ini(&args_run_to("main"), Path::new("dump.txt"));
        assert!(ini.contains("G ,main"), "{ini}");
        assert!(!ini.contains("BS main"), "{ini}");
    }

    /// RESET is skipped on request, but must be the first command otherwise.
    #[test]
    fn reset_can_be_skipped() {
        let mut a = args_run_to("main");
        assert!(build_ini(&a, Path::new("d.txt")).starts_with("RESET\n"));

        a.no_reset = true;
        assert!(!build_ini(&a, Path::new("d.txt")).contains("RESET"));
    }
}
