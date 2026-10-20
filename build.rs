fn main() {
    println!("cargo:rerun-if-changed=assets\\mle.ico");
    println!("cargo:rerun-if-changed=assets\\app.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set("ProductName", "MLE - Make Life Easier")
            .set("FileDescription", "Make Life Easier - Selection Translator")
            .set_icon("assets\\mle.ico")
            .set_manifest(include_str!("assets\\app.manifest"))
            .compile()
            .expect("Could not embed the MLE application icon and manifest");
    }
}
