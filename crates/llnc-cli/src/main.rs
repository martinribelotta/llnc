use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use wait_timeout::ChildExt;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8080/v1";
const DEFAULT_MODEL: &str = "Qwen3-4B-Q4_K_M.gguf";
const MAX_ASSEMBLY_REPAIRS: usize = 3;

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<ChatTemplateOptions>,
}

#[derive(Debug, Serialize)]
struct ChatTemplateOptions {
    enable_thinking: bool,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("llnc: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let Some(command) = args.next() else {
        return Err(usage());
    };
    let Some(manifest) = args.next() else {
        return Err(usage());
    };
    let manifest = PathBuf::from(manifest);

    match command.to_string_lossy().as_ref() {
        "check" => {
            if args.next().is_some() {
                return Err(usage());
            }
            check(&manifest)
        }
        "build" => {
            let options = BuildOptions::parse(args)?;
            build(&manifest, options)
        }
        other => Err(format!("unknown command `{other}`\n{}", usage())),
    }
}

fn usage() -> String {
    "usage:\n  llnc check <project-manifest>\n  llnc build <project-manifest> [--generate-only] [--endpoint URL] [--model NAME] [--output-dir DIR]".into()
}

fn check(manifest: &Path) -> Result<(), String> {
    let summary = llnc_core::validate_project(manifest).map_err(|error| error.to_string())?;
    println!("Project: {}", summary.name);
    println!("Target:  {}", summary.target_id);
    println!("Config:  {}", summary.target_config.display());
    println!("Modules (dependency order):");
    for module in summary.modules {
        println!("  - {} ({})", module.id, module.kind);
    }
    println!("Project descriptions are valid.");
    Ok(())
}

struct BuildOptions {
    generate_only: bool,
    endpoint: String,
    model: String,
    output_dir: Option<PathBuf>,
}

impl BuildOptions {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Self, String> {
        let mut options = Self {
            generate_only: false,
            endpoint: env::var("LLNC_LLM_ENDPOINT")
                .unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string()),
            model: env::var("LLNC_LLM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
            output_dir: None,
        };
        let mut args = args;
        while let Some(arg) = args.next() {
            match arg.to_string_lossy().as_ref() {
                "--generate-only" => options.generate_only = true,
                "--endpoint" => {
                    options.endpoint = next_value(&mut args, "--endpoint")?
                        .to_string_lossy()
                        .into_owned();
                }
                "--model" => {
                    options.model = next_value(&mut args, "--model")?
                        .to_string_lossy()
                        .into_owned();
                }
                "--output-dir" => {
                    options.output_dir =
                        Some(PathBuf::from(next_value(&mut args, "--output-dir")?));
                }
                "--help" | "-h" => return Err(usage()),
                unknown => return Err(format!("unknown build option `{unknown}`\n{}", usage())),
            }
        }
        if options.endpoint.trim().is_empty() || options.model.trim().is_empty() {
            return Err("LLM endpoint and model name must not be empty".into());
        }
        Ok(options)
    }
}

fn next_value(args: &mut impl Iterator<Item = OsString>, option: &str) -> Result<OsString, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {option}"))
}

