// Writes src/MdRgr.rgr as the crate's Rust module (ranger::build). EVG's
// types are the rangerevg crate's and MathRender rangermath's; the crate's
// dependents' .rgr code uses its .rs types too, so lib.rs is its root.
fn main() {
    ranger::build::Module::new("src/MdRgr.rgr")
        .extern_crate("rangerevg")
        .extern_crate("rangermath")
        .export_path("src/lib.rs")
        .compile();
    for f in ["src/MdEmoji.rgr", "src/MdMath.rgr", "src/MdText.rgr", "../css/CssCore.rgr", "../vfs/src"] {
        println!("cargo:rerun-if-changed={}", f);
    }
}
