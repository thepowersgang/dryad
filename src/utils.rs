#![allow(private_no_mangle_fns)]

use core::str;
use core::slice;

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

// this comes from asm.s
extern {
    fn _print(msg: *const u8, len: u64);
}

#[no_mangle]
pub unsafe extern fn write(msg: &str){
    _print(msg.as_ptr(), msg.len() as u64);
}

/*
// this is _totally_ broken and is massively sideeffectul and unpredicatable
#[no_mangle]
pub extern fn write(msg: &str) {
    unsafe {
        asm!("pushq %rdi
              pushq %rax
              pushq %rdx
              pushq %rsi
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
//             : "{rdi}","{rax}", "{rdx}", "{rsi}"
             //             :"{rsi}"(msg.as_ptr()), "{rdx}"(msg.len())
             //
             );
    }
}
*/
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

#[no_mangle]
pub unsafe extern fn write_u64(i: u64) {
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

fn to_str<'a>(cs: *const u8) -> &'a str {
    if cs.is_null() {
        ""
    }else {
        unsafe {
            let mut i = 0;
            let mut c = *cs;
            while c != 0 {
                i += 1;
                c = *cs.offset(i);
            }
            let slice = slice::from_raw_parts(cs, i as usize);
            str::from_utf8(slice).unwrap()
        }
    }
}

pub unsafe extern fn write_chars(cs: *const u8) {
    write(to_str(cs));
}

