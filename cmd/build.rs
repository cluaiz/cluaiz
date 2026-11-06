fn main() {
    // The icon is outside this package: without this line a new icon needs a clean build
    println!("cargo:rerun-if-changed=../assets/logo.ico");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set("InternalName", "cluaiz.exe");
        res.set("FileDescription", "cluaiz");
        res.set("ProductName", "cluaiz");
        res.set("OriginalFilename", "cluaiz.exe");
        res.set("LegalCopyright", "Copyright © 2026 Cluaiz");
        res.set("CompanyName", "Cluaiz");
        let version = format!("{}.0", env!("CARGO_PKG_VERSION"));
        res.set("FileVersion", &version);
        res.set("ProductVersion", &version);

        res.set_manifest(
            r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
        <requestedPrivileges>
            <requestedExecutionLevel level="asInvoker" uiAccess="false" />
        </requestedPrivileges>
    </security>
</trustInfo>
</assembly>
"#,
        );

        // 🚀 Set the cluaiz Taskbar & Executable Icon
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let icon_path = std::path::Path::new(&manifest_dir).parent().unwrap().join("assets").join("logo.ico");
        res.set_icon(icon_path.to_str().unwrap());

        if let Err(e) = res.compile() {
            eprintln!("Failed to compile Windows resources: {}", e);
        }
    }
}
