fn main() {
    // tauri::generate_context!() embeds ../dist at compile time.
    println!("cargo:rerun-if-changed=../dist");
    tauri_build::build()
}
