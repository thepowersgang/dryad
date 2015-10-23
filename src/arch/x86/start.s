        .text
        .globl _start
        .type _start, @function
_start:
        mov %rsp, %rdi
        callq _dryad_init
        retq
