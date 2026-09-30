.section .text.init
.globl _start
.type _start, @function
_start:
    la sp, __stack_top
    la t0, __bss_start
    la t1, __bss_end
    bgeu t0, t1, 2f
1:
    sw zero, 0(t0)
    addi t0, t0, 4
    bltu t0, t1, 1b
2:
    call main
    li t0, 0x5555
    li t1, 0x100000
    sw t0, 0(t1)
3:
    j 3b
.size _start, . - _start
