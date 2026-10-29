// The Rust engine reports errors and progress in English (stable, greppable logs).
// The UI is Turkish, so engine text is translated here at the bridge boundary.
// Rules are ordered: specific patterns first, then generic fallbacks.

type Rule = [RegExp, string | ((...groups: string[]) => string)];

const RULES: Rule[] = [
  // Passwords and unreadable files.
  [/incorrect password|password-protected|PasswordError|provided password/i, 'Parola yanlış veya belge bozuk. Parolayı kontrol edip yeniden deneyin.'],
  [/^PDF file not found: '(.+)'/, (p) => `PDF dosyası bulunamadı: '${p}'`],
  [/not a valid \.docx package/i, 'Seçilen dosya geçerli bir Word (.docx) belgesi değil.'],
  [/Word document body is too large/i, 'Word belgesi dönüştürülemeyecek kadar büyük.'],
  [/Word document (contains no readable text|has no word\/document\.xml part)/i, 'Word belgesinde okunabilir metin bulunamadı.'],
  [/This PDF contains no readable text/i, 'Bu PDF okunabilir metin içermiyor. Taranmış bir belgeyse OCR kullanın.'],
  [/No readable text found in the image/i, 'Görselde okunabilir metin bulunamadı.'],
  [/scan and the local OCR component is not installed|local OCR component is not installed|Tesseract executable was not found/i, "Bu işlem için yerel OCR bileşeni gerekli. 'Yerel OCR' panelinden kurun (tek seferlik indirme)."],
  [/^Tesseract OCR failed: ?(.*)/, (d) => `OCR başarısız oldu${d ? `: ${d}` : '.'}`],
  [/Secure random source unavailable/i, 'Güvenli rastgele sayı kaynağına erişilemedi; şifreleme yapılamadı.'],

  // Output safety.
  [/^Output file '(.+)' already exists/, (p) => `Çıktı dosyası zaten var: '${p}'. Farklı bir ad seçin; mevcut dosyaların üzerine asla yazılmaz.`],
  [/^Output directory '(.+)' is not empty/, (p) => `Çıktı klasörü boş değil: '${p}'. Boş bir klasör seçin; mevcut dosyaların üzerine asla yazılmaz.`],
  [/collides with input file|contains input file|collides directly with input/i, 'Çıktı, giriş dosyasıyla aynı yeri gösteriyor. Giriş dosyalarının üzerine yazılmaz; başka bir konum seçin.'],
  [/output must end with \.docx/i, 'Çıktı dosyasının uzantısı .docx olmalı.'],
  [/output must end with \.pdf/i, 'Çıktı dosyasının uzantısı .pdf olmalı.'],

  // Page ranges.
  [/^Page (\d+) is out of bounds \(document has (\d+) pages\)/, (p, n) => `${p}. sayfa yok (belgede ${n} sayfa var).`],
  [/^(?:Start |End )?[Pp]age (\d+) exceeds total document pages \((\d+)\)/, (p, n) => `${p}. sayfa yok (belgede ${n} sayfa var).`],
  [/^Invalid page number '(.*?)'/, (p) => `Geçersiz sayfa numarası: '${p}'. Örnek: 1,3-5`],
  [/1-based/, 'Sayfa numaraları 1\'den başlar.'],
  [/^No valid pages specified|^No pages specified/, 'Geçerli sayfa belirtilmedi. Örnek: 1,3-5'],
  [/empty document \(0 pages\)|has 0 pages/, 'Belgede hiç sayfa yok.'],
  [/Cannot remove all pages/, 'Tüm sayfalar silinemez; en az bir sayfa kalmalı.'],
  [/requires 'pages' parameter/, 'Sayfa aralığını girin. Örnek: 5-1 veya 3,1,2'],

  // Missing inputs / parameters.
  [/requires a non-empty 'newPassword'|requires 'password' parameter/, 'Parola girin.'],
  [/requires non-empty 'text'/, 'Filigran metnini girin.'],
  [/requires at least one 'annotation'/, 'En az bir işaretleme ekleyin.'],
  [/requires 'imagePath'/, 'İmza görselini seçin.'],
  [/requires at least 2 coordinate points/, 'Çizim için en az iki nokta gerekli.'],
  [/requires (?:an|at least one) input (?:PDF )?(?:document|image|\.docx document|image file)/i, 'Önce bir giriş dosyası seçin.'],
  [/cancelled/i, 'İşlem iptal edildi.'],

  // Progress lines.
  [/^Checking output destination/, 'Çıktı konumu denetleniyor...'],
  [/^Loading document/, 'Belge yükleniyor...'],
  [/^Applying AES-256 PDF encryption/, 'AES-256 şifreleme uygulanıyor...'],
  [/^Applying Flate stream compression/, 'Akışlar sıkıştırılıyor...'],
  [/^Pruning unused/, 'Kullanılmayan nesneler temizleniyor...'],
  [/^Decrypting document/, 'Parola kaldırılıyor...'],
  [/^Running local OCR/, 'Yerel OCR çalışıyor...'],
  [/^Starting merge of (\d+) documents \((\d+) pages\)/, (d, p) => `${d} belge birleştiriliyor (${p} sayfa)...`],
  [/^Applying (\d+) annotations/, (n) => `${n} işaretleme uygulanıyor...`],
  [/^(?:Merged|Numbered|Split|Processed) page (\d+) of (\d+)/, (a, b) => `Sayfa ${a} / ${b}`],
  [/^Applied watermark to page (\d+) of (\d+)/, (a, b) => `Filigran: sayfa ${a} / ${b}`],
  [/^Exported page (\d+) of (\d+)/, (a, b) => `Görsel: sayfa ${a} / ${b}`],
  [/^Converted image (\d+) of (\d+)/, (a, b) => `Görsel ${a} / ${b} dönüştürüldü`],
  [/^OCR recognized page (\d+) of (\d+)/, (a, b) => `OCR: sayfa ${a} / ${b}`],
  [/^Writing paragraph (\d+) of (\d+)/, (a, b) => `Paragraf ${a} / ${b} yazılıyor`],
  [/^Writing block (\d+) of (\d+)/, (a, b) => `Bölüm ${a} / ${b} yazılıyor`],
  [/^(?:Extracted|Reordered|Copied) page (\d+) \((\d+) of (\d+)/, (_p, a, b) => `Sayfa ${a} / ${b}`],
  [/^Extracted text from page \d+ \((\d+) of (\d+)\)/, (a, b) => `Metin: sayfa ${a} / ${b}`],
  [/^Applied '.*' annotation to page (\d+)/, (p) => `${p}. sayfaya işaretleme uygulandı`],

  // Results.
  [/^Document encrypted successfully|^Successfully protected/, 'Belge AES-256 ile parola korumasına alındı.'],
  [/^Document unlocked|^Successfully removed password/, 'Parola koruması kaldırıldı.'],
  [/^Successfully split document into (\d+)/, (n) => `Belge ${n} ayrı PDF dosyasına bölündü.`],
  [/^Successfully extracted text from (\d+) pages/, (n) => `${n} sayfanın metni çıkarıldı.`],
  [/^Successfully extracted (\d+) pages/, (n) => `${n} sayfa çıkarıldı.`],
  [/^Successfully exported (\d+) pages to PNG images at (\d+) DPI/, (n, d) => `${n} sayfa ${d} DPI PNG olarak dışa aktarıldı.`],
  [/^Successfully created PDF from (\d+) images/, (n) => `${n} görselden PDF oluşturuldu.`],
  [/^Successfully performed OCR on (\d+) pages/, (n) => `${n} sayfada OCR tamamlandı.`],
  [/^Successfully added page numbers to (\d+) pages/, (n) => `${n} sayfaya numara eklendi.`],
  [/^Successfully applied (\d+) annotations/, (n) => `${n} işaretleme uygulandı.`],
  [/^Successfully reordered document \((\d+) pages\)/, (n) => `Sayfalar yeniden sıralandı (${n} sayfa).`],
  [/^Successfully watermarked (\d+) pages/, (n) => `${n} sayfaya filigran eklendi.`],
  [/^Word document created/, 'Word belgesi oluşturuldu.'],
  [/^Successfully merged (\d+) documents \((\d+) pages total\)/, (d, p) => `${d} belge birleştirildi (toplam ${p} sayfa).`],
  [/^Successfully removed (\d+) pages \((\d+) pages remaining\)/, (n, k) => `${n} sayfa silindi (${k} sayfa kaldı).`],
  [/^Successfully rotated (\d+) pages by (\d+) degrees/, (n, d) => `${n} sayfa ${d}° döndürüldü.`],
  [/^Successfully applied redactions: (\d+) pages permanently raster-burned, (\d+) vector pages preserved/, (b, v) => `Sansür kalıcı olarak uygulandı: ${b} sayfa görüntüye çevrildi, ${v} sayfa olduğu gibi korundu.`],
  [/^Lossless compression complete \(pruned (\d+) unreferenced objects, size: (\d+) KB -> (\d+) KB, ([\d.]+)% saved\)/, (_o, a, b, p) => `Kayıpsız sıkıştırma tamamlandı: ${a} KB → ${b} KB (%${p} kazanç).`],
  [/^Compressed from (\d+) KB to (\d+) KB \(([\d.-]+)% reduction\)/, (a, b, p) => `${a} KB → ${b} KB (%${p} küçüldü)`],
  [/^Created Word document with (\d+) paragraph\(s\)(?:, (\d+) table\(s\))?(?:, (\d+) image\(s\))? from (\d+) page\(s\)(?:; (\d+) page\(s\) recognised with local OCR)?/,
    (p, tb, im, n, o) => `Word belgesi oluşturuldu: ${n} sayfadan ${p} paragraf${tb ? `, ${tb} tablo` : ''}${im ? `, ${im} görsel` : ''}${o ? `; ${o} sayfa yerel OCR ile tanındı` : ''}.`],
  [/^Converted Word document \((\d+) paragraph\(s\)(?:, (\d+) table\(s\))?(?:, (\d+) image\(s\))?\) to PDF(?:; (\d+) unsupported image\(s\) skipped)?/,
    (p, tb, im, sk) => `Word belgesi PDF'e dönüştürüldü (${p} paragraf${tb ? `, ${tb} tablo` : ''}${im ? `, ${im} görsel` : ''}).${sk ? ` Desteklenmeyen ${sk} görsel (EMF/WMF gibi) eklenemedi.` : ''}`],
  [/^Created Word document from image \((\d+) paragraph\(s\), local OCR\)/, (p) => `Görselden Word belgesi oluşturuldu (${p} paragraf, yerel OCR).`],
  [/^Page (\d+) of (\d+) \((scanned page, local OCR|empty page|text layer)\)/, (a, b, k) => `Sayfa ${a} / ${b} (${k === 'text layer' ? 'metin katmanı' : k === 'empty page' ? 'boş sayfa' : 'taranmış sayfa, yerel OCR'})`],

  // Generic engine failures (keep the technical detail for support).
  [/^Failed (?:saving|to save) .*?'(.+?)'/, (p) => `Çıktı kaydedilemedi: '${p}'. Klasöre yazma izniniz olduğundan emin olun.`],
  [/^Failed (?:opening|to open) .*?'(.+?)'/, (p) => `Belge açılamadı: '${p}'. Dosya bozuk veya desteklenmiyor olabilir.`],
  [/^(?:Unknown or unsupported PDF operation kind|Unknown worker command action)/, 'Bilinmeyen PDF işlemi.'],
];

/** Heuristic: text that is already Turkish (or not English) is left alone. */
function looksTurkish(text: string): boolean {
  return /[çğıöşüÇĞİÖŞÜ]/.test(text);
}

/** Translates a single engine message to Turkish. Unknown English text gets a Turkish lead-in. */
export function translateEngineMessage(raw: string): string {
  const message = raw.replace(/^Error:\s*/, '').trim();
  if (!message) return message;
  for (const [pattern, replacement] of RULES) {
    const match = message.match(pattern);
    if (match) {
      return typeof replacement === 'string' ? replacement : replacement(...match.slice(1));
    }
  }
  if (looksTurkish(message)) return message;
  if (/^Failed |error|invalid|unsupported|corrupt/i.test(message)) {
    return `PDF işlemi tamamlanamadı. (Ayrıntı: ${message})`;
  }
  return message;
}

/**
 * Translates an error coming back from a Tauri command. `open_documents` reports
 * failures as a JSON object `{path, message}`; that shape is preserved.
 */
export function translateEngineError(error: unknown): string {
  const text = typeof error === 'string' ? error : error instanceof Error ? error.message : String(error ?? '');
  if (text.startsWith('{')) {
    try {
      const parsed = JSON.parse(text) as { path?: string; message?: string };
      if (parsed && typeof parsed.message === 'string') {
        return JSON.stringify({ ...parsed, message: translateEngineMessage(parsed.message) });
      }
    } catch {
      // Not JSON; fall through.
    }
  }
  return translateEngineMessage(text);
}
