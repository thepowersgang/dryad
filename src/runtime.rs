use binary::elf::image::SharedObject;
use binary::elf::rela;
use binary::elf::gnu_hash;

extern {
    /// The assembly stub which grabs the stack pointer, aligns and unwinds the stack into parameters and then calls `dryad_resolve_symbol` with those parameters.
    /// _Many_ thanks to Mutabah from `#rust@Mozilla` for suggesting the stack needed to be 16-byte aligned, after I experienced crashes on `movaps %xmm2,0x60(%rsp)`.
    pub fn _dryad_resolve_symbol();
}

/// The data structure which allows runtime lazy binding.  A pointer to this structure is placed in a binaries GOT[1] in `prepare_got`,
/// and reconstructed in `dryad_resolve_symbol`
#[repr(C)]
pub struct Rendezvous<'a> {
    pub idx: usize,
    pub debug: bool,
    pub link_map: &'a[SharedObject<'a>],
}

#[no_mangle]
pub extern fn dryad_resolve_symbol (rndzv_ptr: *const Rendezvous, rela_idx: usize) -> usize {
    unsafe {
        let rndzv = &*rndzv_ptr; // dereference the data structure
        if rndzv.debug { println!("<dryad_resolve_symbol> link_map_ptr: {:#?} rela_idx: {}", rndzv_ptr, rela_idx); }
        let link_map = rndzv.link_map;
        let requesting_so = &link_map[rndzv.idx]; // get who called us using the index in the data structure
        let rela = &requesting_so.pltrelatab[rela_idx]; // now get the relocation using the rela_idx the binary pushed onto the stack
        let requested_symbol = &requesting_so.symtab[rela::r_sym(rela.r_info) as usize]; // obtain the actual symbol being requested
        let name = &requesting_so.strtab[requested_symbol.st_name as usize]; // ... and now it's name, which we'll use to search
        if rndzv.debug { println!("<dryad_resolve_symbol> reconstructed link_map of size {} with requesting binary {:#?} for symbol {} with rela idx {}", link_map.len(), requesting_so.name, name, rela_idx); }
        let hash = gnu_hash::hash(name);
        for (i, so) in link_map.iter().enumerate() {
            if let Some (symbol) = so.find(name, hash) {
                if rndzv.debug { println!("<dryad_resolve_symbol> binding \"{}\" in {} to {} at address 0x{:x}", name, so.name, requesting_so.name, symbol); }
                return symbol as usize
            }
        }
        println!("<dryad_resolve_symbol> Uh-oh, symbol {} not found, about to return a 0xdeadbeef sandwhich for you to munch on, goodbye!", name);
        0xdeadbeef // lel
    }
}
