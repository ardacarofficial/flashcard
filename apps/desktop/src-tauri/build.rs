fn main() {
    // `generate_context!` requires `frontendDist` to exist at compile time.
    // Create an empty placeholder so `cargo check` works before `pnpm build`.
    let dist = std::path::Path::new("../dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(dist);
    }
    // Declaring the app's commands makes them deny-by-default: the webview may
    // only call them when a capability grants `allow-invoke`.
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["invoke"])),
    )
    .expect("failed to run tauri-build");
}
