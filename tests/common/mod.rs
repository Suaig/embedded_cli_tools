//! Shared fixtures for the CLI-level integration tests.
//!
//! These drive the real binary through `CARGO_BIN_EXE_emb` and deliberately use
//! minimal hand-written projects so that no Keil installation or toolchain is
//! required.
//!
//! Not every test binary uses every helper, so silence the per-binary dead-code
//! warnings.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// A single-target .uvprojx, small enough to keep in the test source.
pub fn project_xml(target: &str, define: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Project>
  <SchemaVersion>2.1</SchemaVersion>
  <Targets>
    <Target>
      <TargetName>{target}</TargetName>
      <ToolsetNumber>0x4</ToolsetNumber>
      <ToolsetName>ARM-ADS</ToolsetName>
      <TargetOption>
        <TargetCommonOption>
          <Device>STM32H7R7L8Hx</Device>
          <OutputName>{target}</OutputName>
          <OutputDirectory>.\Objects\</OutputDirectory>
          <CreateExecutable>1</CreateExecutable>
          <CreateHexFile>1</CreateHexFile>
        </TargetCommonOption>
        <CommonProperty>
          <IncludeInBuild>1</IncludeInBuild>
        </CommonProperty>
        <TargetArmAds>
          <Cads>
            <Optim>0</Optim>
            <VariousControls>
              <Define>{define}</Define>
              <IncludePath>..\Core\Inc</IncludePath>
            </VariousControls>
          </Cads>
        </TargetArmAds>
      </TargetOption>
      <Groups>
        <Group>
          <GroupName>Application</GroupName>
          <Files>
            <File>
              <FileName>main.c</FileName>
              <FileType>1</FileType>
              <FilePath>..\Core\Src\main.c</FilePath>
            </File>
          </Files>
        </Group>
      </Groups>
    </Target>
  </Targets>
</Project>
"#
    )
}

/// A scratch directory unique to this test binary.
fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("emb_cli_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write the single-project fixture and return its path.
pub fn write_fixture(name: &str) -> PathBuf {
    let path = scratch_dir().join(name);
    std::fs::write(&path, project_xml("OnlyTarget", "USE_HAL_DRIVER")).unwrap();
    path
}

/// Write arbitrary content into this test binary's scratch directory.
pub fn write_scratch(name: &str, contents: &str) -> PathBuf {
    let path = scratch_dir().join(name);
    std::fs::write(&path, contents).unwrap();
    path
}

/// Write a minimal armlink map: entry point, Grand Totals, the three published
/// totals, and one execution region.
pub fn write_map_fixture(name: &str) -> PathBuf {
    write_scratch(
        name,
        "\
  Image Entry point : 0x08000000
    40828       4176       5408         40      14860     415344   Grand Totals
    Total RO  Size (Code + RO Data)                46236 (  45.15kB)
    Total RW  Size (RW Data + ZI Data)             14900 (  14.55kB)
    Total ROM Size (Code + RO Data + RW Data)      46276 (  45.19kB)
    Execution Region ER_IROM1 (Exec base: 0x08000000, Load base: 0x08000000, Size: 0x0000b49c, Max: 0x00100000, ABSOLUTE)
",
    )
}

/// Write a map that carries a symbol table, for `keil map --lookup`.
///
/// Rows are copied from a real armlink map so the shapes that are easy to get
/// wrong stay covered: a `Thumb Code` function whose value has bit 0 set, a
/// `Section` marker sitting at the same address as that function, a name with a
/// space in it, and a linker-generated object with a space and no parentheses.
pub fn write_symbol_map_fixture(name: &str) -> PathBuf {
    write_scratch(
        name,
        "\
  Image Entry point : 0x90001000

    Local Symbols

    Symbol Name                              Value     Ov Type        Size  Object(Section)

    etharp_raw                               0x90035399   Thumb Code   360  etharp.o(.text.etharp_raw)
    [Anonymous Symbol]                       0x90035398   Section        0  etharp.o(.text.etharp_raw)

==============================================================================

    Global Symbols

    Symbol Name                              Value     Ov Type        Size  Object(Section)

    Image$$ER_SYS_INIT$$Base                 0x90092948   Number         0  anon$$obj.o ABSOLUTE
",
    )
}

/// Write a two-project workspace and return the `.uvmpw` path.///
/// The sub-projects have different target names and different defines, so a
/// test can tell which one `-p <project>` actually selected.
pub fn write_workspace(name: &str) -> PathBuf {
    let root = scratch_dir().join(name);
    let _ = std::fs::remove_dir_all(&root);

    for (sub, project, target, define) in [
        ("SubA", "a.uvprojx", "TargetA", "DEFINE_A"),
        ("SubB", "b.uvprojx", "TargetB", "DEFINE_B"),
    ] {
        let dir = root.join(sub);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(project), project_xml(target, define)).unwrap();
    }

    let ws = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<ProjectWorkspace xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <SchemaVersion>1.0</SchemaVersion>
  <Header>### uVision Project, (C) Keil Software</Header>
  <WorkspaceName>{}</WorkspaceName>
  <project>
    <PathAndName>.\SubA\a.uvprojx</PathAndName>
    <NodeIsCheckedInBatchBuild>1</NodeIsCheckedInBatchBuild>
  </project>
  <project>
    <PathAndName>.\SubB\b.uvprojx</PathAndName>
    <NodeIsActive>1</NodeIsActive>
    <NodeIsCheckedInBatchBuild>1</NodeIsCheckedInBatchBuild>
  </project>
</ProjectWorkspace>
"#,
        root.join(name).display()
    );

    let ws_path = root.join(name);
    std::fs::write(&ws_path, ws).unwrap();
    ws_path
}

/// Run the `emb` binary under test.
pub fn emb(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_emb"))
        .args(args)
        .output()
        .expect("failed to run emb")
}

/// Run `emb` with `cwd`, returning `(combined stdout+stderr, success)`.
pub fn emb_in(dir: &Path, args: &[&str]) -> (String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_emb"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to run emb");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (text, out.status.success())
}
