fn main() {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
    println!("cargo:rustc-link-arg=-fapple-link-rtlib");

    println!("cargo:rustc-link-lib=static=pdfium");
    println!(
        "cargo:rustc-link-search=native=/Users/amine/coding/ferrules/libs/pdfium-static-arm64-v6694/lib"
    );
    println!("cargo:rustc-link-lib=dylib=c++");
    // https://github.com/ajrcarey/pdfium-render/issues/126
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
}
