---
id: target.riscv32imafdc
kind: target
---
# RISC-V 32-bit instruction set and ABI

The initial target uses the RISC-V 32-bit general-purpose ISA with the
`I`, `M`, `A`, `F`, `D`, and `C` extensions (`rv32imafdc`). The selected ABI is
`ilp32d`, which uses 32-bit integer, long, and pointer types and the hardware
floating-point calling convention for double-precision values.

GNU assembler invocations must select both `-march=rv32imafdc` and
`-mabi=ilp32d`. The concrete QEMU machine, memory layout, and platform devices
are described separately by the selected platform profile.

The standard integer ABI register aliases are `zero`, `ra`, `sp`, `gp`, `tp`,
`t0`-`t6`, `s0`-`s11`, and `a0`-`a7`. Names such as `t7` and `t8` do not exist.
