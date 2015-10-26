#![feature(no_std, lang_items, asm, core, core_str_ext)]
#![no_std]
#![no_main]
#![no_builtins]

#![allow(unused_features)]
#![allow(unused_mut)]
#![allow(dead_code)]
#![allow(unused_variables)]

//extern crate core;

mod auxv;
mod kernel_block;
mod utils;

use kernel_block::KernelBlock;
use utils::*;

//extern "C"
//void __attribute__((noinline)) __attribute__((visibility("default"))) 
extern "C" {
    fn rtld_db_dlactivity();
}

// elf abi requires _start; this must be in assembly because we need
// the raw stack pointer as the argument to _dryad_init
extern {
    fn _start();
}

#[no_mangle]
pub extern fn _dryad_init(raw_args: *const u64) {
    write("dryad::_dryad_init\n");
    let block = KernelBlock::new(raw_args);
    block.print();
    if let Some(entry) = block.getauxval(auxv::AT::ENTRY) {
        write(&"entry: ");
        write_u64(entry);
        write(&"\n");
    }
    _exit(0)
        // this will successfully tranfer control
        // to the program entry in test/test,
        // but segfaults when printf is called (obviously)
//    0x400270

}

#[lang = "stack_exhausted"] extern fn stack_exhausted() {}
#[lang = "eh_personality"] extern fn eh_personality() {}
#[lang = "panic_fmt"]
extern fn panic_fmt(args: &core::fmt::Arguments,
                    file: &str,
                    line: u32) -> ! {
    loop {}
}


// this is all shit from chrichton's rlibc to get it to compile will move to a mod or elsewhere later

#[no_mangle]
pub unsafe extern fn memcpy(dest: *mut u8, src: *const u8,
                            n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *dest.offset(i as isize) = *src.offset(i as isize);
        i += 1;
    }
    return dest;
}

#[no_mangle]
pub unsafe extern fn memmove(dest: *mut u8, src: *const u8,
                             n: usize) -> *mut u8 {
    if src < dest as *const u8 { // copy from end
        let mut i = n;
        while i != 0 {
            i -= 1;
            *dest.offset(i as isize) = *src.offset(i as isize);
        }
    } else { // copy from beginning
        let mut i = 0;
        while i < n {
            *dest.offset(i as isize) = *src.offset(i as isize);
            i += 1;
        }
    }
    return dest;
}

#[no_mangle]
pub unsafe extern fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *s.offset(i as isize) = c as u8;
        i += 1;
    }
    return s;
}

#[no_mangle]
pub unsafe extern fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let a = *s1.offset(i as isize);
        let b = *s2.offset(i as isize);
        if a != b {
            return a as i32 - b as i32
        }
        i += 1;
    }
    return 0;
}

// TODO: ADD IMPLEMENTATIONS

#[no_mangle]
pub unsafe extern fn fmodf () -> f32 {
    0.0
}

#[no_mangle]
pub unsafe extern fn fmod () -> f64 {
    0.0f64
}

#[no_mangle]
pub unsafe extern fn __powisf2 () -> f64 {
    0.0f64
}

#[no_mangle]
pub unsafe extern fn __powidf2 () -> f64 {
    0.0f64
}
