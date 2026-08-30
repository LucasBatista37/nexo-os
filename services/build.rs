// Compartilhado por todos os programas de usuario: aplica o linker script do diretorio services/.
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let ld = std::path::Path::new(&dir).parent().unwrap().join("linker.ld");
    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rerun-if-changed={}", ld.display());
}
