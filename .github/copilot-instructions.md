# GitHub Copilot Instructions for llnc

## Project vision

`llnc` is a generic compiler frontend that uses a locally running language model to translate user descriptions of programs into assembly and then into a binary. Its orchestration frontend invokes GNU toolchain programs such as `as` and `ld` to assemble and link the generated output. The initial MVP is implemented in Rust; do not assume the compiler itself or generated firmware is written in Python, C, or another high-level language.

The LLM is intended to run locally, including smaller models such as Qwen 2.5/3 7B. Keep the design practical for local inference: provide the model with relevant, structured context, make outputs machine-consumable, and validate generated artifacts with real tools instead of trusting the model.

## Source descriptions and knowledge layers

Natural-language descriptions are the program's source code. Users must be able to write a description as large as needed and organize it across multiple Markdown (`*.md`) files, from reusable high-level modules down to detailed implementation guidance. Do not require users to rewrite their intent in a conventional high-level programming language.

Keep these concerns distinct and composable:

1. **Generic knowledge library:** architecture-independent descriptions of common algorithms and techniques, such as comparison, sorting, searching, arithmetic, and buffers. Describe behavior, preconditions, edge cases, resource needs, and relevant tradeoffs so implementations can be adapted to a target.
2. **Instruction-set and toolchain description:** the processor architecture, instruction set, registers, ABI, assembly syntax, assembler/linker tools and options, and artifact-generation details.
3. **Processor/platform description:** the concrete processor or board, memory map, peripherals, registers, and documented procedures for using hardware features. This layer may reference an instruction-set description but must not be confused with it.
4. **Program description:** one or more user-authored Markdown files specifying desired behavior, modules, requirements, constraints, and how the program should use the selected platform and reusable knowledge.

Machine descriptions may use a structured format such as JSON or YAML when that makes factual data easier to validate and consume. Human-authored program and library descriptions remain natural-language Markdown. Avoid duplicating target facts in generic prompts or generic knowledge.

Support both a single comprehensive program description and a modular collection of files. Preserve file identity, hierarchy, dependencies, and provenance when resolving references and assembling context. Detect missing, conflicting, or ambiguous requirements; do not silently invent behavior. Apply clear precedence: target and platform constraints bound possible implementations, program requirements define intended behavior, and generic knowledge provides reusable implementation guidance without overriding either.

## Generic target support

Do not treat any single architecture or microcontroller as the definition of `llnc`. Keep target-specific facts and rules in the machine description and target configuration, rather than hardcoding them into generic orchestration or prompt logic.

The WCH CH32V003 with `rv32ec` / `ilp32e` is an example test target only. Its 24 MHz clock, memory map, register restrictions, and peripherals apply only when that target is selected. Other targets must be able to provide their own architecture, ABI, toolchain options, memory layout, linker configuration, and validation rules.

Keep instruction-set/toolchain facts separate from concrete processor/platform facts so an architecture description can be reused by compatible platforms. Do not assume every difference can be represented as data: introduce a target-specific adapter only when real toolchain or platform behavior requires one.

## Compilation and validation

The compiler flow should make the stages and their failures explicit:

1. Load and validate the selected instruction-set/toolchain and processor/platform descriptions, program Markdown files, and referenced generic knowledge modules.
2. Resolve references and dependencies, preserve provenance, and identify conflicts or material gaps before generation.
3. Assemble only the relevant context and instructions for the local LLM, then request assembly suitable for the selected target.
4. Extract the generated assembly and run the configured GNU assembler with target-appropriate options.
5. If assembly fails, use the assembler diagnostics as input for a bounded correction attempt; report failure clearly if the configured limit is reached.
6. Link the object with a linker script and settings appropriate to the selected target, then produce the requested binary artifact with the configured toolchain.
7. Run applicable static checks and toolchain validation before treating the output as successful.

Keep tool invocation, LLM communication, input/specification handling, and validation clearly separated. Handle subprocess errors and missing tools explicitly; do not report success when assembly, linking, or artifact generation failed.

Validation rules must be tied to the selected machine and task. For example, timing assertions based on a particular clock or core cycle count belong to that target's validation configuration, not to generic compiler behavior. Likewise, peripheral availability and alternative implementations must be determined from the selected machine description.

## Development guidelines

- Prefer simple, explicit, deterministic behavior over vague abstractions.
- Keep machine assumptions configurable and traceable to the selected target description.
- Make Spanish-language user specifications work naturally; keep source code identifiers, comments, and internal data schemas in English unless an existing project convention says otherwise.
- Design prompts and intermediate representations so smaller local models can follow constraints and produce output that can be parsed and validated.
- Treat modular Markdown descriptions as source modules: preserve their structure, references, dependencies, and provenance instead of flattening them into an opaque prompt.
- Keep reusable algorithm knowledge generic; adapt it to the selected instruction set and platform during generation.
- Make conflicts and unresolved requirements visible, and ask for clarification when they affect program behavior or target correctness.
- Test both successful and failing assembler/linker paths, and verify that generated outputs match the selected target configuration.
