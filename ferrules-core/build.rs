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

#[allow(dead_code)]
fn cfg_linux_arm64() {
    println!("cargo:rustc-link-lib=static=pdfium");
    println!("cargo:rustc-link-search=native=./libs/pdfium-static-arm64-v6694/lib");
    println!("cargo:rustc-link-lib=dylib=c++");
}

fn main() {
    #[cfg(target_os = "macos")]
    cfg_macos();

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    cfg_linux_x86();

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    cfg_linux_arm64();
}
