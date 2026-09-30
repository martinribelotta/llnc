use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid YAML in {path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("invalid project manifest: {0}")]
    Manifest(String),
    #[error("invalid Markdown module {path}: {message}")]
    Module { path: PathBuf, message: String },
    #[error("duplicate module id `{id}` in {first} and {second}")]
    DuplicateModule {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },
    #[error("module `{module}` depends on missing module `{dependency}`")]
    MissingDependency { module: String, dependency: String },
    #[error("dependency cycle: {0}")]
    DependencyCycle(String),
    #[error("module `{0}` is not declared as a program")]
    NotProgram(String),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    project: Project,
    target: PathBuf,
    program: String,
    source_roots: Vec<PathBuf>,
    #[serde(default)]
    run: RunConfig,
}

#[derive(Debug, Deserialize)]
struct Project {
    name: String,
}

#[derive(Debug, Deserialize)]
struct TargetConfig {
    id: String,
    architecture: String,
    abi: String,
    toolchain: ToolchainConfig,
    emulator: EmulatorConfig,
    linker_script: PathBuf,
    startup: PathBuf,
}

#[derive(Debug, Deserialize)]
struct ToolchainConfig {
    assembler: String,
    assembler_args: Vec<String>,
    linker: String,
    linker_args: Vec<String>,
    objcopy: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EmulatorConfig {
    executable: String,
    args: Vec<String>,
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct RunConfig {
    expected_output: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModuleMetadata {
    id: String,
    kind: String,
    #[serde(default)]
    depends_on: Vec<String>,
}

#[derive(Debug)]
struct Module {
    path: PathBuf,
    metadata: ModuleMetadata,
    content: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub id: String,
    pub kind: String,
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct Toolchain {
    pub assembler: String,
    pub assembler_args: Vec<String>,
    pub linker: String,
    pub linker_args: Vec<String>,
    pub objcopy: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Emulator {
    pub executable: String,
    pub args: Vec<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug)]
pub struct ProjectSummary {
    pub name: String,
    pub target_id: String,
    pub architecture: String,
    pub abi: String,
    pub target_config: PathBuf,
    pub module_order: Vec<String>,
    pub modules: Vec<ResolvedModule>,
    pub toolchain: Toolchain,
    pub emulator: Emulator,
    pub linker_script: PathBuf,
    pub startup: PathBuf,
    pub expected_output: Option<String>,
}

pub fn validate_project(
    manifest_path: impl AsRef<Path>,
) -> Result<ProjectSummary, ValidationError> {
    let manifest_path = manifest_path.as_ref();
    let manifest_text = read_to_string(manifest_path)?;
    let manifest: Manifest =
        serde_yaml::from_str(&manifest_text).map_err(|source| ValidationError::Yaml {
            path: manifest_path.to_path_buf(),
            source,
        })?;

    if manifest.project.name.trim().is_empty() {
        return Err(ValidationError::Manifest(
            "`project.name` must not be empty".into(),
        ));
    }

    let project_dir = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|source| ValidationError::Read {
            path: manifest_path.to_path_buf(),
            source,
        })?;

    let target_config = resolve_path(&project_dir, &manifest.target)?;
    let target_text = read_to_string(&target_config)?;
    let target: TargetConfig =
        serde_yaml::from_str(&target_text).map_err(|source| ValidationError::Yaml {
            path: target_config.clone(),
            source,
        })?;

    let mut modules = HashMap::new();
    for source_root in &manifest.source_roots {
        let source_root = resolve_path(&project_dir, source_root)?;
        if !source_root.is_dir() {
            return Err(ValidationError::Manifest(format!(
                "source root is not a directory: {}",
                source_root.display()
            )));
        }
        collect_modules(&source_root, &mut modules)?;
    }

    let program = modules.get(&manifest.program).ok_or_else(|| {
        ValidationError::Manifest(format!(
            "program module `{}` was not found in source_roots",
            manifest.program
        ))
    })?;
    if program.metadata.kind != "program" {
        return Err(ValidationError::NotProgram(manifest.program));
    }

    let module_order = resolve_dependencies(&modules, &manifest.program)?;
    let target_dir = target_config
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let resolved_modules = module_order
        .iter()
        .map(|id| {
            let module = &modules[id];
            ResolvedModule {
                id: id.clone(),
                kind: module.metadata.kind.clone(),
                path: module.path.clone(),
                content: module.content.clone(),
            }
        })
        .collect();
    Ok(ProjectSummary {
        name: manifest.project.name,
        target_id: target.id,
        architecture: target.architecture,
        abi: target.abi,
        target_config,
        module_order,
        modules: resolved_modules,
        toolchain: Toolchain {
            assembler: target.toolchain.assembler,
            assembler_args: target.toolchain.assembler_args,
            linker: target.toolchain.linker,
            linker_args: target.toolchain.linker_args,
            objcopy: target.toolchain.objcopy,
        },
        emulator: Emulator {
            executable: target.emulator.executable,
            args: target.emulator.args,
            timeout_seconds: target.emulator.timeout_seconds.unwrap_or(10),
        },
        linker_script: resolve_path(&target_dir, &target.linker_script)?,
        startup: resolve_path(&target_dir, &target.startup)?,
        expected_output: manifest.run.expected_output,
    })
}

fn resolve_path(base: &Path, path: &Path) -> Result<PathBuf, ValidationError> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let canonical = path
        .canonicalize()
        .map_err(|source| ValidationError::Read { path, source })?;
    #[cfg(windows)]
    {
        if let Some(stripped) = canonical
            .to_str()
            .and_then(|value| value.strip_prefix(r"\\?\"))
        {
            if let Some(unc_path) = stripped.strip_prefix("UNC\\") {
                return Ok(PathBuf::from(format!(r"\\{unc_path}")));
            }
            return Ok(PathBuf::from(stripped));
        }
    }
    Ok(canonical)
}

fn read_to_string(path: &Path) -> Result<String, ValidationError> {
    fs::read_to_string(path).map_err(|source| ValidationError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn collect_modules(
    root: &Path,
    modules: &mut HashMap<String, Module>,
) -> Result<(), ValidationError> {
    let entries = fs::read_dir(root).map_err(|source| ValidationError::Read {
        path: root.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| ValidationError::Read {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| ValidationError::Read {
            path: path.clone(),
            source,
        })?;

        if file_type.is_dir() {
            collect_modules(&path, modules)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "md") {
            let text = read_to_string(&path)?;
            let metadata = parse_module_metadata(&path, &text)?;
            let id = metadata.id.clone();
            if let Some(previous) = modules.get(&id) {
                return Err(ValidationError::DuplicateModule {
                    id,
                    first: previous.path.clone(),
                    second: path,
                });
            }
            modules.insert(
                id,
                Module {
                    path,
                    metadata,
                    content: text,
                },
            );
        }
    }
    Ok(())
}

fn parse_module_metadata(path: &Path, text: &str) -> Result<ModuleMetadata, ValidationError> {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err(ValidationError::Module {
            path: path.to_path_buf(),
            message: "expected YAML front matter beginning with `---`".into(),
        });
    }

    let mut yaml = String::new();
    let mut closed = false;
    for line in lines {
        if line == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    if !closed {
        return Err(ValidationError::Module {
            path: path.to_path_buf(),
            message: "unterminated YAML front matter".into(),
        });
    }

    let metadata: ModuleMetadata =
        serde_yaml::from_str(&yaml).map_err(|source| ValidationError::Module {
            path: path.to_path_buf(),
            message: source.to_string(),
        })?;
    if metadata.id.trim().is_empty() || metadata.kind.trim().is_empty() {
        return Err(ValidationError::Module {
            path: path.to_path_buf(),
            message: "`id` and `kind` must not be empty".into(),
        });
    }
    Ok(metadata)
}

fn resolve_dependencies(
    modules: &HashMap<String, Module>,
    program: &str,
) -> Result<Vec<String>, ValidationError> {
    fn visit(
        id: &str,
        modules: &HashMap<String, Module>,
        visiting: &mut Vec<String>,
        visited: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), ValidationError> {
        if visited.contains(id) {
            return Ok(());
        }
        if let Some(start) = visiting.iter().position(|item| item == id) {
            let mut cycle = visiting[start..].to_vec();
            cycle.push(id.to_string());
            return Err(ValidationError::DependencyCycle(cycle.join(" -> ")));
        }

        let module = modules
            .get(id)
            .ok_or_else(|| ValidationError::MissingDependency {
                module: visiting.last().cloned().unwrap_or_else(|| id.to_string()),
                dependency: id.to_string(),
            })?;
        visiting.push(id.to_string());
        for dependency in &module.metadata.depends_on {
            if !modules.contains_key(dependency) {
                return Err(ValidationError::MissingDependency {
                    module: id.to_string(),
                    dependency: dependency.clone(),
                });
            }
            visit(dependency, modules, visiting, visited, order)?;
        }
        visiting.pop();
        visited.insert(id.to_string());
        order.push(id.to_string());
        Ok(())
    }

    let mut order = Vec::new();
    visit(
        program,
        modules,
        &mut Vec::new(),
        &mut HashSet::new(),
        &mut order,
    )?;
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::{parse_module_metadata, validate_project};
    use std::path::Path;

    #[test]
    fn reads_module_front_matter() {
        let metadata = parse_module_metadata(
            Path::new("module.md"),
            "---\nid: algorithm.search\nkind: knowledge\ndepends_on:\n  - algorithm.compare\n---\n# Search\n",
        )
        .unwrap();

        assert_eq!(metadata.id, "algorithm.search");
        assert_eq!(metadata.kind, "knowledge");
        assert_eq!(metadata.depends_on, ["algorithm.compare"]);
    }

    #[test]
    fn resolves_the_serial_hello_project_and_target() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let summary = validate_project(workspace.join("examples/serial-hello/llnc.yaml")).unwrap();

        assert_eq!(summary.target_id, "target.riscv32-qemu-virt");
        assert_eq!(summary.architecture, "rv32imafdc");
        assert_eq!(summary.abi, "ilp32d");
        assert_eq!(summary.expected_output.as_deref(), Some("Hello from llnc!"));
        assert_eq!(
            summary.modules.last().map(|module| module.id.as_str()),
            Some("program.serial-hello")
        );
    }
}
