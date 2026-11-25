fn main() {
    // Windows: program icon and version information in autoupload.exe.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("../../assets/brand/autoupload.ico")
                .set("ProductName", "AutoUpload")
                .set("FileDescription", "AutoUpload command line")
                .set("CompanyName", "Ellipsio Studio")
                .set("LegalCopyright", "MIT License, Ellipsio Studio");
            res.compile().expect("Windows resources");
        }
    }
}
