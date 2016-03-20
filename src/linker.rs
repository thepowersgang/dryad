#![allow(unused_assignments)] // remove this after validating rela for get_linker_relocations
// Questions from README:
// 1. Is the `rela` _always_ in a `PT_LOAD` segment?
// 2. Is the `strtab` _always_ after the `symtab` in terms of binary offset, and hence we can compute the size of the symtab by subtracting the two?
// TODO: LOAD THE VDSO: linux-vdso.so.1
// TODO: implement the gnu symbol lookup with bloom filter
// start linking some symbols!
use std::collections::HashMap;
use std::boxed::Box;
use std::slice;
use std::fmt;
use std::mem;
use std::fs::File;
use std::path::Path;

//use scoped_thread::Pool;
//use std::thread;
//use std::sync::{Arc, Mutex};

use binary::elf::header::Header;
use binary::elf::program_header;
use binary::elf::program_header::ProgramHeader;
use binary::elf::dyn;
use binary::elf::rela;
use binary::elf::loader;
use binary::elf::image::SharedObject;

use utils;
use kernel_block;
use auxv;

//thread_local!(static FOO: u32 = 0xdeadbeef);

/// The internal config the dynamic linker generates from the environment variables it receives.
struct Config<'a> {
    bind_now: bool,
    debug: bool,
    secure: bool,
    verbose: bool,
    trace_loaded_objects: bool,
    library_path: Vec<&'a str>,
    preload: &'a[&'a str]
}

impl<'a> Config<'a> {
    pub fn new<'b> (block: &'b kernel_block::KernelBlock) -> Config<'b> {
        // Must be non-null or not in environment to be "false".  See ELF spec, page 42:
        // http://flint.cs.yale.edu/cs422/doc/ELF_Format.pdf
        let bind_now = if let Some (var) = block.getenv("LD_BIND_NOW") {
            var != "" } else { false };
        let debug = if let Some (var) = block.getenv("LD_DEBUG") {
            var != "" } else { false };
        // TODO: FIX THIS IS NOT VALID and massively unsafe
        let secure = block.getauxval(auxv::AT_SECURE).unwrap() != 0;
        // TODO: add different levels of verbosity
        let verbose = if let Some (var) = block.getenv("LD_VERBOSE") {
            var != "" } else { false };
        let trace_loaded_objects = if let Some (var) = block.getenv("LD_TRACE_LOADED_OBJECTS") {
            var != "" } else { false };
        let library_path =
            if let Some (paths) = block.getenv("LD_LIBRARY_PATH") {
                // we don't need to allocate since technically the strings are preallocated in the environment variable, but being lazy for now
                let mut dirs: Vec<&str> = vec![];
                if !secure {
                    dirs.extend(paths.split(":").collect::<Vec<&str>>());
                }
                dirs.push("/usr/lib");
                dirs
            } else { 
                vec!["/usr/lib"]
            };
        Config {
            bind_now: bind_now,
            debug: debug,
            secure: secure,
            verbose: verbose,
            trace_loaded_objects: trace_loaded_objects,
            //TODO: finish path logics
            library_path: library_path,
            preload: &[],
        }
    }
}

impl<'a> fmt::Debug for Config<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "bind_now: {} debug: {} secure: {} verbose: {} trace_loaded_objects: {} library_path: {:#?} preload: {:#?}",
               self.bind_now,
               self.debug,
               self.secure,
               self.verbose,
               self.trace_loaded_objects,
               self.library_path,
               self.preload
               )
    }
}

// TODO: this is not inlined with -g
#[inline]
fn compute_load_bias(base:u64, phdrs:&[ProgramHeader]) -> u64 {
    for phdr in phdrs {
        if phdr.p_type == program_header::PT_LOAD {
            return base + (phdr.p_offset - phdr.p_vaddr);
        }
    }
    0
}

/*
#[no_mangle]
pub extern fn _dryad_fini() {
    return
}
*/

unsafe fn get_linker_relocations(bias: u64, dynamic: &[dyn::Dyn]) -> &[rela::Rela] {
    let mut rela = 0;
    let mut relasz = 0;
    let mut relaent = 0;
    let mut relacount = 0;
    for dyn in dynamic {
        match dyn.d_tag {
            dyn::DT_RELA => {rela = dyn.d_val + bias;},
            dyn::DT_RELASZ => {relasz = dyn.d_val;},
            dyn::DT_RELAENT => {relaent = dyn.d_val;},
            dyn::DT_RELACOUNT => {relacount = dyn.d_val;},
            _ => ()
        }
    }
    // TODO: validate relaent, using relacount
    let count = (relasz / relaent) as usize;
    slice::from_raw_parts(rela as *const rela::Rela, count)
}

