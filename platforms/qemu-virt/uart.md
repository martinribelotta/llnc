---
id: platform.qemu-virt.uart
kind: platform
depends_on:
  - target.riscv32-qemu-virt
---
# QEMU `virt` platform UART

The initial platform is QEMU's generic 32-bit RISC-V `virt` machine. It
provides 128 MiB of RAM beginning at `0x80000000` and a 16550-compatible UART
at MMIO base address `0x10000000`.

The UART transmitter holding register is at offset `0x00`. The line-status
register is at offset `0x05`; bit 5 (`THRE`) indicates that the transmitter
can accept another byte. Poll that bit before writing each byte to the
transmitter holding register.

Programs can use the UART as their observable output channel in the emulator.
Do not assume this emulated device is present on a different physical board.

When `main` returns, startup writes `0x5555` to the QEMU `virt` test finisher at
`0x00100000`. This ends the emulation with a successful status. This address
is reserved for startup code; program modules must return rather than writing
to it or spinning forever when they are expected to complete.

For a byte in `a0`, a target-specific transmitter routine can load the UART
base into `t0`, poll the line-status register with `lbu t1, 5(t0)` and
`andi t1, t1, 0x20` until nonzero, then send the byte with `sb a0, 0(t0)`.
Only use documented registers and return from helper routines with `ret`.

When emitting a byte string, keep its bytes in `.rodata` and iterate over it
with a pointer. For each byte, poll `THRE`, store the byte to the transmitter
holding register, advance the pointer, and stop at the null terminator. Do not
assume multiple UART writes can complete without checking transmitter status.

The following GNU assembly pattern shows the polling loop inside `main`; place
the null-terminated message in the `.rodata` section and replace `message`
with its label:

```asm
    .section .text
    la t0, message
    li t1, 0x10000000
.Lnext:
    lbu t2, 0(t0)
    beqz t2, .Ldone
.Lwait:
    lbu t3, 5(t1)
    andi t3, t3, 0x20
    beqz t3, .Lwait
    sb t2, 0(t1)
    addi t0, t0, 1
    j .Lnext
.Ldone:
    ret

    .section .rodata
message:
    .asciz "example\n"
```
