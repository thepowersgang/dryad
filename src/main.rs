#![no_main]

extern {
    fn _start();
}

#[no_mangle]
pub extern fn _dryad_init(raw_args: *const u8) -> u64{
    unsafe {
        println!("raw_args: {}", *raw_args);
    }
    0xdeadbeef
}