fn build(manifest_path: &Path, options: BuildOptions) -> Result<(), String> {
    let project = llnc_core::validate_project(manifest_path).map_err(|error| error.to_string())?;
    let output_dir = options
        .output_dir
        .unwrap_or_else(|| PathBuf::from("artifacts").join(safe_name(&project.name)));
    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("could not create {}: {error}", output_dir.display()))?;
    let output_dir = output_dir
        .canonicalize()
        .map_err(|error| format!("could not resolve artifact directory: {error}"))?;
    println!("Project: {} ({})", project.name, project.target_id);
    println!(
        "Target: {} / {}, manifest: {}",
        project.architecture,
        project.abi,
        manifest_path.display()
    );
    println!("Artifacts: {}", output_dir.display());
    println!("Modules (dependency order):");
    for module in &project.modules {
        println!("  - {} ({})", module.id, module.kind);
    }

    if !options.generate_only {
        println!("Checking required build tools...");
        require_tool(&project.toolchain.assembler)?;
        require_tool(&project.toolchain.linker)?;
        if let Some(objcopy) = &project.toolchain.objcopy {
            require_tool(objcopy)?;
        }
        require_tool(&project.emulator.executable)?;
        println!("All required tools are available.");
    } else {
        println!("Assembly generation only; assembler, linker, and emulator will be skipped.");
    }

    let prompt = create_prompt(&project);
    write_artifact(&output_dir, "prompt.txt", prompt.as_bytes())?;
    println!(
        "Resolved prompt: {} bytes across {} modules.",
        prompt.len(),
        project.modules.len()
    );
    let mut feedback = None;
    for attempt in 0..=MAX_ASSEMBLY_REPAIRS {
        println!(
            "\n=== Generation attempt {}/{} ===",
            attempt + 1,
            MAX_ASSEMBLY_REPAIRS + 1
        );
        let current_prompt = if let Some(feedback) = &feedback {
            format!(
                "{prompt}\n\nThe previous candidate failed validation:\n{feedback}\n\nCorrect the complete assembly. Preserve the requested behavior and output only GNU assembly."
            )
        } else {
            prompt.clone()
        };
        write_artifact(
            &output_dir,
            &format!("prompt-{}.txt", attempt + 1),
            current_prompt.as_bytes(),
        )?;
        println!(
            "Requesting assembly from {} using model `{}` (prompt: {} bytes)...",
            endpoint_label(&options.endpoint),
            options.model,
            current_prompt.len()
        );
        let generation_started = Instant::now();
        let candidate = request_assembly(&options.endpoint, &options.model, &current_prompt)?;
        println!(
            "Received {} bytes of assembly in {:.2}s.",
            candidate.len(),
            generation_started.elapsed().as_secs_f64()
        );
        write_artifact(&output_dir, "generated.s", candidate.as_bytes())?;
        println!(
            "Saved generated source: {}",
            output_dir.join("generated.s").display()
        );
        let comments = generated_comments(&candidate);
        if comments.is_empty() {
            println!("LLM comments: none.");
        } else {
            println!("LLM comments:\n{comments}");
        }
        if let Err(error) = validate_assembly_entry(&candidate) {
            if attempt == MAX_ASSEMBLY_REPAIRS {
                return Err(format!(
                    "LLM output did not satisfy the assembly contract after {MAX_ASSEMBLY_REPAIRS} corrections: {error}"
                ));
            }
            eprintln!(
                "Assembly contract check failed: {error}\nRequesting correction {}/{}...",
                attempt + 1,
                MAX_ASSEMBLY_REPAIRS
            );
            feedback = Some(error);
            continue;
        }
        println!("Assembly entry-point contract validated.");

        if options.generate_only {
            println!(
                "Generated assembly (syntax and behavior not yet verified): {}",
                output_dir.join("generated.s").display()
            );
            return Ok(());
        }

        println!("Assembling startup and generated program...");
        let assembly_started = Instant::now();
        if let Some(error) = assemble(&project, &output_dir)? {
            if attempt == MAX_ASSEMBLY_REPAIRS {
                return Err(format!(
                    "assembly failed after {MAX_ASSEMBLY_REPAIRS} corrections:\n{error}"
                ));
            }
            eprintln!(
                "GNU assembler rejected the generated program:\n{error}\nRequesting correction {}/{}...",
                attempt + 1,
                MAX_ASSEMBLY_REPAIRS
            );
            feedback = Some(format!(
                "Previous assembly:\n```asm\n{candidate}\n```\nGNU assembler diagnostics:\n{error}"
            ));
            continue;
        }
        println!(
            "Assembly completed in {:.2}s.",
            assembly_started.elapsed().as_secs_f64()
        );

        println!("Linking firmware...");
        let link_started = Instant::now();
        if let Err(error) = link(&project, &output_dir) {
            if attempt == MAX_ASSEMBLY_REPAIRS {
                return Err(error);
            }
            eprintln!(
                "{error}\nRequesting correction {}/{}...",
                attempt + 1,
                MAX_ASSEMBLY_REPAIRS
            );
            feedback = Some(format!(
                "Previous assembly:\n```asm\n{candidate}\n```\n{error}"
            ));
            continue;
        }
        println!(
            "Linking completed in {:.2}s.",
            link_started.elapsed().as_secs_f64()
        );

        println!(
            "Running {} (timeout: {}s)...",
            project.emulator.executable, project.emulator.timeout_seconds
        );
        let emulator_started = Instant::now();
        let output = match run_emulator(&project, &output_dir) {
            Ok(output) => output,
            Err(error) => {
                if attempt == MAX_ASSEMBLY_REPAIRS {
                    return Err(error);
                }
                eprintln!(
                    "{error}\nRequesting correction {}/{}...",
                    attempt + 1,
                    MAX_ASSEMBLY_REPAIRS
                );
                feedback = Some(format!(
                    "Previous assembly:\n```asm\n{candidate}\n```\n{error}"
                ));
                continue;
            }
        };
        println!(
            "Emulator completed in {:.2}s; captured {} bytes.",
            emulator_started.elapsed().as_secs_f64(),
            output.len()
        );
        let output_text = String::from_utf8_lossy(&output);
        let output_text = output_text.trim();
        if let Some(expected) = &project.expected_output {
            if !output_text.contains(expected) {
                let error = format!(
                    "QEMU output did not contain expected text {expected:?}; captured output: {output_text:?}"
                );
                if attempt == MAX_ASSEMBLY_REPAIRS {
                    return Err(format!(
                        "{error}. See {}",
                        output_dir.join("qemu.log").display()
                    ));
                }
                eprintln!(
                    "{error}\nRequesting correction {}/{}...",
                    attempt + 1,
                    MAX_ASSEMBLY_REPAIRS
                );
                feedback = Some(format!(
                    "Previous assembly:\n```asm\n{candidate}\n```\n{error}"
                ));
                continue;
            }
            println!("Expected output check passed: {expected:?}");
        }

        println!("QEMU output:\n{output_text}");
        println!("Build succeeded. Artifacts: {}", output_dir.display());
        return Ok(());
    }
    Err("build did not produce a validated firmware image".into())
}

