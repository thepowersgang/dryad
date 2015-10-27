#![feature(no_std, lang_items, asm, core, core_str_ext)]
#![no_std]
#![no_main]
#![no_builtins]

#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(unused_variables)]

mod llvm_symbols;
mod auxv;
mod kernel_block;
mod utils;

use kernel_block::KernelBlock;
use utils::*;

//extern "C"
//void __attribute__((noinline)) __attribute__((visibility("default")))

// unused; someone figure out how to get gdb working when running as a dyld
extern "C" {
    fn rtld_db_dlactivity();
}

// elf abi requires _start; this must be in assembly because we need
// the raw stack pointer as the argument to _dryad_init
extern {
    fn _start();
}

#[no_mangle]
pub extern fn _dryad_init(raw_args: *const u64) -> u64 {
    write(&"dryad::_dryad_init\n");
    let block = KernelBlock::new(raw_args);
    unsafe { block.debug_print();}

    // commenting _exit will successfully tranfer control
    // to the program entry in test/test,
    // but segfaults when printf is called (obviously)
    let entry = block.getauxval(auxv::AT::ENTRY).unwrap();
    let base = block.getauxval(auxv::AT::BASE).unwrap();
    write(&"entry: ");
    write_u64(entry);
    write(&"\n");
    write(&"base: ");
    write_u64(base);
    write(&"\n");

    _exit(0);
    entry
}
