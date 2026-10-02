fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/macos_ddc.m")
            .flag("-fobjc-arc")
            .compile("displayflip_macos_ddc");
        for framework in ["Foundation", "CoreGraphics", "ColorSync", "IOKit"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rerun-if-changed=native/macos_ddc.m");
    }
    tauri_build::build()
}
