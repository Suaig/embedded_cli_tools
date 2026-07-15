//! Keil debug via UV4 -d + .ini script (batch / non-interactive).
//!
//! Flow per invocation:
//!   1. build a temp .ini (RESET + LOG > dump + printf reads + LOG OFF + EXIT)
//!   2. patch <tIfile> in the matching target's <DebugOpt> in .uvoptx
//!   3. run `UV4 -d -j0 -t <target> <proj> -o log` with a timeout
//!   4. ALWAYS restore <tIfile> to its original value
//!   5. read the dump file, strip command echoes, return
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
    let (patched, orig_tifile) = patch_tifile(&orig_text, target, &ini_str)?;
    std::fs::write(&uvoptx, &patched)
        .with_context(|| format!("write {}", uvoptx.display()))?;

    // 3. run UV4 -d with timeout
    let log_path = tmp.path().join("emb_uv4.log");
    let run_res = run_uv4_debug(&uv4, proj, target, &log_path, args.timeout);

    // 4. ALWAYS restore <tIfile> (Keil may have rewritten other fields, so
    //    re-read the current text and only replace the tIfile node)
    let restore_text = std::fs::read_to_string(&uvoptx).unwrap_or_else(|_| patched.clone());
    match restore_tifile(&restore_text, target, &orig_tifile) {
        Ok(r) => {
            let _ = std::fs::write(&uvoptx, &r);
        }
        Err(_) => {
            // last-resort: put back the exact bytes we read before patching
            let _ = std::fs::write(&uvoptx, &orig_text);
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
        // temp breakpoint, run to it, then clear it
        l.push(format!("BS {}", rt));
        l.push("G".into());
        l.push(format!("BC {}", rt));
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
// .uvoptx <tIfile> patch / restore
// ---------------------------------------------------------------------------

fn patch_tifile(text: &str, target: &str, new_value: &str) -> Result<(String, String)> {
    let doc = Document::parse(text)?;
    let node = find_target_tifile(&doc, target)?;
    let orig = node.text().unwrap_or("").to_string();
    let range = node.range();
    let mut s = text.to_string();
    s.replace_range(range.start..range.end, &format!("<tIfile>{}</tIfile>", new_value));
    Ok((s, orig))
}

fn restore_tifile(text: &str, target: &str, orig: &str) -> Result<String> {
    let doc = Document::parse(text)?;
    let node = find_target_tifile(&doc, target)?;
    let range = node.range();
    let mut s = text.to_string();
    s.replace_range(range.start..range.end, &format!("<tIfile>{}</tIfile>", orig));
    Ok(s)
}

fn find_target_tifile<'a, 'input>(
    doc: &'a Document<'input>,
    target: &str,
) -> Result<Node<'a, 'input>> {
    let root = doc.root_element();
    for t in root.children().filter(|c| c.has_tag_name("Target")) {
        let name = t
            .children()
            .find(|c| c.has_tag_name("TargetName"))
            .and_then(|n| n.text())
            .unwrap_or("");
        if name == target {
            if let Some(n) = walk(t, "tIfile") {
                return Ok(n);
            }
        }
    }
    bail!("target '{}' or its <tIfile> not found in .uvoptx", target)
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
