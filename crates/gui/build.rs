fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint build failed");

    // Windows: program icon and version information in the .exe.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("../../assets/brand/autoupload.ico")
                .set("ProductName", "AutoUpload")
                .set("FileDescription", "AutoUpload")
                .set("CompanyName", "Ellipsio Studio")
                .set("LegalCopyright", "MIT License, Ellipsio Studio");
            res.compile().expect("Windows resources");
        }
    }
}
