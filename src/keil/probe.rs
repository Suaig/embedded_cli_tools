//! Debug-probe pre-flight check for `keil flash`.
//!
//! μVision has no command-line switch for choosing the debug adapter — the
//! choice lives in the `.uvoptx` (`<pMon>` plus `<nTsel>`). Point a project at a
//! probe that is not plugged in and UV4 fails with `Internal DLL Error` or
//! `Target DLL has been cancelled`: an error that names neither the probe the
//! project expects nor the one that is attached, which is what makes it cost
//! several rounds to work out.
//!
//! This module compares the two and says so *before* the flash runs. It only
//! ever warns: the device list is a heuristic (probes enumerate under vendor
//! strings that vary by driver and locale), so a check that cannot see a probe
//! says nothing rather than blocking work that would have succeeded.

use std::path::{Path, PathBuf};

/// A debug pod, as far as this check cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    StLink,
    CmsisDap,
    JLink,
    Ulink,
    /// A driver name this module does not recognise; never reported as a
    /// mismatch, because it cannot tell what it would be a mismatch with.
    Unknown,
}

impl ProbeKind {
    pub fn label(self) -> &'static str {
        match self {
            ProbeKind::StLink => "ST-Link",
            ProbeKind::CmsisDap => "CMSIS-DAP",
            ProbeKind::JLink => "J-Link",
            ProbeKind::Ulink => "ULINK",
            ProbeKind::Unknown => "unknown",
        }
    }
}

/// Map a `.uvoptx` `<pMon>` driver name onto the pod it drives.
///
/// Seen in the wild: `STLink\ST-LINKIII-KEIL_SWO.dll`,
/// `BIN\CMSIS_AGDI.dll`, `BIN\JLink\JLinkARM.dll`, `BIN\UL2CM3.DLL`.
pub fn kind_from_pmon(pmon: &str) -> ProbeKind {
    let p = pmon.to_ascii_lowercase();
    if p.contains("stlink") {
        ProbeKind::StLink
    } else if p.contains("cmsis") {
        ProbeKind::CmsisDap
    } else if p.contains("jlink") || p.contains("jl2cm3") {
        ProbeKind::JLink
    } else if p.contains("ulink") || p.contains("ul2cm3") {
        ProbeKind::Ulink
    } else {
        ProbeKind::Unknown
    }
}

/// Classify a PnP device name, or `None` when it is not a debug probe.
///
/// The patterns have to be this tight: a loose `DAP` matches every
/// "…Adapter" on the machine (Wi-Fi Direct, Hyper-V, "AC Adapter"), which would
/// turn the check into noise.
pub fn kind_from_device_name(name: &str) -> Option<ProbeKind> {
    let n = name.to_ascii_lowercase();
    if n.contains("st-link") || n.contains("stlink") {
        Some(ProbeKind::StLink)
    } else if n.contains("cmsis-dap") || n.contains("cmsis_dap") {
        Some(ProbeKind::CmsisDap)
    } else if n.contains("j-link") || n.contains("jlink") {
        Some(ProbeKind::JLink)
    } else if n.contains("ulink") {
        Some(ProbeKind::Ulink)
    } else {
        None
    }
}

/// The `.uvoptx` that belongs to a `.uvprojx`: same directory, same stem.
pub fn uvoptx_path(project: &Path) -> PathBuf {
    project.with_extension("uvoptx")
}

/// Read the probe a `.uvoptx` configures for `target`.
///
/// Each `<Target>` block carries its own `<pMon>`, and the blocks really do
/// differ: a two-slot project can have one target on ST-Link and the other on
/// CMSIS-DAP, which is a mismatch waiting to happen on whichever one was not
/// touched last.
pub fn configured_probe(text: &str, target: &str) -> Option<ProbeKind> {
    let mut depth: usize = 0;
    let mut name = String::new();
    let mut pmon: Option<String> = None;

    for line in text.lines() {
        let t = line.trim();

        if t.starts_with("<Target>") {
            depth += 1;
            if depth == 1 {
                name.clear();
                pmon = None;
            }
        } else if t.starts_with("</Target>") {
            if depth == 1 && name.trim() == target {
                return pmon.as_deref().map(kind_from_pmon);
            }
            depth = depth.saturating_sub(1);
        } else if depth == 1 {
            if let Some(rest) = t.strip_prefix("<TargetName>") {
                name = rest.split("</TargetName>").next().unwrap_or("").to_string();
            } else if let Some(rest) = t.strip_prefix("<pMon>") {
                pmon = Some(rest.split("</pMon>").next().unwrap_or("").to_string());
            }
        }
    }

    None
}

/// Debug probes currently attached, as `(device name, kind)`.
#[cfg(target_os = "windows")]
pub fn connected_probes() -> Vec<(String, ProbeKind)> {
    // Only the tight vendor patterns; see `kind_from_device_name`.
    let script = concat!(
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8; ",
        "Get-CimInstance Win32_PnPEntity ",
        "| Where-Object { $_.Name -match 'ST-?Link|CMSIS-?DAP|J-?Link|ULINK' } ",
        "| ForEach-Object { $_.Name }"
    );

    let Ok(output) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
    else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| kind_from_device_name(l).map(|k| (l.to_string(), k)))
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub fn connected_probes() -> Vec<(String, ProbeKind)> {
    Vec::new()
}

