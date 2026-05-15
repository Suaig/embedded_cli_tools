use std::path::Path;

use anyhow::{Context, ensure};
use roxmltree::Node;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct KeilProject {
    pub schema_version: String,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Target {
    pub name: String,
    pub toolset_name: String,
    pub toolset_number: String,
    pub ac6: bool,
    pub pcc: String,
    pub include_in_build: bool,
    pub device: DeviceInfo,
    pub output: OutputInfo,
    pub c_compiler: CCompilerInfo,
    pub assembler: AssemblerInfo,
    pub linker: LinkerInfo,
    pub memory: MemoryInfo,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub vendor: String,
    pub pack_id: String,
    pub cpu: String,
    pub svd_file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputInfo {
    pub name: String,
    pub directory: String,
    pub create_executable: bool,
    pub create_hex: bool,
    pub debug_information: bool,
    pub browse_information: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CCompilerInfo {
    pub optimization: u8,
    pub optimize_time: bool,
    pub c99: bool,
    pub gnu: bool,
    pub warning_level: u8,
    pub one_elf: bool,
    pub strict: bool,
    pub lang: u8,
    pub lang_profile: u8,
    pub short_enums: bool,
    pub short_wchar: bool,
    pub ropi: bool,
    pub rwpi: bool,
    pub defines: Vec<String>,
    pub include_paths: Vec<String>,
    pub misc_controls: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssemblerInfo {
    pub defines: Vec<String>,
    pub include_paths: Vec<String>,
    pub misc_controls: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkerInfo {
    pub scatter_file: String,
    pub libs: Vec<String>,
    pub lib_paths: Vec<String>,
    pub misc: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryInfo {
    pub irom: MemoryRegion,
    pub iram: MemoryRegion,
    pub xram: MemoryRegion,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryRegion {
    pub start: String,
    pub size: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub name: String,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub file_type: u8,
    pub path: String,
    pub included_in_build: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeilWorkspace {
    pub projects: Vec<WorkspaceProject>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceProject {
    pub path: String,
    pub is_active: bool,
    pub is_expanded: bool,
    pub checked_in_batch_build: bool,
}

// ---------------------------------------------------------------------------
// XML helper utilities
// ---------------------------------------------------------------------------

fn text_of<'a>(node: Node<'a, 'a>, tag: &str) -> String {
    node.children()
        .find(|c| c.has_tag_name(tag))
        .and_then(|c| c.text())
        .unwrap_or("")
        .to_string()
}

fn bool_of<'a>(node: Node<'a, 'a>, tag: &str) -> bool {
    let text = text_of(node, tag);
    text == "1"
}

fn child<'a>(node: Node<'a, 'a>, tag: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|c| c.has_tag_name(tag))
}

fn parse_comma_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn parse_semicolon_list(s: &str) -> Vec<String> {
    s.split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn parse_memory_region(parent: Node, tag: &str) -> MemoryRegion {
    if let Some(m) = child(parent, tag) {
        MemoryRegion {
            start: text_of(m, "StartAddress"),
            size: text_of(m, "Size"),
        }
    } else {
        MemoryRegion {
            start: String::new(),
            size: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Target parsing
// ---------------------------------------------------------------------------

fn parse_target(target_node: Node) -> Target {
    let name = text_of(target_node, "TargetName");
    let toolset_name = text_of(target_node, "ToolsetName");
    let toolset_number = text_of(target_node, "ToolsetNumber");
    let ac6 = bool_of(target_node, "uAC6");
    let pcc = text_of(target_node, "pCCUsed");

    let target_option = child(target_node, "TargetOption");

    // include_in_build
    let include_in_build = target_option
        .and_then(|to| child(to, "CommonProperty"))
        .map(|cp| bool_of(cp, "IncludeInBuild"))
        .unwrap_or(true);

    // device info
    let device = target_option
        .and_then(|to| child(to, "TargetCommonOption"))
        .map(|tco| DeviceInfo {
            name: text_of(tco, "Device"),
            vendor: text_of(tco, "Vendor"),
            pack_id: text_of(tco, "PackID"),
            cpu: text_of(tco, "Cpu"),
            svd_file: text_of(tco, "SFDFile"),
        })
        .unwrap_or(DeviceInfo {
            name: String::new(),
            vendor: String::new(),
            pack_id: String::new(),
            cpu: String::new(),
            svd_file: String::new(),
        });

    // output info
    let output = target_option
        .and_then(|to| child(to, "TargetCommonOption"))
        .map(|tco| OutputInfo {
            name: text_of(tco, "OutputName"),
            directory: text_of(tco, "OutputDirectory"),
            create_executable: bool_of(tco, "CreateExecutable"),
            create_hex: bool_of(tco, "CreateHexFile"),
            debug_information: bool_of(tco, "DebugInformation"),
            browse_information: bool_of(tco, "BrowseInformation"),
        })
        .unwrap_or(OutputInfo {
            name: String::new(),
            directory: String::new(),
            create_executable: false,
            create_hex: false,
            debug_information: false,
            browse_information: false,
        });

    // ArmAds section
    let arm_ads = target_option.and_then(|to| child(to, "TargetArmAds"));
    let cads = arm_ads.and_then(|aa| child(aa, "Cads"));
    let aads = arm_ads.and_then(|aa| child(aa, "Aads"));
    let ldads = arm_ads.and_then(|aa| child(aa, "LDads"));

    // C compiler
    let c_various = cads.and_then(|c| child(c, "VariousControls"));
    let c_compiler = CCompilerInfo {
        optimization: cads
            .map(|c| text_of(c, "Optim").parse::<u8>().unwrap_or(0))
            .unwrap_or(0),
        optimize_time: cads.map(|c| bool_of(c, "oTime")).unwrap_or(false),
        c99: cads.map(|c| bool_of(c, "uC99")).unwrap_or(false),
        gnu: cads.map(|c| bool_of(c, "uGnu")).unwrap_or(false),
        warning_level: cads
            .map(|c| text_of(c, "wLevel").parse::<u8>().unwrap_or(0))
            .unwrap_or(0),
        one_elf: cads.map(|c| bool_of(c, "OneElfS")).unwrap_or(false),
        strict: cads.map(|c| bool_of(c, "Strict")).unwrap_or(false),
        lang: cads
            .map(|c| text_of(c, "v6Lang").parse::<u8>().unwrap_or(0))
            .unwrap_or(0),
        lang_profile: cads
            .map(|c| text_of(c, "v6LangP").parse::<u8>().unwrap_or(0))
            .unwrap_or(0),
        short_enums: cads.map(|c| bool_of(c, "vShortEn")).unwrap_or(false),
        short_wchar: cads.map(|c| bool_of(c, "vShortWch")).unwrap_or(false),
        ropi: cads.map(|c| bool_of(c, "Ropi")).unwrap_or(false),
        rwpi: cads.map(|c| bool_of(c, "Rwpi")).unwrap_or(false),
        defines: c_various
            .as_ref()
            .map(|v| parse_comma_list(&text_of(*v, "Define")))
            .unwrap_or_default(),
        include_paths: c_various
            .as_ref()
            .map(|v| parse_semicolon_list(&text_of(*v, "IncludePath")))
            .unwrap_or_default(),
        misc_controls: c_various
            .as_ref()
            .map(|v| text_of(*v, "MiscControls"))
            .unwrap_or_default(),
    };

    // Assembler
    let a_various = aads.and_then(|a| child(a, "VariousControls"));
    let assembler = AssemblerInfo {
        defines: a_various
            .as_ref()
            .map(|v| parse_comma_list(&text_of(*v, "Define")))
            .unwrap_or_default(),
        include_paths: a_various
            .as_ref()
            .map(|v| parse_semicolon_list(&text_of(*v, "IncludePath")))
            .unwrap_or_default(),
        misc_controls: a_various
            .as_ref()
            .map(|v| text_of(*v, "MiscControls"))
            .unwrap_or_default(),
    };

    // Linker
    let linker = LinkerInfo {
        scatter_file: ldads
            .as_ref()
            .map(|ld| text_of(*ld, "ScatterFile"))
            .unwrap_or_default(),
        libs: ldads
            .as_ref()
            .map(|ld| {
                let raw = text_of(*ld, "IncludeLibs");
                parse_semicolon_list(&raw)
            })
            .unwrap_or_default(),
        lib_paths: ldads
            .as_ref()
            .map(|ld| {
                let raw = text_of(*ld, "IncludeLibsPath");
                parse_semicolon_list(&raw)
            })
            .unwrap_or_default(),
        misc: ldads
            .as_ref()
            .map(|ld| text_of(*ld, "Misc"))
            .unwrap_or_default(),
    };

    // Memory
    let arm_ads_misc = arm_ads.and_then(|aa| child(aa, "ArmAdsMisc"));
    let on_chip = arm_ads_misc.and_then(|m| child(m, "OnChipMemories"));
    let memory = MemoryInfo {
        irom: on_chip
            .map(|oc| parse_memory_region(oc, "IROM"))
            .unwrap_or(MemoryRegion {
                start: String::new(),
                size: String::new(),
            }),
        iram: on_chip
            .map(|oc| parse_memory_region(oc, "IRAM"))
            .unwrap_or(MemoryRegion {
                start: String::new(),
                size: String::new(),
            }),
        xram: on_chip
            .map(|oc| parse_memory_region(oc, "XRAM"))
            .unwrap_or(MemoryRegion {
                start: String::new(),
                size: String::new(),
            }),
    };

    // Groups
    let groups = parse_groups(target_node);

    Target {
        name,
        toolset_name,
        toolset_number,
        ac6,
        pcc,
        include_in_build,
        device,
        output,
        c_compiler,
        assembler,
        linker,
        memory,
        groups,
    }
}

fn parse_groups(target_node: Node) -> Vec<Group> {
    let groups_node = match child(target_node, "Groups") {
        Some(g) => g,
        None => return Vec::new(),
    };

    groups_node
        .children()
        .filter(|c| c.has_tag_name("Group"))
        .map(|g| {
            let name = text_of(g, "GroupName");
            let files = parse_files(g);
            Group { name, files }
        })
        .collect()
}

fn parse_files(group_node: Node) -> Vec<FileEntry> {
    let files_node = match child(group_node, "Files") {
        Some(f) => f,
        None => return Vec::new(),
    };

    files_node
        .children()
        .filter(|c| c.has_tag_name("File"))
        .map(|f| {
            let included = match f
                .children()
                .find(|c| c.has_tag_name("IncludeInBuild"))
                .and_then(|c| c.text())
            {
                Some("0") => false,
                _ => true,
            };
            FileEntry {
                name: text_of(f, "FileName"),
                file_type: text_of(f, "FileType")
                    .parse::<u8>()
                    .unwrap_or(1),
                path: text_of(f, "FilePath"),
                included_in_build: included,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Workspace parsing
// ---------------------------------------------------------------------------

fn parse_workspace_node(doc: &roxmltree::Document) -> KeilWorkspace {
    let root = doc.root();
    let pw = root
        .children()
        .find(|c| c.has_tag_name("ProjectWorkspace"))
        .unwrap_or(root);

    let projects = pw
        .children()
        .filter(|c| c.has_tag_name("project"))
        .map(|p| WorkspaceProject {
            path: text_of(p, "PathAndName"),
            is_active: bool_of(p, "NodeIsActive"),
            is_expanded: bool_of(p, "NodeIsExpanded"),
            checked_in_batch_build: bool_of(p, "NodeIsCheckedInBatchBuild"),
        })
        .collect();

    KeilWorkspace { projects }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn parse_project(content: &str) -> anyhow::Result<KeilProject> {
    let doc = roxmltree::Document::parse(content)
        .with_context(|| "failed to parse .uvprojx XML")?;

    let project_node = doc
        .root()
        .children()
        .find(|c| c.has_tag_name("Project"))
        .ok_or_else(|| anyhow::anyhow!("no <Project> root element found"))?;

    let schema_version = project_node
        .attribute("SchemaVersion")
        .unwrap_or("")
        .to_string();

    let targets_node = child(project_node, "Targets").unwrap_or(project_node);

    let targets: Vec<Target> = targets_node
        .children()
        .filter(|c| c.has_tag_name("Target"))
        .map(parse_target)
        .collect();

    ensure!(!targets.is_empty(), "no <Target> elements found in project");

    Ok(KeilProject {
        schema_version,
        targets,
    })
}

pub fn parse_workspace(content: &str) -> anyhow::Result<KeilWorkspace> {
    let doc = roxmltree::Document::parse(content)
        .with_context(|| "failed to parse .uvmpw XML")?;
    Ok(parse_workspace_node(&doc))
}

pub fn load_project(path: &Path) -> anyhow::Result<KeilProject> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read project file: {}", path.display()))?;
    parse_project(&content)
}

pub fn load_workspace(path: &Path) -> anyhow::Result<KeilWorkspace> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read workspace file: {}", path.display()))?;
    parse_workspace(&content)
}

pub fn is_workspace_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("uvmpw"))
        .unwrap_or(false)
}

pub fn is_project_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("uvprojx"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_project() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Project SchemaVersion="1.0">
  <Targets>
    <Target>
      <TargetName>TestTarget</TargetName>
      <ToolsetNumber>0x4</ToolsetNumber>
      <ToolsetName>ARM-ADS</ToolsetName>
      <uAC6>1</uAC6>
      <pCCUsed>5060528</pCCUsed>
      <TargetOption>
        <TargetCommonOption>
          <Device>STM32H743VITx</Device>
          <Vendor>STMicroelectronics</Vendor>
          <PackID>Keil.STM32H7xx_DFP.4.0.0</PackID>
          <Cpu>IRAM(0x20000000,0x00020000)</Cpu>
          <OutputDirectory>.\Objects\</OutputDirectory>
          <OutputName>test_out</OutputName>
          <CreateExecutable>1</CreateExecutable>
          <CreateHexFile>1</CreateHexFile>
          <DebugInformation>1</DebugInformation>
          <BrowseInformation>0</BrowseInformation>
          <SFDFile>some.svd</SFDFile>
        </TargetCommonOption>
        <CommonProperty>
          <IncludeInBuild>1</IncludeInBuild>
        </CommonProperty>
        <TargetArmAds>
          <ArmAdsMisc>
            <OnChipMemories>
              <IROM><StartAddress>0x8000000</StartAddress><Size>0x100000</Size></IROM>
              <IRAM><StartAddress>0x20000000</StartAddress><Size>0x20000</Size></IRAM>
              <XRAM><StartAddress>0x30000000</StartAddress><Size>0x48000</Size></XRAM>
            </OnChipMemories>
          </ArmAdsMisc>
          <Cads>
            <Optim>2</Optim>
            <oTime>0</oTime>
            <uC99>1</uC99>
            <uGnu>1</uGnu>
            <wLevel>3</wLevel>
            <OneElfS>1</OneElfS>
            <Strict>0</Strict>
            <v6Lang>3</v6Lang>
            <v6LangP>5</v6LangP>
            <vShortEn>1</vShortEn>
            <vShortWch>1</vShortWch>
            <Ropi>0</Ropi>
            <Rwpi>0</Rwpi>
            <VariousControls>
              <MiscControls>--diag_suppress=1</MiscControls>
              <Define>USE_HAL_DRIVER,STM32H743xx</Define>
              <IncludePath>../Core/Inc;../Drivers/CMSIS/Include</IncludePath>
            </VariousControls>
          </Cads>
          <Aads>
            <VariousControls>
              <Define>ASM_DEF</Define>
              <IncludePath>../asm/inc</IncludePath>
              <MiscControls></MiscControls>
            </VariousControls>
          </Aads>
          <LDads>
            <ScatterFile>linker.sct</ScatterFile>
            <IncludeLibs>lib1.a;lib2.a</IncludeLibs>
            <IncludeLibsPath>../libs</IncludeLibsPath>
            <Misc>--map</Misc>
          </LDads>
        </TargetArmAds>
      </TargetOption>
      <Groups>
        <Group>
          <GroupName>Application/User/Core</GroupName>
          <Files>
            <File>
              <FileName>main.c</FileName>
              <FileType>1</FileType>
              <FilePath>../Core/Src/main.c</FilePath>
            </File>
            <File>
              <FileName>excluded.c</FileName>
              <FileType>1</FileType>
              <FilePath>../Core/Src/excluded.c</FilePath>
              <IncludeInBuild>0</IncludeInBuild>
            </File>
          </Files>
        </Group>
        <Group>
          <GroupName>Drivers</GroupName>
          <Files>
            <File>
              <FileName>stm32h7xx_hal.c</FileName>
              <FileType>1</FileType>
              <FilePath>../Drivers/Src/stm32h7xx_hal.c</FilePath>
            </File>
          </Files>
        </Group>
      </Groups>
    </Target>
  </Targets>
</Project>"#;

        let proj = parse_project(xml).unwrap();
        assert_eq!(proj.schema_version, "1.0");
        assert_eq!(proj.targets.len(), 1);

        let t = &proj.targets[0];
        assert_eq!(t.name, "TestTarget");
        assert_eq!(t.toolset_number, "0x4");
        assert_eq!(t.toolset_name, "ARM-ADS");
        assert!(t.ac6);
        assert_eq!(t.pcc, "5060528");
        assert!(t.include_in_build);

        // device
        assert_eq!(t.device.name, "STM32H743VITx");
        assert_eq!(t.device.vendor, "STMicroelectronics");
        assert_eq!(t.device.pack_id, "Keil.STM32H7xx_DFP.4.0.0");
        assert_eq!(t.device.svd_file, "some.svd");

        // output
        assert_eq!(t.output.name, "test_out");
        assert!(t.output.create_executable);
        assert!(t.output.create_hex);
        assert!(!t.output.browse_information);

        // c compiler
        assert_eq!(t.c_compiler.optimization, 2);
        assert!(!t.c_compiler.optimize_time);
        assert!(t.c_compiler.c99);
        assert!(t.c_compiler.gnu);
        assert_eq!(t.c_compiler.warning_level, 3);
        assert!(t.c_compiler.one_elf);
        assert!(!t.c_compiler.strict);
        assert_eq!(t.c_compiler.lang, 3);
        assert_eq!(t.c_compiler.lang_profile, 5);
        assert!(t.c_compiler.short_enums);
        assert!(t.c_compiler.short_wchar);
        assert!(!t.c_compiler.ropi);
        assert!(!t.c_compiler.rwpi);
        assert_eq!(t.c_compiler.defines, vec!["USE_HAL_DRIVER", "STM32H743xx"]);
        assert_eq!(
            t.c_compiler.include_paths,
            vec!["../Core/Inc", "../Drivers/CMSIS/Include"]
        );
        assert_eq!(t.c_compiler.misc_controls, "--diag_suppress=1");

        // assembler
        assert_eq!(t.assembler.defines, vec!["ASM_DEF"]);
        assert_eq!(t.assembler.include_paths, vec!["../asm/inc"]);

        // linker
        assert_eq!(t.linker.scatter_file, "linker.sct");
        assert_eq!(t.linker.libs, vec!["lib1.a", "lib2.a"]);
        assert_eq!(t.linker.misc, "--map");

        // memory
        assert_eq!(t.memory.irom.start, "0x8000000");
        assert_eq!(t.memory.irom.size, "0x100000");
        assert_eq!(t.memory.iram.start, "0x20000000");
        assert_eq!(t.memory.iram.size, "0x20000");
        assert_eq!(t.memory.xram.start, "0x30000000");
        assert_eq!(t.memory.xram.size, "0x48000");

        // groups
        assert_eq!(t.groups.len(), 2);
        assert_eq!(t.groups[0].name, "Application/User/Core");
        assert_eq!(t.groups[0].files.len(), 2);
        assert_eq!(t.groups[0].files[0].name, "main.c");
        assert_eq!(t.groups[0].files[0].file_type, 1);
        assert!(t.groups[0].files[0].included_in_build);
        assert!(!t.groups[0].files[1].included_in_build);
        assert_eq!(t.groups[1].name, "Drivers");
        assert_eq!(t.groups[1].files.len(), 1);
    }

    #[test]
    fn test_parse_workspace() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ProjectWorkspace>
  <project>
    <PathAndName>.\Boot\Test_Boot.uvprojx</PathAndName>
    <NodeIsActive>1</NodeIsActive>
    <NodeIsExpanded>1</NodeIsExpanded>
    <NodeIsCheckedInBatchBuild>1</NodeIsCheckedInBatchBuild>
  </project>
  <project>
    <PathAndName>.\Appli\Test_Appli.uvprojx</PathAndName>
  </project>
</ProjectWorkspace>"#;

        let ws = parse_workspace(xml).unwrap();
        assert_eq!(ws.projects.len(), 2);

        assert_eq!(ws.projects[0].path, ".\\Boot\\Test_Boot.uvprojx");
        assert!(ws.projects[0].is_active);
        assert!(ws.projects[0].is_expanded);
        assert!(ws.projects[0].checked_in_batch_build);

        assert_eq!(ws.projects[1].path, ".\\Appli\\Test_Appli.uvprojx");
        assert!(!ws.projects[1].is_active);
        assert!(!ws.projects[1].is_expanded);
        assert!(!ws.projects[1].checked_in_batch_build);
    }

    #[test]
    fn test_file_type_checkers() {
        assert!(is_project_file(Path::new("test.uvprojx")));
        assert!(is_project_file(Path::new("test.UVPROJX")));
        assert!(!is_project_file(Path::new("test.uvmpw")));

        assert!(is_workspace_file(Path::new("test.uvmpw")));
        assert!(!is_workspace_file(Path::new("test.uvprojx")));
    }

    #[test]
    fn test_missing_nodes_default_gracefully() {
        let xml = r#"<Project SchemaVersion="2.0">
  <Targets>
    <Target>
      <TargetName>Minimal</TargetName>
    </Target>
  </Targets>
</Project>"#;

        let proj = parse_project(xml).unwrap();
        let t = &proj.targets[0];
        assert_eq!(t.name, "Minimal");
        assert_eq!(t.toolset_name, "");
        assert!(!t.ac6);
        assert_eq!(t.pcc, "");
        assert!(t.include_in_build);
        assert_eq!(t.device.name, "");
        assert_eq!(t.c_compiler.defines.len(), 0);
        assert_eq!(t.groups.len(), 0);
    }
}
