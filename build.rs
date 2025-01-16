fn main() {
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-fapple-link-rtlib");

    println!("cargo:rustc-link-lib=static=pdfium");
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-search=native=./libs/pdfium-static-arm64-v6694/lib");
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-search=native=./libs/pdfium-linux-x86-134.0/lib");
    println!("cargo:rustc-link-lib=dylib=c++");
    // https://github.com/ajrcarey/pdfium-render/issues/126
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
}
