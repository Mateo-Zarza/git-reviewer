// Builds the app icon into the Windows .exe, so Explorer, the taskbar and shortcuts show
// it. For any other target, embed_resource does nothing. The macOS icon goes into the
// .app instead (see .github/workflows/build.yml).
fn main() {
    println!("cargo::rerun-if-changed=assets/app-icon/icon.rc");
    println!("cargo::rerun-if-changed=assets/app-icon/icon.ico");
    embed_resource::compile("assets/app-icon/icon.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("couldn't build the app icon into the .exe");
}
