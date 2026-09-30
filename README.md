# llnc

`llnc` is an experimental compiler whose source language is natural-language
descriptions. Users describe a program in Markdown modules; a local language
model translates the resolved descriptions into target-specific assembly, and
the selected toolchain builds the executable artifact.

The project deliberately separates reusable algorithm knowledge, instruction
set/toolchain facts, concrete platform details, and the program's behavior.
The initial MVP target is QEMU's 32-bit RISC-V `virt` machine.

## MVP structure

```text
.
├── Cargo.toml                 # Rust workspace
├── crates/
│   ├── llnc-cli/              # User-facing CLI and orchestration
│   └── llnc-core/             # Manifest and description-module validation
├── examples/
│   └── serial-hello/           # Small modular program specification
├── knowledge/                 # Architecture-independent algorithm modules
├── platforms/
│   └── qemu-virt/             # Platform and UART documentation
└── targets/
    └── riscv32-qemu-virt/      # ISA/ABI profile, linker script, startup code
```

Each project has an `llnc.yaml` manifest that selects a target and a program
module. The target profile is data in `machine.yaml`; related Markdown modules
provide reusable architectural and platform guidance. Markdown source modules begin with YAML front matter containing an
`id`, `kind`, and optional `depends_on` list. Dependencies form a reusable
module graph; the initial `llnc check` command checks references and cycles
before any model is invoked.

## Prerequisites

- Rust toolchain (stable; minimum Rust 1.85)
- xPack RISC-V bare-metal GNU toolchain and QEMU (install with the script below)
- PowerShell 7 and `curl.exe` for the pinned local LLM setup (optional)

The example profile uses `rv32imafdc` with the `ilp32d` ABI. QEMU's `virt`
machine is configured with 128 MiB of RAM starting at `0x80000000`; the UART
is described separately as a platform device. Tool availability and exact
executable names can vary by operating system.

## Build and check

```sh
cargo build --workspace
cargo test --workspace
cargo run -p llnc -- check examples/serial-hello/llnc.yaml
```

`check` validates the target YAML, parses Markdown front matter, resolves all
declared module dependencies, and reports the dependency order.

## Generate and build

With the local model server running, generate assembly only:

```powershell
cargo run -p llnc -- build examples/serial-hello/llnc.yaml --generate-only
```

The prompt and generated assembly are saved under `artifacts/serial-hello/`.
To also assemble, link, run in QEMU, and check the expected UART output, install
the pinned xPack toolchain and QEMU, then run:

```powershell
.\scripts\setup-xpack-toolchain.ps1
.\scripts\build-qemu-example.ps1
```

The toolchain setup downloads the official xPack Windows x64 releases
(`riscv-none-elf-gcc` 15.2.0-1 and QEMU RISC-V 9.2.4-1), verifies archive
sizes and SHA-256 hashes, and extracts both under the ignored `.local\xpack`
directory. `build-qemu-example.ps1` adds their executable directories to PATH
for the build without changing the machine-wide PATH.

Set `LLNC_LLM_ENDPOINT`, `LLNC_LLM_MODEL`, and optionally `LLNC_LLM_API_KEY`
to use another local or remote OpenAI-compatible chat-completions endpoint.
When using a remote endpoint, the resolved Markdown descriptions and target
details are sent to that service; use only an endpoint trusted with that data.
The compiler makes up to three correction rounds using assembler/linker
diagnostics or QEMU output; prompts, generated assembly, ELF/binary, and QEMU
logs are saved as artifacts.

## Local LLM bundle (Windows, CPU/Vulkan)

The repository provides reproducible download scripts for llama.cpp
`b11284` (Windows CPU or Vulkan backend) and the official Qwen3-4B GGUF
`Q4_K_M` model. The model is pinned to
Hugging Face revision `bc640142c66e1fdd12af0bd68f40445458f3869b`; the setup
script verifies SHA-256 and model size before accepting either download.
Expect approximately 2.5 GB for the model plus the runtime archive.

```powershell
.\scripts\setup-local-llm.ps1
.\scripts\start-local-llm.ps1
```

On supported Intel Arc systems, try the Vulkan build instead:

```powershell
.\scripts\setup-local-llm.ps1 -Backend vulkan
.\scripts\start-local-llm.ps1 -Backend vulkan
```

The server listens only on `127.0.0.1:8080`, uses an 8k context by default,
and runs CPU-only by default for the compatibility baseline. The Vulkan
option offloads up to 99 layers when supported by the local Vulkan driver.
Override its port or context size with `-Port` or `-ContextSize`. In another
terminal, smoke-test the OpenAI-compatible chat endpoint:

```powershell
Invoke-RestMethod `
  -Uri http://127.0.0.1:8080/v1/chat/completions `
  -Method Post `
  -ContentType "application/json" `
  -Body '{"model":"local-model","messages":[{"role":"user","content":"Respond only with: LLNC READY"}],"temperature":0,"max_tokens":32}'
```

Downloaded executables, archives, model weights, and local runtime state are
ignored by Git. Only scripts and pinned metadata are committed. The Rust CLI
calls the same OpenAI-compatible endpoint; the bundled runtime is optional
when configuring another local or remote provider.

## Reproducibility

Commit `Cargo.lock`, target profiles, linker/startup files, source descriptions,
and deterministic test fixtures. Do not commit model weights or generated
firmware. Pin model revisions and record quantization/checksums for inference
runtime assets. Keep tests independent of model availability; tests that
require GNU tools or QEMU should be explicit integration tests.

## Status

This repository contains an MVP pipeline for module resolution, local or
remote LLM assembly generation, GNU assembly/linking, and QEMU execution with
expected-output checking. The `--generate-only` path can be tested without
the cross-toolchain or QEMU installed.
