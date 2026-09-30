---
id: target.riscv32-qemu-virt
kind: target
depends_on:
  - target.riscv32imafdc
---
# QEMU RISC-V `virt` target profile

This target combines the `rv32imafdc` instruction set and `ilp32d` ABI with
QEMU's 32-bit `virt` machine. The configured linker places the bare-metal
image in RAM at `0x80000000`; startup initializes the stack and clears `.bss`
before calling the generated `main` routine.

The machine-specific toolchain and emulator arguments are in the adjacent
`machine.yaml` file. Keep these details out of architecture-independent
algorithm descriptions.
