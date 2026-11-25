//! Writes the app's icon and version details into the `.exe` when the target is Windows.

#[path = "src/icon.rs"]
mod icon;

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=APP_VERSION");
    let version = std::env::var("APP_VERSION").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    let version = version.trim_start_matches('v').to_string();
    println!("cargo:rustc-env=GLOVEBOX_VERSION={version}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let ico = out.join("glovebox.ico");
    if let Err(e) = std::fs::write(&ico, icon::ico()) {
        println!("cargo:warning=the icon couldn't be written: {e}");
        return;
    }
    // Windows wants four numbers
    let mut parts: Vec<u64> = version.split(['.', '-', '+']).map_while(|p| p.parse().ok()).take(4).collect();
    parts.resize(4, 0);
    let packed = parts[0] << 48 | parts[1] << 32 | parts[2] << 16 | parts[3];
    let mut res = winresource::WindowsResource::new();
    res.set_icon(&ico.to_string_lossy());
    res.set("ProductName", "Glovebox");
    res.set("FileDescription", "Glovebox, a level editor for Glover (N64)");
    res.set("OriginalFilename", "Glovebox.exe");
    res.set("ProductVersion", &version);
    res.set("FileVersion", &version);
    res.set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed);
    res.set_version_info(winresource::VersionInfo::FILEVERSION, packed);
    if let Err(e) = res.compile() {
        println!("cargo:warning=the icon and version weren't written into the program: {e}");
    }
}
