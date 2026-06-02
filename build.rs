fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=src/macos_notifications.m");
        cc::Build::new()
            .file("src/macos_notifications.m")
            .flag("-fobjc-arc")
            .flag("-Wno-deprecated-declarations")
            .compile("pullbell_macos_notifications");
    }
}
