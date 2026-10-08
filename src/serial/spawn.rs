//! Spawn the detached serial daemon without leaking this process's handles
//! into it.
//!
//! Why this exists instead of a plain `std::process::Command`. Rust's Windows
//! `Command` passes `bInheritHandles = TRUE` and duplicates **every**
//! inheritable handle in our handle table into the child - there is no
//! whitelist (rust-lang/rust#161158; the `inherit_handles` opt-out is nightly
//! only, rust-lang/rust#146407). The daemon is a long-lived detached process,
//! so it keeps holding our caller's stdout pipe until it exits, and anything
//! that pipes this command hangs forever waiting for an EOF that never comes:
//!
//!     emb serial daemon start COM16 --id fan | head      # never returns
//!
//! `Stdio::null()` does not help: it only repoints the child's own std
//! handles, it does not stop the extra handles from being inherited.
//!
//! The documented cure is `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`, which turns
//! `bInheritHandles = TRUE` into "inherit exactly these": the three NUL
//! handles we hand the child. Everything else stays in this process.

use std::io;
use std::path::Path;

/// Start `exe args...` detached, with only NUL inherited. Returns the child's
/// PID. The child is not waited for.
#[cfg(windows)]
pub fn spawn_detached(exe: &Path, args: &[String]) -> io::Result<u32> {
    imp::spawn(exe, args)
}

/// Unix does not have this problem: `Stdio::null()` dup2s NUL over the child's
/// fds 0/1/2, and Rust marks its own fds close-on-exec, so the caller's pipe
/// does not survive into the child.
#[cfg(not(windows))]
pub fn spawn_detached(exe: &Path, args: &[String]) -> io::Result<u32> {
    use std::process::{Command, Stdio};

    let child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = child.id();
    drop(child); // do not wait on it
    Ok(pid)
}

#[cfg(windows)]
mod imp {
    use std::ffi::{c_void, OsStr};
    use std::io;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
        UpdateProcThreadAttribute, CREATE_NEW_PROCESS_GROUP, EXTENDED_STARTUPINFO_PRESENT,
        PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES,
        STARTUPINFOEXW,
    };

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(once(0)).collect()
    }

    /// Quote one argument by the rules the MSVC CRT uses to split a command
    /// line back apart: wrap in quotes when needed, and double the backslashes
    /// that precede a quote (or the closing quote).
    fn push_arg(out: &mut String, arg: &str) {
        if !out.is_empty() {
            out.push(' ');
        }
        if !arg.is_empty() && !arg.chars().any(|c| c == ' ' || c == '\t' || c == '"') {
            out.push_str(arg);
            return;
        }
        out.push('"');
        let mut backslashes = 0usize;
        for c in arg.chars() {
            match c {
                '\\' => {
                    backslashes += 1;
                    out.push('\\');
                }
                '"' => {
                    // Every backslash before a quote must be doubled, and the
                    // quote itself escaped.
                    for _ in 0..=backslashes {
                        out.push('\\');
                    }
                    out.push('"');
                    backslashes = 0;
                }
                _ => {
                    backslashes = 0;
                    out.push(c);
                }
            }
        }
        for _ in 0..backslashes {
            out.push('\\');
        }
        out.push('"');
    }

    fn command_line(exe: &Path, args: &[String]) -> Vec<u16> {
        let mut s = String::new();
        push_arg(&mut s, &exe.to_string_lossy());
        for a in args {
            push_arg(&mut s, a);
        }
        s.encode_utf16().chain(once(0)).collect()
    }

    /// Closes whatever it was given when it goes out of scope.
    struct OwnedHandles(Vec<HANDLE>);

    impl Drop for OwnedHandles {
        fn drop(&mut self) {
            for h in &self.0 {
                if !h.is_null() && *h != INVALID_HANDLE_VALUE {
                    unsafe { CloseHandle(*h) };
                }
            }
        }
    }

    struct AttrList(*mut c_void);

    impl Drop for AttrList {
        fn drop(&mut self) {
            unsafe { DeleteProcThreadAttributeList(self.0) };
        }
    }

    pub fn spawn(exe: &Path, args: &[String]) -> io::Result<u32> {
        unsafe {
            // The only handles the child is allowed to inherit: NUL for all
            // three streams. The daemon never writes to them; they exist so a
            // panic or a stray print does not land somewhere unexpected.
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: ptr::null_mut(),
                bInheritHandle: 1,
            };
            let nul = wide(OsStr::new("NUL"));

            let mut owned = OwnedHandles(Vec::with_capacity(3));
            for _ in 0..3 {
                let h = CreateFileW(
                    nul.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    &sa,
                    OPEN_EXISTING,
                    0,
                    ptr::null_mut(),
                );
                if h == INVALID_HANDLE_VALUE {
                    return Err(io::Error::last_os_error());
                }
                owned.0.push(h);
            }
            let nul_handles: [HANDLE; 3] = [owned.0[0], owned.0[1], owned.0[2]];

            // The attribute list has to live in a buffer whose size the API
            // tells us by failing on purpose the first time round.
            let mut size: usize = 0;
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size);
            let mut buf = vec![0u8; size];
            let raw = buf.as_mut_ptr() as *mut c_void;
            if InitializeProcThreadAttributeList(raw, 1, 0, &mut size) == 0 {
                return Err(io::Error::last_os_error());
            }
            let attr = AttrList(raw);

            // bInheritHandles is still TRUE below; this is what narrows it
            // down to the list.
            if UpdateProcThreadAttribute(
                attr.0,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                nul_handles.as_ptr() as *const c_void,
                std::mem::size_of_val(&nul_handles),
                ptr::null_mut(),
                ptr::null(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }

            let mut si: STARTUPINFOEXW = std::mem::zeroed();
            si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            si.StartupInfo.hStdInput = nul_handles[0];
            si.StartupInfo.hStdOutput = nul_handles[1];
            si.StartupInfo.hStdError = nul_handles[2];
            si.lpAttributeList = attr.0;

            let mut cmdline = command_line(exe, args);
            let mut pi: PROCESS_INFORMATION = std::mem::zeroed();

            let ok = CreateProcessW(
                ptr::null(), // argv[0] of the command line names the program
                cmdline.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_NEW_PROCESS_GROUP,
                ptr::null(),
                ptr::null(),
                // The EX struct starts with the plain one, and its cb already
                // says so - passing the first field is what the API expects.
                &si.StartupInfo,
                &mut pi,
            );
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }

            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            Ok(pi.dwProcessId)
        }
    }
}
