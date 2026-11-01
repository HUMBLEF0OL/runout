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
    // Bake the Spotify client ID into the exe (spotify::BUILTIN_CLIENT_ID) so the installed app
    // needs no env var: from RUNOUT_SPOTIFY_CLIENT_ID at build time, else the gitignored
    // `spotify_client_id.local` next to this file. Neither → the Connect dialog asks for it.
    println!("cargo:rerun-if-env-changed=RUNOUT_SPOTIFY_CLIENT_ID");
    println!("cargo:rerun-if-changed=spotify_client_id.local");
    let id = std::env::var("RUNOUT_SPOTIFY_CLIENT_ID")
        .ok()
        .or_else(|| std::fs::read_to_string("spotify_client_id.local").ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
        println!("cargo:rustc-env=RUNOUT_BUILTIN_CLIENT_ID={id}");
    }
    tauri_build::try_build(attrs).expect("tauri-build failed");
}
