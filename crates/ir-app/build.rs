//! Embeds the app icon into the Windows executable, so Explorer, the taskbar, and shortcuts
//! show it. (The running window gets its icon from the code; the file icon can only come from
//! the executable's resources.)

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    // `cfg!(windows)` is about the machine running this script; the target is what matters.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_icon();
    }
}

#[cfg(windows)]
fn embed_icon() {
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("../../assets/icon.ico");
    if let Err(e) = resource.compile() {
        // The app works without the file icon, so a missing resource compiler is only a warning.
        println!("cargo:warning=could not embed the app icon: {e}");
    }
}

#[cfg(not(windows))]
fn embed_icon() {}
