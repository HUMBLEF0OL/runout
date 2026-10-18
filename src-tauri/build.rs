fn main() {
    // tauri-build embeds the Common-Controls v6 manifest into bins only, so `cargo test` executables
    // that link the dialog plugin die with STATUS_ENTRYPOINT_NOT_FOUND (TaskDialogIndirect). Embed
    // the same manifest into every linked target instead (docs/kb/windows-tauri.md).
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let mut attrs = tauri_build::Attributes::new();
    if msvc {
        attrs =
            attrs.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attrs).expect("tauri-build failed");
}