/// Warn when the project expects a probe that is not attached.
///
/// Silent when the `.uvoptx` is missing, names an unknown driver, or no probe
/// at all could be seen — "cannot tell" must never turn into a warning.
pub fn warn_if_probe_mismatch(project: &Path, target: &str) {
    let uvoptx = uvoptx_path(project);
    let Ok(text) = std::fs::read_to_string(&uvoptx) else {
        return;
    };
    let Some(configured) = configured_probe(&text, target) else {
        return;
    };
    if configured == ProbeKind::Unknown {
        return;
    }

    let connected = connected_probes();
    if connected.is_empty() || connected.iter().any(|(_, kind)| *kind == configured) {
        return;
    }

    let found: Vec<String> = connected
        .iter()
        .map(|(name, kind)| format!("{} ({})", kind.label(), name))
        .collect();

    eprintln!(
        "warning: {} is configured for {}, but the only debug probes found are: {}",
        uvoptx.file_name().unwrap_or_default().to_string_lossy(),
        configured.label(),
        found.join(", ")
    );
    eprintln!(
        "         UV4 would report this as \"Target DLL has been cancelled\" / \
         \"Internal DLL Error\" rather than naming the probe."
    );
}

/// Explain a *failed* session when no probe could be seen at all.
///
/// The pre-flight check stays silent when enumeration finds nothing, because it
/// cannot tell "no probe attached" from "a probe whose name it does not know".
/// Once the session has already failed, that silence is the wrong answer: UV4's
/// own message is `Target DLL has been cancelled`, which points nowhere.
pub fn missing_probe_hint(project: &Path, target: &str) -> Option<String> {
    let text = std::fs::read_to_string(uvoptx_path(project)).ok()?;
    let configured = configured_probe(&text, target)?;
    if configured == ProbeKind::Unknown || !connected_probes().is_empty() {
        return None;
    }

    Some(format!(
        "note: the project is configured for {}, but no debug probe was detected on this \
         machine. UV4 reports that as \"Target DLL has been cancelled\" - connect the probe, \
         or check that its driver is installed.",
        configured.label()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UVOPTX: &str = "\
<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>
<ProjectOpt>
  <SchemaVersion>1.1</SchemaVersion>
  <Target>
    <TargetName>TX_SlotA</TargetName>
    <DebugOpt>
      <nTsel>6</nTsel>
      <pMon>STLink\\ST-LINKIII-KEIL_SWO.dll</pMon>
    </DebugOpt>
  </Target>
  <Target>
    <TargetName>TX_SlotB</TargetName>
    <DebugOpt>
      <nTsel>3</nTsel>
      <pMon>BIN\\CMSIS_AGDI.dll</pMon>
    </DebugOpt>
  </Target>
</ProjectOpt>
";

    #[test]
    fn each_target_has_its_own_probe() {
        assert_eq!(
            configured_probe(UVOPTX, "TX_SlotA"),
            Some(ProbeKind::StLink)
        );
        assert_eq!(
            configured_probe(UVOPTX, "TX_SlotB"),
            Some(ProbeKind::CmsisDap)
        );
        assert_eq!(configured_probe(UVOPTX, "NoSuchTarget"), None);
    }

    #[test]
    fn driver_names_map_to_pods() {
        assert_eq!(
            kind_from_pmon("STLink\\ST-LINKIII-KEIL_SWO.dll"),
            ProbeKind::StLink
        );
        assert_eq!(kind_from_pmon("BIN\\CMSIS_AGDI.dll"), ProbeKind::CmsisDap);
        assert_eq!(kind_from_pmon("BIN\\JLink\\JLinkARM.dll"), ProbeKind::JLink);
        assert_eq!(kind_from_pmon("BIN\\UL2CM3.DLL"), ProbeKind::Ulink);
        assert_eq!(kind_from_pmon("Simulator"), ProbeKind::Unknown);
        assert_eq!(kind_from_pmon(""), ProbeKind::Unknown);
    }

    /// The loose patterns are the whole risk here: `DAP` alone matches
    /// "Wi-Fi Direct Virtual Adapter", "AC Adapter" and half of Hyper-V, which
    /// is what the first version of the PowerShell filter returned.
    #[test]
    fn unrelated_device_names_are_not_probes() {
        for name in [
            "Microsoft Wi-Fi Direct Virtual Adapter #3",
            "Hyper-V Virtual Switch Extension Adapter",
            "Microsoft AC Adapter",
            "Microsoft Kernel Debug Network Adapter",
            "USB-SERIAL CH340 (COM5)",
        ] {
            assert_eq!(kind_from_device_name(name), None, "{name}");
        }
        assert_eq!(
            kind_from_device_name("STMicroelectronics STLink USB device"),
            Some(ProbeKind::StLink)
        );
        assert_eq!(
            kind_from_device_name("CMSIS-DAP v2 Interface"),
            Some(ProbeKind::CmsisDap)
        );
    }

    #[test]
    fn the_uvoptx_sits_beside_the_project() {
        assert_eq!(
            uvoptx_path(Path::new("MDK-ARM/TX.uvprojx")),
            PathBuf::from("MDK-ARM/TX.uvoptx")
        );
    }
}
