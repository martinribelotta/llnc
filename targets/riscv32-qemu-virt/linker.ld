OUTPUT_ARCH(riscv)
ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x80000000, LENGTH = 128M
}

SECTIONS
{
    . = ORIGIN(RAM);
    .text : ALIGN(4)
    {
        KEEP(*(.text.init))
        *(.text .text.*)
    } > RAM

    .rodata : ALIGN(4) { *(.rodata .rodata.*) } > RAM
    .data : ALIGN(4) { *(.data .data.*) } > RAM
    .bss (NOLOAD) : ALIGN(4)
    {
        __bss_start = .;
        *(.bss .bss.* COMMON)
        . = ALIGN(4);
        __bss_end = .;
    } > RAM

    . = ALIGN(16);
    __stack_top = ORIGIN(RAM) + LENGTH(RAM);
}
