fn main() {
    // Native libraries are packaged beside this provider. This affects only the
    // optional provider artifact; the main engine has no FFmpeg startup imports.
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        // DT_RPATH is inherited by transitive FFmpeg dependencies; DT_RUNPATH
        // alone cannot find codec -> resampler dependencies beside this provider.
        Ok("linux" | "android") => {
            println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags,-rpath,$ORIGIN")
        }
        Ok("macos" | "ios") => println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path"),
        _ => {}
    }
}
