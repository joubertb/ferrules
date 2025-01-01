#!/bin/bash
mkdir ./pdfium
curl -sLo pdfium.tgz https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-mac-x64.tgz
tar -xzvf pdfium.tgz -C ./pdfium
mv ./pdfium /usr/local/opt/
VERSION="$(cat /usr/local/opt/pdfium/VERSION | grep BUILD | sed -e 's/BUILD=//')" \
cat > /usr/local/lib/pkgconfig/pdfium.pc<< EOF
prefix=/usr/local/opt/pdfium
libdir=/usr/local/opt/pdfium/lib
includedir=/usr/local/opt/pdfium/include

Name: PDFium
Description: PDFium
Version: $VERSION
Requires:

Libs: -L\${libdir} -lpdfium
Cflags: -I\${includedir}
EOF
 ln -s /usr/local/opt/pdfium/lib/libpdfium.dylib /usr/local/lib/libpdfium.dylib
