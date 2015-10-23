#!/bin/bash
clang -c -o start.o src/arch/x86/start.s
rustc -O src/main.rs --emit obj -o dryad.o

ld -pie -shared -I /lib64/ld-linux-x86-64.so.2 -e _start -o dryad start.o dryad.o -L /usr/local/lib/rustlib/x86_64-unknown-linux-gnu/lib/ -lstd-35017696
