# Third-party components

ZoriaPDF for Windows includes or uses the following components. Each keeps its own license.

| Component | Use | License |
|---|---|---|
| [PDFium](https://pdfium.googlesource.com/pdfium/) (binaries by [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)) | PDF rendering, text, page layout | BSD-3-Clause / Apache-2.0 |
| [lopdf](https://github.com/J-F-Liu/lopdf) (vendored, modified) | PDF structure, encryption, compression | MIT |
| [Tesseract OCR](https://github.com/tesseract-ocr/tesseract) 5.5.3 | Offline text recognition | Apache-2.0 |
| [tessdata_fast](https://github.com/tesseract-ocr/tessdata_fast) (tur, eng) | OCR language models | Apache-2.0 |
| [Tauri](https://tauri.app) | Desktop shell | MIT / Apache-2.0 |
| [React](https://react.dev) | User interface | MIT |
| [Manrope](https://github.com/sharanda/manrope) | Interface and PDF font | SIL Open Font License 1.1 |

Rust and npm dependencies are listed in `Cargo.lock` and `package-lock.json` with their licenses.
