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
mod image;
mod binary;

use kernel_block::KernelBlock;
use utils::*;
use binary::elf::Header;

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
    unsafe { write(&"dryad::_dryad_init\n"); }

    let block = KernelBlock::new(raw_args);
    unsafe { block.debug_print(); }

    let image = image::Elf::new(block);
    unsafe { image.debug_print(); }

    // such unsafeties
    unsafe {
        //<aatch> m4b, rather than trying to re-write C++ code,
        // figure out what the code is actually doing,
        // then do *that* in Rust.
        let elf_header:&binary::elf::Header = core::mem::transmute(image.base as *const u64);
        write(&"e_phoff: ");
        write_u64(elf_header.e_phoff);
        write(&"\n");
    }

    // because it's _tradition_
    // (https://fossies.org/dox/glibc-2.22/rtld_8c_source.html)
    // line 786:
    // > Ho ho.  We are not the program interpreter!  We are the program itself! 
    if _start as *const u64 as u64 == image.entry {
        unsafe { write(&"-=|dryad====-\n"); }
        _exit(0);
        0
    } else {
        // commenting _exit will successfully
        // tranfer control (in my single test case ;))
        // to the program entry in test/test,
        // but segfaults when printf is called (obviously)
        // since we've done no dynamic linking

        _exit(0);
        image.entry
    }
}