fn endpoint_label(endpoint: &str) -> String {
    reqwest::Url::parse(endpoint)
        .map(|url| {
            let host = url.host_str().unwrap_or("<unknown host>");
            match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            }
        })
        .unwrap_or_else(|_| "<invalid endpoint>".into())
}

fn generated_comments(assembly: &str) -> String {
    assembly
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
}

fn safe_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();
    sanitized.trim_matches('-').to_string()
}

fn write_artifact(directory: &Path, name: &str, content: &[u8]) -> Result<(), String> {
    let path = directory.join(name);
    fs::write(&path, content)
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn create_prompt(project: &llnc_core::ProjectSummary) -> String {
    let mut prompt = format!(
        "You are the code-generation backend of llnc. The user's natural-language Markdown descriptions are the source program. Generate GNU assembly for target {} using architecture {} and ABI {}. Use non-thinking mode (/no_think).\n\nReturn only complete GNU assembler source defining a global `main` function. Do not define `_start`, do not emit C or Rust, do not use markdown fences, and do not add prose outside the assembly. Add concise GNU assembler comments that explain the purpose of each logical block, important register roles, and non-obvious platform or algorithm assumptions. Do not add comments for obvious individual instructions. `main` must return with `ret` when its requested work is complete; the provided startup code handles process completion. Use only the described platform resources and satisfy the program requirements.\n\n",
        project.target_id, project.architecture, project.abi
    );
    for module in &project.modules {
        prompt.push_str(&format!(
            "\n===== MODULE {} ({}) =====\n{}\n",
            module.id, module.kind, module.content
        ));
    }
    prompt.push_str(
        "\n===== ASSEMBLY REQUIREMENTS =====\n\
Use GNU assembler syntax accepted by the selected target. Comments must use GNU assembler comment syntax and must not replace executable instructions. Do not assume a C runtime, libc, syscalls, or initialized global data. Do not use pseudo-operations requiring external symbols. Return from `main` after completing the requested behavior. Treat target and platform modules as authoritative for machine-specific behavior. Never write to QEMU's test finisher or its address 0x00100000; only startup code may use it after main returns. Only use register aliases documented by the target. Avoid helper calls from `main` unless you preserve its return address.\n",
    );
    prompt
}

fn request_assembly(endpoint: &str, model: &str, prompt: &str) -> Result<String, String> {
    let endpoint = endpoint.trim_end_matches('/');
    let url = format!("{endpoint}/chat/completions");
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| format!("could not create LLM HTTP client: {error}"))?;
    let request = ChatRequest {
        model,
        messages: vec![
            ChatMessage {
                role: "system",
                content: "You are a bare-metal assembly code generator. Follow the user requirements exactly. Output only complete GNU assembler source defining the requested entry point, including both `.globl main` and `main:`. Add concise GNU assembler comments explaining logical blocks, important register roles, and non-obvious assumptions. Never use markdown or prose outside the source. Do not replace requested behavior with a shortcut or write to an emulator exit device unless explicitly required by the program.",
            },
            ChatMessage {
                role: "user",
                content: prompt,
            },
        ],
        temperature: 0.0,
        max_tokens: 4096,
        chat_template_kwargs: use_llama_template_options(endpoint).then_some(ChatTemplateOptions {
            enable_thinking: false,
        }),
    };
    let mut call = client.post(&url).json(&request);
    if let Ok(api_key) = env::var("LLNC_LLM_API_KEY") {
        if !api_key.is_empty() {
            call = call.bearer_auth(api_key);
        }
    }
    let response = call
        .send()
        .map_err(|error| format!("could not reach local LLM at {url}: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("could not read LLM response: {error}"))?;
    if !status.is_success() {
        return Err(format!("LLM server returned HTTP {status}: {body}"));
    }
    let response: ChatResponse = serde_json::from_str(&body)
        .map_err(|error| format!("invalid OpenAI-compatible response: {error}; body: {body}"))?;
    let choice = response
        .choices
        .first()
        .ok_or_else(|| "LLM response contained no choices".to_string())?;
    if choice.finish_reason.as_deref() == Some("length") {
        return Err("LLM output reached the token limit before completion".into());
    }
    let content = choice.message.content.trim();
    extract_assembly(content)
}

fn use_llama_template_options(endpoint: &str) -> bool {
    reqwest::Url::parse(endpoint)
        .ok()
        .and_then(|url| {
            let host = url.host_str()?.to_ascii_lowercase();
            let port = url.port_or_known_default()?;
            Some(matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1") && port == 8080)
        })
        .unwrap_or(false)
}

fn validate_assembly_entry(assembly: &str) -> Result<(), String> {
    let is_exported = assembly.lines().map(str::trim).any(|line| {
        line.eq_ignore_ascii_case(".globl main") || line.eq_ignore_ascii_case(".global main")
    });
    if !is_exported {
        return Err("missing `.globl main` or `.global main` directive".into());
    }
    if !assembly.lines().map(str::trim).any(|line| line == "main:") {
        return Err("missing `main:` function label".into());
    }
    Ok(())
}

fn extract_assembly(response: &str) -> Result<String, String> {
    let content = if let Some(start) = response.find("```") {
        let after_marker = &response[start + 3..];
        let Some(body_start) = after_marker.find('\n') else {
            return Err("LLM returned an incomplete code fence".into());
        };
        let code = &after_marker[body_start + 1..];
        let Some(end) = code.find("```") else {
            return Err("LLM returned an unterminated code fence".into());
        };
        code[..end].trim()
    } else {
        response.trim()
    };
    if content.is_empty() {
        return Err("LLM returned empty assembly".into());
    }
    Ok(content.to_string())
}

fn require_tool(name: &str) -> Result<(), String> {
    println!("  - {name} --version");
    let output = Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| {
            format!(
                "required tool `{name}` is not executable ({error}); install the target GNU toolchain and QEMU, or use `build --generate-only`"
            )
        })?;
    if !output.success() {
        return Err(format!(
            "required tool `{name}` returned {output} for --version"
        ));
    }
    Ok(())
}

fn assemble(
    project: &llnc_core::ProjectSummary,
    output_dir: &Path,
) -> Result<Option<String>, String> {
    let startup_result = run_assembler(
        &project.toolchain.assembler,
        &project.toolchain.assembler_args,
        &project.startup,
        Path::new("startup.o"),
        output_dir,
    )?;
    if !startup_result.success {
        return Err(format!(
            "could not assemble startup code:\n{}",
            startup_result.output
        ));
    }

    let source_result = run_assembler(
        &project.toolchain.assembler,
        &project.toolchain.assembler_args,
        Path::new("generated.s"),
        Path::new("program.o"),
        output_dir,
    )?;
    if !source_result.success {
        return Ok(Some(source_result.output));
    }
    Ok(None)
}

struct ToolOutput {
    success: bool,
    output: String,
}

fn run_assembler(
    executable: &str,
    args: &[String],
    source: &Path,
    object: &Path,
    current_dir: &Path,
) -> Result<ToolOutput, String> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .arg(source)
        .arg("-o")
        .arg(object)
        .current_dir(current_dir);
    println!(
        "  $ {executable} {} (cwd: {})",
        args.iter()
            .map(String::as_str)
            .chain([
                source.to_string_lossy().as_ref(),
                "-o",
                object.to_string_lossy().as_ref()
            ])
            .collect::<Vec<_>>()
            .join(" "),
        current_dir.display()
    );
    let output = command
        .output()
        .map_err(|error| format!("could not run `{executable}`: {error}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(ToolOutput {
        success: output.status.success(),
        output: text,
    })
}

fn link(project: &llnc_core::ProjectSummary, output_dir: &Path) -> Result<(), String> {
    let mut command = Command::new(&project.toolchain.linker);
    command
        .args(&project.toolchain.linker_args)
        .arg("-T")
        .arg(&project.linker_script)
        .arg("-o")
        .arg("firmware.elf")
        .arg("startup.o")
        .arg("program.o")
        .current_dir(output_dir);
    println!(
        "  $ {} {} -T {} -o firmware.elf startup.o program.o (cwd: {})",
        project.toolchain.linker,
        project.toolchain.linker_args.join(" "),
        project.linker_script.display(),
        output_dir.display()
    );
    let output = command
        .output()
        .map_err(|error| format!("could not run `{}`: {error}", project.toolchain.linker))?;
    if !output.status.success() {
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(format!("GNU linker failed:\n{diagnostic}"));
    }
    if let Some(objcopy) = &project.toolchain.objcopy {
        println!("  $ {objcopy} -O binary firmware.elf firmware.bin");
        let result = Command::new(objcopy)
            .args(["-O", "binary"])
            .arg("firmware.elf")
            .arg("firmware.bin")
            .current_dir(output_dir)
            .output()
            .map_err(|error| format!("could not run `{objcopy}`: {error}"))?;
        if !result.status.success() {
            return Err(format!(
                "GNU objcopy failed:\n{}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
    }
    Ok(())
}

fn run_emulator(project: &llnc_core::ProjectSummary, output_dir: &Path) -> Result<Vec<u8>, String> {
    let mut command = Command::new(&project.emulator.executable);
    let mut displayed_args = Vec::new();
    for arg in &project.emulator.args {
        if arg == "{elf}" {
            command.arg("firmware.elf");
            displayed_args.push("firmware.elf");
        } else if arg == "{bin}" {
            command.arg("firmware.bin");
            displayed_args.push("firmware.bin");
        } else if arg.contains('{') || arg.contains('}') {
            return Err(format!("unsupported emulator argument placeholder: {arg}"));
        } else {
            command.arg(arg);
            displayed_args.push(arg.as_str());
        }
    }
    println!(
        "  $ {} {} (cwd: {})",
        project.emulator.executable,
        displayed_args.join(" "),
        output_dir.display()
    );
    command
        .current_dir(output_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        format!(
            "could not launch `{}`: {error}",
            project.emulator.executable
        )
    })?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Err("QEMU stdout pipe was not created".into());
    };
    let Some(mut stderr) = child.stderr.take() else {
        let _ = child.kill();
        return Err("QEMU stderr pipe was not created".into());
    };
    let stdout_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    let stderr_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stderr.read_to_end(&mut output).map(|_| output)
    });
    let timeout = Duration::from_secs(project.emulator.timeout_seconds);
    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            let stdout = collect_pipe(stdout_reader, "stdout")?;
            let stderr = collect_pipe(stderr_reader, "stderr")?;
            let mut output = stdout;
            output.extend_from_slice(&stderr);
            write_artifact(output_dir, "qemu.log", &output)?;
            return Err(format!(
                "QEMU exceeded {} seconds and was stopped. stderr: {}",
                project.emulator.timeout_seconds,
                String::from_utf8_lossy(&stderr)
            ));
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = collect_pipe(stdout_reader, "stdout")?;
            let _ = collect_pipe(stderr_reader, "stderr")?;
            return Err(format!("could not wait for QEMU: {error}"));
        }
    };
    let mut output = collect_pipe(stdout_reader, "stdout")?;
    let stderr = collect_pipe(stderr_reader, "stderr")?;
    output.extend_from_slice(&stderr);
    write_artifact(output_dir, "qemu.log", &output)?;
    if !status.success() {
        return Err(format!(
            "QEMU exited with {status}; see {}",
            output_dir.join("qemu.log").display()
        ));
    }
    Ok(output)
}

