fn main() {
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    tauri_build::build()
}