/// TODO: i think this is false; we may need to relocate R_X86_64_GLOB_DAT and R_X86_64_64
/// DTPMOD64 is showing up in relocs if we make dryad -shared instead of -pie.  and this is because it leaves local executable TLS model because the damn hash map uses random TLS data.  `working_set` has been the bane of my life in this project
/// private linker relocation function; assumes dryad _only_
/// contains X86_64_RELATIVE relocations, which should be true
fn relocate_linker(bias: u64, relas: &[rela::Rela]) {
    for rela in relas {
        if rela::r_type(rela.r_info) == rela::R_X86_64_DTPMOD64 {
            let reloc = (rela.r_offset + bias) as *mut u64;
            unsafe {
                *reloc = 0; // lol seriously ?
            }
        }
        if rela::r_type(rela.r_info) == rela::R_X86_64_RELATIVE {
            // get the actual symbols address given by adding the on-disk binary offset `r_offset` to the symbol with the actual address the linker was loaded at
            let reloc = (rela.r_offset + bias) as *mut u64;
            // now set the content of this address to whatever is at the load bias + the addend
            // typically, this is all static, constant read only global data, like strings, constant ints, etc.
            unsafe {
                // TODO: verify casting bias to an i64 is correct
                *reloc = (rela.r_addend + bias as i64) as u64;
            }
        }
    }
}

extern {
    /// TLS init function which needs a pointer to aux vector indexed by AT_<TYPE> that musl likes
    fn __init_tls(aux: *const u64);
    // added all this for testing  TLS is going to be horrible
    fn __init_tp(p: *const u64);
    fn __copy_tls(mem: *const u8);
    static builtin_tls: *const u64;
}

/// The dynamic linker
/// TODO: Change permissions on most of these fields
pub struct Linker<'process> {
    // TODO: maybe remove base
    pub base: u64,
    pub load_bias: u64,
    pub ehdr: &'process Header,
    pub phdrs: &'process [program_header::ProgramHeader],
    pub dynamic: &'process [dyn::Dyn],
    config: Config<'process>,
    working_set: Box<HashMap<String, SharedObject<'process>>>,
    link_map_order: Vec<String>,
    link_map: Vec<SharedObject<'process>>,
    // TODO: add a set of SharedObject names which a dryad thread inserts into after stealing work to load a SharedObject;
    // this way when other threads check to see if they should load a dep, they can skip from adding it to the set because it's being worked on
    // TODO: lastly, must determine a termination condition to that indicates all threads have finished recursing and no more work is waiting, and hence can transition to the relocation stage
}

impl<'process> fmt::Debug for Linker<'process> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "base: {:x} load_bias: {:x} ehdr: {:#?} phdrs: {:#?} dynamic: {:#?} Config: {:#?}",
               self.base,
               self.load_bias,
               self.ehdr,
               self.phdrs,
               self.dynamic,
               self.config
               )
    }
}

extern {
    /// The assembly stub which grabs the stack pointer, aligns and unwinds the stack into parameters and then calls `dryad_resolve_symbol` with those parameters.
    /// _Many_ thanks to Mutabah from `#rust@Mozilla` for suggesting the stack needed to be 16-byte aligned, after I experienced crashes on `movaps %xmm2,0x60(%rsp)`.
    fn _dryad_resolve_symbol();
}

/// The data structure which allows runtime lazy binding.  A pointer to this structure is placed in a binaries GOT[1] in `prepare_got`,
/// and reconstructed in `dryad_resolve_symbol`
#[repr(C)]
pub struct Rendezvous<'a> {
    pub idx: usize,
    pub link_map: &'a[SharedObject<'a>],
}

