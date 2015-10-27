use kernel_block;
use auxv;
use utils::*;

// lore:
//<aatch> It should work, if you do `$t { $($fld: $fld),+ }`.
//<Gankro> ^--- why I hate Rust's macros // [22:21]

//<durka42> macro_rules! init { ($t:ident: $($fld:ident),* $(,)*) => { $t {   $($fld: $fld),* } } }
macro_rules! init {
    ($t: ident: $($fld: ident,)+) => { $t { $($fld: $fld),* } };
    ($t: ident: $($fld: ident),+) => { $t { $($fld: $fld),* } }
}
// init! { Elf: base, phdr, phent, phnum, entry }

// TODO: move this to a separate elf mod, like collections, as per @Gankro?
pub struct Elf {
    pub base: u64, // future: *const elf_hdr
    pub phdr: *const u64, // future: *const elf_phdr
    pub phent: u64, // sizeof an elf program header entry
    pub phnum: u64,
    pub entry: u64,
    /*
    pub load_bias: u64,
    pub dynamic: * const u64, // future: *const elf_dyn
    pub strtab: * const u8,
    pub symtab: * const u64, // future: *const elf_sym
     */
}

impl Elf {
    pub fn new (block: kernel_block::KernelBlock) -> Elf {
        // base is also = elf_hdr
        let base   = block.getauxval(auxv::AT::BASE).unwrap();
        let phdr   = block.getauxval(auxv::AT::PHDR).unwrap() as *const u64;
        let phent  = block.getauxval(auxv::AT::PHENT).unwrap();
        let phnum  = block.getauxval(auxv::AT::PHNUM).unwrap();
        let entry  = block.getauxval(auxv::AT::ENTRY).unwrap();
        Elf {
            base: base,
            phdr: phdr,
            phent: phent,
            phnum: phnum,
            entry: entry
        }
    }
    pub unsafe fn debug_print (&self) {
        write(&"base: ");
        write_u64(self.base);
        write(&"\n");
        write(&"phdr: ");
        write_u64(self.phdr as u64);
        write(&"\n");
        write(&"phent: ");
        write_u64(self.phent);
        write(&"\n");
        write(&"phnum: ");
        write_u64(self.phnum);
        write(&"\n");
        write(&"entry: ");
        write_u64(self.entry);
        write(&"\n");
    }
}
