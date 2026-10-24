fn main() {
    // Watch the directory even when a previous interrupted build saw it empty.
    // Watching only the files discovered then can cache a GUI with no assets.
    println!("cargo:rerun-if-changed=../dist");
    if std::env::var_os("CARGO_FEATURE_CUSTOM_PROTOCOL").is_some()
        && !std::path::Path::new("../dist/index.html").is_file()
    {
        panic!("Build the desktop frontend before embedding production assets");
    }
    tauri_build::build()
}