fn collect_pipe(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| format!("QEMU {name} reader thread panicked"))?
        .map_err(|error| format!("could not read QEMU {name}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{
        extract_assembly, generated_comments, safe_name, use_llama_template_options,
        validate_assembly_entry,
    };

    #[test]
    fn requires_an_exported_main_entry() {
        assert!(validate_assembly_entry(".main:\nret").is_err());
        assert!(validate_assembly_entry(".globl main\nother:\nret").is_err());
        assert!(validate_assembly_entry(".global main\nmain:\nret").is_ok());
    }

    #[test]
    fn adds_llama_template_options_only_for_local_servers() {
        assert!(use_llama_template_options("http://127.0.0.1:8080/v1"));
        assert!(use_llama_template_options("http://localhost:8080/v1"));
        assert!(!use_llama_template_options("http://localhost:11434/v1"));
        assert!(!use_llama_template_options("https://api.example.com/v1"));
    }

    #[test]
    fn extracts_assembly_from_model_code_fence() {
        assert_eq!(
            extract_assembly("Here is the result:\n```asm\n.globl main\nret\n```").unwrap(),
            ".globl main\nret"
        );
    }

    #[test]
    fn rejects_empty_or_incomplete_model_output() {
        assert!(extract_assembly("```asm\n.globl main").is_err());
        assert!(extract_assembly("```asm\n```").is_err());
    }

    #[test]
    fn displays_only_generated_assembly_comments() {
        assert_eq!(
            generated_comments("# setup\n.globl main\n  # send byte\nret"),
            "# setup\n# send byte"
        );
        assert!(generated_comments(".globl main\nret").is_empty());
    }

    #[test]
    fn sanitizes_artifact_directory_name() {
        assert_eq!(safe_name("serial hello!"), "serial-hello");
    }
}
