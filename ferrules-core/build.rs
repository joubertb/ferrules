#[allow(dead_code)]
fn cfg_macos() {
    println!("cargo:rustc-link-search=native=./libs/pdfium-static-arm64-v6694/lib");
    println!("cargo:rustc-link-arg=-fapple-link-rtlib");
    println!("cargo:rustc-link-lib=static=pdfium");
    println!("cargo:rustc-link-lib=dylib=c++");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-framework=CoreFoundation");
    println!("cargo:rustc-link-framework=CoreText");
}

#[allow(dead_code)]
fn cfg_linux_x86() {
    println!("cargo:rustc-link-lib=static=pdfium");
    println!("cargo:rustc-link-search=native=./libs/pdfium-linux-static");
}
fn main() {
    #[cfg(target_os = "macos")]
    cfg_macos();

    #[cfg(target_os = "linux")]
    cfg_linux_x86();
}
