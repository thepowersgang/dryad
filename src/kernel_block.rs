use auxv;
use utils::*;

pub struct KernelBlock {
    pub argc: isize,
    pub argv: *const *const u8,
    pub envp: *const *const u8,
    pub auxv: *const auxv::Elf64_auxv_t,
}

impl KernelBlock {

    pub fn getauxval(&self, t:auxv::AT) -> Option<u64> {
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
        //m4b: ptr.iter().take_while(|x| x.some_field != SOME_DEFINE)
        }
        None
    }
    
    pub fn new (args: *const u64) -> KernelBlock {        
        unsafe {
            let argc = (*args) as isize;
            let argv = args.offset(1) as *const *const u8;
            let envp = argv.offset(argc + 1);

            let mut p = envp;
            // two null pointers mark end of envp
            // and beginning of the auxillary vectors
            while !(*p).is_null() {
                p = p.offset(1);
            }
            p = p.offset(1);
            let auxv = p as *const auxv::Elf64_auxv_t;
            KernelBlock{
                argc:argc,
                argv:argv,
                envp: envp,
                auxv: auxv,
            }
        }
    }

    pub fn print (&self) -> () {

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
