---
id: program.serial-hello
kind: program
depends_on:
  - platform.qemu-virt.uart
---
# Serial hello

Write the exact UTF-8 byte sequence `Hello from llnc!\n` to the UART's
transmitter, in order. Poll the UART's transmitter-ready status before each
byte. After sending the newline, return from `main`.

Keep the message as a null-terminated byte sequence in read-only program
memory. Walk it one byte at a time, stop at the null terminator, and send each
byte through the documented UART transmitter procedure. Do not pack the
message into an integer or write bytes without checking transmitter readiness.

This example defines observable behavior, not assembly. The generated program
must use the platform's documented UART, must not access the QEMU test
finisher, and must be suitable for execution under the selected QEMU profile.
