/// Source: https://docs.rs/same-file/1.0.4/src/same_file/lib.rs.html#326-328
use prusti_contracts::*;


/* COUNTEREXAMPLE : 
    Handle.as_file_mut: 
        self <- Handle(UnixImplHandle {
            file : Option::Some(StdFsFile{
                inner : SysFsFile(FileDesc{
                    fd: 42,
                })
            }),
            is_std : true,
            dev : 42,
            ino : 42, 
            })
        result <- t2

*/

pub type CInt = i32;

pub struct FileDesc {
    fd: CInt,
}

pub struct SysFsFile(FileDesc);

pub struct StdFsFile {
    inner: SysFsFile,
}

pub struct UnixImplHandle {
    file: Option<StdFsFile>,
    // If is_std is true, then we don't drop the corresponding File since it
    // will close the handle.
    is_std: bool,
    dev: u64,
    ino: u64,
}

impl UnixImplHandle {
    #[trusted]
    pub fn as_file_mut(&mut self) -> &mut StdFsFile {
        // unwrap() will not panic. Since we were able to open the
        // file successfully, then `file` is guaranteed to be Some()
        self.file.as_mut().take().unwrap()
    }
}

pub struct Handle(UnixImplHandle);

impl Handle {
    #[ensures(false)] //~ ERROR postcondition
    pub fn as_file_mut(&mut self) -> &mut StdFsFile {
        self.0.as_file_mut()
    }
}

fn main() {}