#[no_mangle]
pub extern fn dryad_resolve_symbol (rndzv_ptr: *const Rendezvous, rela_idx: usize) -> usize {
    unsafe {
        println!("<dryad_resolve_symbol> link_map_ptr: {:#?} rela_idx: {}", rndzv_ptr, rela_idx);
        let rndzv = &*rndzv_ptr; // dereference the data structure
        let link_map = rndzv.link_map;
        let requesting_so = &link_map[rndzv.idx]; // get who called us using the index in the data structure
        let rela = &requesting_so.pltrelatab[rela_idx]; // now get the relocation using the rela_idx the binary pushed onto the stack
        let requested_symbol = &requesting_so.symtab[rela::r_sym(rela.r_info) as usize]; // obtain the actual symbol being requested
        let name = &requesting_so.strtab[requested_symbol.st_name as usize]; // ... and now it's name, which we'll use to search
        println!("<dryad_resolve_symbol> reconstructed link_map of size {} with requesting binary {:#?} for symbol {} with rela idx {}", link_map.len(), requesting_so.name, name, rela_idx);
        for (i, so) in link_map.iter().enumerate() {
            if let Some (symbol) = so.find(name) {
                println!("<dryad_resolve_symbol> binding \"{}\" in {} to {} at address 0x{:x}", name, so.name, requesting_so.name, symbol);
                return symbol as usize
            }
        }
        println!("<dryad_resolve_symbol> Uh-oh, symbol {} not found, about to return a 0xdeadbeef sandwhich for you to munch on, goodbye!", name);
        0xdeadbeef // lel
    }
}

