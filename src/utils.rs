#![allow(private_no_mangle_fns)]

use core::str;

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
#[no_mangle]
pub extern fn write_u64(i: u64){
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
