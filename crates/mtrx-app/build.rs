//! On Windows, embed the app icon and version information in the `.exe`.
//! `cargo xtask bundle` makes the icon from the logo and passes its path in
//! `MTRX_ICON`; other builds go without (the window icon is set at run
//! time).

fn main() {
    println!("cargo:rerun-if-env-changed=MTRX_ICON");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources();
    }
}

#[cfg(windows)]
fn windows_resources() {
    let mut resources = winresource::WindowsResource::new();
    resources
        .set("ProductName", "Mtrx")
        .set("FileDescription", "Mtrx");
    if let Some(icon) = std::env::var("MTRX_ICON").ok().filter(|i| !i.is_empty()) {
        println!("cargo:rerun-if-changed={icon}");
        resources.set_icon(&icon);
    }
    resources.compile().expect("compiling Windows resources");
}
