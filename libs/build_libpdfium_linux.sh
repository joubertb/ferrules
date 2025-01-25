#!/usr/bin/env bash
mkdir -p pdfium
cd pdfium

# Clone depot tools, standard tools used for building Chromium and associated projects.
git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git
export PATH="$PATH:$(cd depot_tools; pwd)"

# Clone the pdfium source.
gclient config --unmanaged https://pdfium.googlesource.com/pdfium.git
gclient sync --no-history

# Create default build configuration...
cd pdfium
./build/install-build-deps.sh --no-prompt
rm out/Default/args.gn
gn gen out/Default

# ... and now customise it as needed.
echo "use_goma = false" >> out/Default/args.gn
echo "clang_use_chrome_plugins = false" >> out/Default/args.gn
echo "is_component_build = false" >> out/Default/args.gn
echo "pdf_is_standalone = true" >> out/Default/args.gn
echo "pdf_is_complete_lib = true" >> out/Default/args.gn
echo "pdf_enable_v8 = true" >> out/Default/args.gn
echo "pdf_enable_xfa = true" >> out/Default/args.gn
echo "pdf_use_skia = false" >> out/Default/args.gn
echo "pdf_use_skia_paths = false" >> out/Default/args.gn
echo "is_clang = true" >> out/Default/args.gn
echo "use_custom_libcxx = false" >> out/Default/args.gn
echo "treat_warnings_as_errors = false" >> out/Default/args.gn
echo "target_cpu = \"x64\"" >> out/Default/args.gn
echo "target_os = \"linux\"" >> out/Default/args.gn
echo "is_debug = false" >> out/Default/args.gn

# Run the build.
ninja -C out/Default pdfium
