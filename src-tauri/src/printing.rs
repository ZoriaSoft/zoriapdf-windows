//! Native Windows printing, with one bounded page bitmap alive at a time.
use std::path::Path;
use anyhow::{bail, Context, Result};
use base64::Engine as _;
use pdf_core::Engine;
use windows_sys::Win32::{
    Foundation::GlobalFree,
    Graphics::Gdi::{CreateDCW, DeleteDC, GetDeviceCaps, SetStretchBltMode, StretchDIBits,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE, HORZRES, VERTRES, SRCCOPY},
    Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW},
    UI::Controls::Dialogs::{CommDlgExtendedError, PrintDlgW, PRINTDLGW, PD_RETURNDC, PD_NOSELECTION,
        PD_PAGENUMS, PD_USEDEVMODECOPIESANDCOLLATE},
};
fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

struct PrintContext(windows_sys::Win32::Graphics::Gdi::HDC);
impl Drop for PrintContext { fn drop(&mut self) { unsafe { DeleteDC(self.0); } } }
struct PrintJob { dc: windows_sys::Win32::Graphics::Gdi::HDC, completed: bool }
impl Drop for PrintJob { fn drop(&mut self) { if !self.completed { unsafe { AbortDoc(self.dc); } } } }

pub fn print(engine: &Engine, path: &Path, password: Option<&str>, printer: Option<&str>, output: Option<&str>) -> Result<()> {
    let info = engine.open(path, password)?;
    if info.pages > u16::MAX as u32 { bail!("Yazdırma için en fazla 65535 sayfa destekleniyor."); }
    let mut from = 1;
    let mut to = info.pages;
    let dc = if let Some(printer) = printer {
        let driver = wide("WINSPOOL");
        let device = wide(printer);
        unsafe { CreateDCW(driver.as_ptr(), device.as_ptr(), std::ptr::null(), std::ptr::null()) }
    } else {
        let mut dialog = PRINTDLGW::default();
        dialog.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
        dialog.Flags = PD_RETURNDC | PD_NOSELECTION | PD_USEDEVMODECOPIESANDCOLLATE;
        dialog.nMinPage = 1; dialog.nMaxPage = info.pages as u16;
        dialog.nFromPage = 1; dialog.nToPage = info.pages as u16; dialog.nCopies = 1;
        let accepted = unsafe { PrintDlgW(&mut dialog) };
        unsafe {
            if !dialog.hDevMode.is_null() { GlobalFree(dialog.hDevMode); }
            if !dialog.hDevNames.is_null() { GlobalFree(dialog.hDevNames); }
        }
        if accepted == 0 {
            let error = unsafe { CommDlgExtendedError() };
            if error == 0 { bail!("Yazdırma iptal edildi."); }
            bail!("Windows yazdırma penceresi açılamadı (kod {error}).");
        }
        if dialog.Flags & PD_PAGENUMS != 0 { from = dialog.nFromPage as u32; to = dialog.nToPage as u32; }
        dialog.hDC
    };
    if dc.is_null() { bail!("Yazıcı açılamadı. Windows yazıcı ayarlarını kontrol edin."); }
    let context = PrintContext(dc);
    let title = wide(path.file_name().and_then(|s|s.to_str()).unwrap_or("ZoriaPDF"));
    let output_path = output.map(wide);
    let doc = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: title.as_ptr(),
        lpszOutput: output_path.as_ref().map_or(std::ptr::null(), |s|s.as_ptr()),
        lpszDatatype: std::ptr::null(), fwType: 0,
    };
    if unsafe { StartDocW(context.0, &doc) } <= 0 { bail!("Yazdırma işi başlatılamadı veya iptal edildi."); }
    let mut job = PrintJob { dc: context.0, completed: false };
    let printable_w = unsafe { GetDeviceCaps(context.0, HORZRES as i32) };
    let printable_h = unsafe { GetDeviceCaps(context.0, VERTRES as i32) };
    if printable_w <= 0 || printable_h <= 0 { bail!("Yazıcının sayfa boyutu okunamadı."); }
    for page in from..=to.min(info.pages) {
        let raster = engine.render(path, password, page, 2400)?;
        let (_, encoded) = raster.data_url.split_once(',').context("Geçersiz sayfa görüntüsü")?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let mut image = image::load_from_memory(&bytes)?.into_rgba8();
        // Windows top-down BI_RGB expects BGRA. Composite alpha against white for paper.
        for pixel in image.pixels_mut() {
            let alpha = pixel[3] as u16;
            let r = ((pixel[0] as u16 * alpha + 255 * (255-alpha))/255) as u8;
            let g = ((pixel[1] as u16 * alpha + 255 * (255-alpha))/255) as u8;
            let b = ((pixel[2] as u16 * alpha + 255 * (255-alpha))/255) as u8;
            pixel.0 = [b,g,r,255];
        }
        let (width,height) = image.dimensions();
        let scale = (printable_w as f64/width as f64).min(printable_h as f64/height as f64);
        let draw_w = (width as f64*scale).round() as i32;
        let draw_h = (height as f64*scale).round() as i32;
        let bitmap = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32, biHeight: -(height as i32), biPlanes: 1,
                biBitCount: 32, biCompression: BI_RGB, ..Default::default()
            }, ..Default::default()
        };
        if unsafe { StartPage(context.0) } <= 0 { bail!("Yazdırma sayfası başlatılamadı."); }
        unsafe { SetStretchBltMode(context.0, HALFTONE as i32); }
        let drawn = unsafe { StretchDIBits(context.0, (printable_w-draw_w)/2,(printable_h-draw_h)/2,draw_w,draw_h,
            0,0,width as i32,height as i32,image.as_ptr().cast(),&bitmap,DIB_RGB_COLORS,SRCCOPY) };
        if drawn <= 0 || unsafe { EndPage(context.0) } <= 0 { bail!("Sayfa yazıcıya gönderilemedi."); }
    }
    if unsafe { EndDoc(context.0) } <= 0 { bail!("Yazdırma tamamlanamadı."); }
    job.completed = true;
    Ok(())
}