/// TODO:
/// 1. add config logic path based on env variables
/// 2. be able to link against linux vdso
impl<'process> Linker<'process> {
    pub fn new<'kernel> (base: u64, block: &'kernel kernel_block::KernelBlock) -> Result<Linker<'kernel>, &'static str> {
        unsafe {

            let ehdr = &*(base as *const Header);
            let addr = (base + ehdr.e_phoff) as *const program_header::ProgramHeader;
            let phdrs = ProgramHeader::from_raw_parts(addr, ehdr.e_phnum as usize);
            let load_bias = compute_load_bias(base, &phdrs);
            let vdso_addr = block.getauxval(auxv::AT_SYSINFO_EHDR).unwrap();
            if let Some(dynamic) = dyn::get_dynamic_array(load_bias, &phdrs) {

                let relocations = get_linker_relocations(load_bias, &dynamic);
                relocate_linker(load_bias, &relocations);
                // dryad has successfully relocated itself; time to init tls
                let auxv = block.get_aux();
//                auxv[auxv::AT_PHDR as usize] = addr as u64;
//                auxv[auxv::AT_BASE as usize] = base as u64;
                __init_tls(auxv.as_ptr()); // this _should_ be safe since vec only allocates and shouldn't access tls. maybe.

                /* need something like this or write custom tls initializer
	        if (__init_tp(__copy_tls((void *)builtin_tls)) < 0) {
		a_crash();
                 }
                 */

                // we relocated ourselves so it should be safe to use global data and allocate
                let mut working_set = Box::new(HashMap::new());
                let vdso = SharedObject::from_raw(vdso_addr);
                let mut link_map_order = Vec::new();
                link_map_order.push(vdso.name.to_string());
                working_set.insert(vdso.name.to_string(), vdso);
                Ok (Linker {
                    base: base,
                    load_bias: load_bias,
                    ehdr: &ehdr,
                    phdrs: &phdrs,
                    dynamic: &dynamic,
                    config: Config::new(&block),
                    working_set: working_set,
                    link_map_order: link_map_order,
                    link_map: Vec::new(),
                })

            } else {

                Err ("<dryad> SEVERE: no dynamic array found for dryad; exiting\n")
            }
        }
    }

    // TODO: holy _god_ is this slow; no wonder they switched to a bloom filter.  fix this with proper symbol finding, etc.
    // HACK for testing, performs shitty linear search using the so's `find` method, which is compounded by * num so's (* number of total relocations in this binary group... ouch)
    // fn find_symbol(&self, name: &str) -> Option<&sym::Sym> {
    fn find_symbol(&self, name: &str) -> Option<u64> {
        for so in &self.link_map {
            //println!("<dryad> searching {} for {}", so.name, name);
            let addr = so.find(name);
            if addr != None {
                return addr
            }
        }

        None
    }

    /// Following the steps below, the dynamic linker and the program "cooperate"
    /// to resolve symbolic references through the procedure linkage table and the global
    /// offset table.
    ///
    /// 1. When first creating the memory image of the program, the dynamic linker
    /// sets the second and the third entries in the global offset table to special
    /// values. Steps below explain more about these values.
    ///
    /// 2. Each shared object file in the process image has its own procedure linkage
    /// table, and control transfers to a procedure linkage table entry only from
    /// within the same object file.
    ///
    /// 3. For illustration, assume the program calls `name1`, which transfers control
    /// to the label `.PLT1`.
    ///
    /// 4. The first instruction jumps to the address in the global offset table entry for
    /// `name1`. Initially the global offset table holds the address of the following
    /// pushq instruction, not the real address of `name1`.
    ///
    /// 5. Now the program pushes a relocation index (index) on the stack. The relocation
    /// index is a 32-bit, non-negative index into the relocation table addressed
    /// by the `DT_JMPREL` dynamic section entry. The designated relocation entry
    /// will have type `R_X86_64_JUMP_SLOT`, and its offset will specify the
    /// global offset table entry used in the previous jmp instruction. The relocation
    /// entry contains a symbol table index that will reference the appropriate
    /// symbol, `name1` in the example.
    ///
    /// 6. After pushing the relocation index, the program then jumps to `.PLT0`, the
    /// first entry in the procedure linkage table. The pushq instruction places the
    /// value of the second global offset table entry (GOT+8) on the stack, thus giving
    /// the dynamic linker one word of identifying information. The program
    /// then jumps to the address in the third global offset table entry (GOT+16),
    /// which transfers control to the dynamic linker.
    ///
    /// 7. When the dynamic linker receives control, it unwinds the stack, looks at
    /// the designated relocation entry, finds the symbol’s value, stores the "real"
    /// address for `name1` in its global offset table entry, and transfers control to
    /// the desired destination.
    ///
    /// 8. Subsequent executions of the procedure linkage table entry will transfer
    /// directly to `name1`, without calling the dynamic linker a second time. That
    /// is, the jmp instruction at `.PLT1` will transfer to `name1`, instead of "falling
    /// through" to the pushq instruction.
    fn prepare_got<'a> (&self, idx: usize, pltgot: *const u64, name: &'a str) {

        if pltgot.is_null() {
            println!("<dryad> empty pltgot for {}", name);
            return
        }

        let len = self.link_map.len();
        let rndzv = Box::new(Rendezvous { idx: idx, link_map: self.link_map.as_slice() });

        unsafe {
            // got[0] == the program's address of the _DYNAMIC array, equal to address of the PT_DYNAMIC.ph_vaddr + load_bias
            // got[1] == "is the pointer to a data structure that the dynamic linker manages. This data structure is a linked list of nodes corresponding to the symbol tables for each shared library linked with the program. When a symbol is to be resolved by the linker, this list is traversed to find the appropriate symbol."
            let second_entry = pltgot.offset(1) as *mut *mut Rendezvous;
            // got[2] == the dynamic linker's runtime symbol resolver
            let third_entry = pltgot.offset(2) as *mut usize;

            *second_entry = Box::into_raw(rndzv);
            *third_entry = _dryad_resolve_symbol as usize;
            println!("<dryad> finished got setup for {} GOT[1] = {:?} GOT[2] = {:#x}", name, *second_entry, *third_entry);
        }

    }

    // TODO: rela::R_X86_64_GLOB_DAT => this is a symbol resolution and requires full link map data, and _cannot_ be done before everything is relocated
    fn relocate_got (&self, idx: usize, object: &SharedObject) {
        let symtab = &object.symtab;
        let strtab = &object.strtab;
        let bias = object.load_bias;
        let mut count = 0;
        for rela in object.relatab {
            let typ = rela::r_type(rela.r_info);
            let sym = rela::r_sym(rela.r_info); // index into the sym table
            let symbol = &symtab[sym as usize];
            let name = &strtab[symbol.st_name as usize];
            let reloc = (rela.r_offset + bias) as *mut u64;
            // TODO: remove this print, misleading on anything other than RELATIVE relocs
//            println!("relocating {} {}({:?}) with addend {:x} to {:x}", name, (rela::type_to_str(typ)), reloc, rela.r_addend, (rela.r_addend + bias as i64));
            match typ {
                // B + A
                rela::R_X86_64_RELATIVE | rela::R_X86_64_TPOFF64 => {
                    // set the relocations address to the load bias + the addend
                    unsafe { *reloc = (rela.r_addend + bias as i64) as u64; }
                    count += 1;
                },
                // S TODO: this is a symbol resolution and requires
                rela::R_X86_64_GLOB_DAT => {
                    // resolve symbol;
                    // 1. start with exe, then next in needed, then next until symbol found
                    // 2. use gnu_hash with symbol name to get sym info
                    if let Some(symbol) = self.find_symbol(name) {
                        unsafe { *reloc = symbol; }
                    }
                    count += 1;
                },
                // S + A
                rela::R_X86_64_64 => {
                    // TODO: this is inaccurate because find_symbol is inaccurate
                    if let Some(symbol) = self.find_symbol(name) {
                        unsafe { *reloc = (rela.r_addend + symbol as i64) as u64; }
                    }
                    count += 1;
                },
                /*
                rela::R_X86_64_TPOFF64 => {
                    unsafe { *reloc = (rela.r_addend + bias as i64) as u64; }
                    count += 1;
                },
                */
                // TODO: add erro checking
                _ => ()
            }
        }

        println!("<dryad> relocated {} symbols in {}", count, &object.name);

        self.prepare_got(idx, object.pltgot, &object.name);
    }

    #[no_mangle]
    fn resolve_with_ifunc (&self, addr: usize) -> usize {
        unsafe {
            let ifunc = mem::transmute::<usize, (fn() -> usize)>(addr);
            let res = ifunc();
            println!("<dryad> ifunc says: 0x{:x}", res);
            res
        }
    }

    /// TODO: add check for if SO has the DT_BIND_NOW, and also other flags...
    fn relocate_plt (&self, object: &SharedObject) {

        let symtab = &object.symtab;
        let strtab = &object.strtab;
        let bias = object.load_bias;
        let mut count = 0;

        // x86-64 ABI, pg. 78:
        // > Much as the global offset table redirects position-independent address calculations
        // > to absolute locations, the procedure linkage table redirects position-independent
        // > function calls to absolute locations.
        for rela in object.pltrelatab {
            let typ = rela::r_type(rela.r_info);
            let sym = rela::r_sym(rela.r_info); // index into the sym table
            let symbol = &symtab[sym as usize];
            let name = &strtab[symbol.st_name as usize];
            let reloc = (rela.r_offset + bias) as *mut u64;
            match typ {
                rela::R_X86_64_JUMP_SLOT if self.config.bind_now => {
                    if let Some(symbol_address) = self.find_symbol(name) {
//                        println!("resolving {} to {:#x}", name, symbol_address);
                        unsafe { *reloc = symbol_address; }
                        count += 1;
                    } else {
                        println!("<dryad> Warning, no resolution for {}", name);
                    }
                },
                // fun @ (B + A)()
                rela::R_X86_64_IRELATIVE => {
                    let addr = rela.r_addend + bias as i64;
                    //println!("<dryad> irelative: bias: {:#x} addend: {:#x} addr: {:#x}", bias, rela.r_addend, addr);
                    // TODO: just inline this call here, it's so simple, doesn't need a function
                    unsafe { *reloc = self.resolve_with_ifunc(addr as usize) as u64; }
                    count += 1;
                },
                // TODO: add error checking
                _ => ()
            }
        }
        println!("<dryad> relocate plt: {} symbols for {}", count, object.name);
    }

    /// TODO: rename to something like `load_all` to signify on return everything has loaded?
    /// So: load many -> join -> relocate many -> join -> relocate executable and transfer control
    /// 1. Open fd to shared object ✓ - TODO: parse and use /etc/ldconfig.cache
    /// 2. get program headers ✓
    /// 3. mmap PT_LOAD phdrs ✓
    /// 4. compute load bias and base ✓
    /// 5. get _DYNAMIC real address from the mmap'd segments ✓
    /// 6a. create SharedObject from above ✓
    /// 6b. relocate the SharedObject, including GLOB_DAT ✓ TODO: TLS shite
    /// 6c. resolve function and PLT; for now, just act like LD_PRELOAD is set
    /// 7. add `soname` => `SharedObject` entry in `linker.loaded` TODO: use better structure, resolve dependency chain
    fn load(&mut self, soname: &str) -> Result<(), String> {
        // TODO: properly open the file using soname -> path with something like `resolve_soname`
        let paths = self.config.library_path.to_owned(); // TODO: so we compile, fix unnecessary alloc

        // soname ∉ linker.loaded
        if !self.working_set.contains_key(soname) {
            let mut found = false;
            for path in paths {
                match File::open(Path::new(&path).join(soname)) {
                    Ok (mut fd) => {
                        found = true;
                        println!("Opened: {:?}", fd);
                        let shared_object = try!(loader::load(soname, &mut fd));
                        let libs = &shared_object.libs.to_owned(); // TODO: fix this unnecessary allocation, but we _must_ insert before iterating
                        self.working_set.insert(soname.to_string(), shared_object);

                        // breadth first addition
                        self.link_map_order.extend(libs.iter().map(|s| s.to_string()));

                        for lib in libs {
                            try!(self.load(lib));
                        }
                        break
                    },
                    _ => (),
                }
            }
            if !found {
                return Err(format!("<dryad> could not find {} in {:?}", &soname, self.config.library_path))
            }
        }

        Ok (())
    }
    
    /// Main staging point for linking the executable dryad received
    /// (Experimental): Responsible for parallel execution and thread joining
    /// 1. First builds the executable and then all the shared object dependencies and joins the result
    /// 2. Then, creates the link map, and then relocates all the shared object dependencies and joins the result
    /// 3. Finally, relocates the executable, and then transfers control
    #[no_mangle]
    pub fn link(&mut self, block: &kernel_block::KernelBlock) -> Result<(), String> {


        // Fun Fact: uncomment this for ridiculous disaster: runs fine when links itself, but not when it links an executable, because hell
        /*
        let v = vec![1, 2, 3, 4];
        let mut guards = vec![];
        for k in v {

            let t = thread::spawn(move || {
                println!("{} init", k);
                println!("{} done", k);
            });
            
            guards.push(t);
        }
        
        for g in guards {
            let _ = g.join().unwrap();
        }
        */

        // build executable
        println!("BEGIN EXE LINKING");
        let name = utils::as_str(block.argv[0]);
        let phdr_addr = block.getauxval(auxv::AT_PHDR).unwrap();
        let phnum  = block.getauxval(auxv::AT_PHNUM).unwrap();
        let image = try!(SharedObject::from_executable(name, phdr_addr, phnum as usize));
        println!("Main Image:\n  {:#?}", &image);

        // 1. load all

        // TODO: transfer ownership of libs (or allocate) to the linker, so it can be parallelized
        // this is the only obvious candidate for parallelization, and it's dubious at best... but large binaries spend 20% of time loading and 80% on relocation
        self.link_map_order.extend(image.libs.iter().map(|s| s.to_string()));

        for lib in &image.libs {
            try!(self.load(lib));
        }

        self.link_map_order.sort();
        self.link_map_order.dedup();;

        println!("LINK MAP ORDER: {:#?}", self.link_map_order);

        self.link_map.reserve_exact(self.link_map_order.len()+1);
        self.link_map.push(image);
        for soname in &self.link_map_order {
            println!("remove: {:#?}", soname);
            let so = self.working_set.remove(soname).unwrap();
            self.link_map.push(so);
        }
        println!("<dryad> working set is drained: {}", self.working_set.len() == 0);
        println!("<dryad> link_map ptr: {:#?}, cap = len: {}", self.link_map.as_ptr(), self.link_map.capacity() == self.link_map.len());
        // <join>
        // 2. relocate all
        // TODO: after _all_ SharedObject have been loaded, it is safe to relocate if we stick to ELF symbol search rule of first search executable, then in each of DT_NEEDED in order, then deps of first DT_NEEDED, and if not found, then deps of second DT_NEEDED, etc., i.e., breadth-first search.  Why this is allowed to continue past the executable's _OWN_ dependency list is anyone's guess; a penchant for chaos perhaps?

        // SCOPE is resolved breadth first by ld-so and flattened to a single search list (more or less)
        // exe
        // |_ libfoo
        // |_ libbar
        //    |_ libderp
        //    |_ libbaz
        //    |_ libfoo
        //    |_ libslerp
        //    |_ libmerp
        // |_ libbaz
        //    |_ libmerp
        //    |_ libslerp
        // |_
        //
        // is reduced to [exe, libfoo, libbar, libbaz, libderp, libslerp, libmerp]

        // TODO: determine ld-so's relocation order (_not_ equivalent to it's search order, which is breadth first from needed libs)
        // Because gnu_ifuncs essentially execute arbitrary code, including calling into the GOT, if the GOT isn't setup and relative relocations, for example, haven't been processed in the binary which has the reference, we're doomed.  Example is a libm ifunc (after matherr) for `__exp_finite` that calls `__get_cpu_features` which resides in libc.

        for (i, so) in self.link_map.iter().enumerate() {
            self.relocate_got(i, so);
        }

        // I believe we can parallelize the relocation pass by:
        // 1. skipping constructors, or blocking until the linkmaps deps are signalled as finished
        // 2. if skip, rerun through the link map again and call each constructor, since the GOT was prepared and now dynamic calls are ready
        for so in self.link_map.iter() {
            self.relocate_plt(so);
        }

        // <join>
        // 3. transfer control

        // we safely loaded and relocated everything, dryad will be forgotten when after we return Ok
        // so it doesn't segfault when we try to access it back again after passing through assembly to `dryad_resolve_symbol`,
        // which from the compiler's perspective means it needs to be dropped
        Ok (())
    }
}
