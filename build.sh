#!/bin/bash
clang -c -o start.o src/arch/x86/start.s
#rustc --crate-type=rlib src/auxv.rs -O -g --crate-name=auxv -o auxv.rlib
#rustc --extern auxv=libauxv.rlib src/main.rs -O -g --emit obj -o dryad.o
rustc src/main.rs -g --emit obj -o dryad.o
#-I /lib64/ld-linux-x86-64.so.2
ld -static -Bsymbolic -nostdlib -shared -e _start -o dryad start.o dryad.o /usr/local/lib/rustlib/x86_64-unknown-linux-gnu/lib/libcore-35017696.rlib
