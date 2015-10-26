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

use core::str;
//use core::slice;

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
pub extern fn _exit(code: u64) {
    unsafe {
        asm!("movq $$60, %rax
              syscall"
             :
             : "{rdi}"(code)
             );
    }
}

// this is _totally_ broken and is massively sideeffectul and unpredicatable
#[no_mangle]
pub extern fn write(msg: &str) {
    unsafe {
        asm!("pushq %rdi
              pushq %rax
              pushq %rdx
              pushq %rsi
              movq $0, %rsi
              movq $1, %rdx
              movq $$1, %rax
              movq $$1, %rdi
              syscall
              popq %rsi
              popq %rdx
              popq %rax
              popq %rdi
              "
             :
             : "{rsi}"(msg.as_ptr()), "{rdx}"(msg.len())
             : "{rdi}","{rax}", "{rdx}", "{rsi}"
//             :"{rsi}"(msg.as_ptr()), "{rdx}"(msg.len())
             );
    }
}

fn digit_to_char_code(i: u8) -> u8 {
    if i <= 9 {
        i + 48
    }else{
        0
    }
}

fn num_digits(i: u64) -> usize {
    let mut count = 0;
    let mut current = i;
    while current > 0 {
        current /= 10;
        count += 1;
    }
    count
}

// ditto side effects
fn write_u64(i: u64){
    let count = num_digits(i);
    let mut _stack = [0; 20];
    let mut chars = &mut _stack[0..count];
    let mut place = count;
    let mut current = i;
    let mut digit;
    loop {
        digit = current % 10;
        current = (current - digit) / 10;
        place -= 1;
        chars[place] = digit_to_char_code(digit as u8);
        if current <= 0 { break; }
    }
    write(str::from_utf8(chars).unwrap());
}

struct KernelBlock {
    argc: isize,
    argv: *const *const u8,
    envp: *const *const u8,
    auxv: *const auxv::Elf64_auxv_t,
//    entry: u64,
//    name: &'static str,
}

impl KernelBlock {

    fn getauxval(&self, t:auxv::AT) -> Option<u64> {
        unsafe {
        let ptr = self.auxv.clone();
        let mut i = 1;
        let mut v = &*ptr;
        while v.a_type != auxv::AT::NULL {
            if v.a_type == t {
                return Some (v.a_val);
            }
            v = &*ptr.offset(i);
            i += 1;
        }
        /*
        m4b: ptr.iter().take_while(|x| x.some_field != SOME_DEFINE)
        for (ElfW(auxv_t)* v = auxv; v->a_type != AT_NULL; ++v) {
            if (v->a_type == type) {
                if (found_match != NULL) {
                    *found_match = true;
                }
                return v->a_un.a_val;
            }
        }
             */
        }
        None
    }
    
    fn new (args: *const u64) -> KernelBlock {        
        unsafe {
            let argc = (*args) as isize;
            let argv = args.offset(1) as *const *const u8;
            let envp = argv.offset(argc + 1);

            let mut p = envp;
            while !(*p).is_null() {
                p = p.offset(1);
            }
            p = p.offset(1);
            let auxv = p as *const auxv::Elf64_auxv_t;
        /*
        uintptr_t* args = reinterpret_cast<uintptr_t*>(raw_args);
        argc = static_cast<int>(*args);
        argv = reinterpret_cast<char**>(args + 1);
        envp = argv + argc + 1;

        // Skip over all environment variable definitions to find aux vector.
        // The end of the environment block is marked by two NULL pointers.
        char** p = envp;
        while (*p != NULL) {
            ++p;
        }
        ++p; // Skip second NULL;
             */
            KernelBlock{
                argc:argc,
                argv:argv,
                envp: envp,
                auxv: auxv,
            }
        }
    }

    fn print (&self) -> () {
        /*
        write(&"name: ");
        write(&self.name);
        write(&"\n");
        write(&"entry: ");
        write_u64(self.entry);
        write(&"\n");
         */
        write(&"argc: ");
        write_u64(self.argc as u64);
        write(&"\n");
        /*
        write(&"argv: ");
        write(self.argv);
        write(&"\n");
         */

    }
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
