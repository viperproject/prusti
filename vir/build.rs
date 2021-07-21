use quote::quote;
use std::env;
use std::io::Write;
use std::path::Path;
use vir_gen::define_vir;

fn main() {
    println!("cargo:rerun-if-changed=defs");
    for entry in walkdir::WalkDir::new("defs") {
        let entry = entry.unwrap();
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }

    let root = quote! { components =>
        pub mod polymorphic;
        pub mod legacy;
        pub mod converter;
    };
    let tokens = define_vir(root, Path::new("defs/root.rs"));
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("vir.rs");
    let mut file = std::fs::File::create(dest_path).unwrap();
    file.write_all(tokens.to_string().as_bytes()).unwrap();
}
