//! A small PDF writer: pages of filled backgrounds, JPEG pictures, lines
//! and text in the standard Helvetica fonts. Enough for zines, with no
//! other dependency.

use std::fmt::Write as _;

/// Points per millimetre.
const PT: f32 = 72.0 / 25.4;

/// The three standard type families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Sans,
    Serif,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Font {
    pub family: Family,
    pub bold: bool,
}

#[allow(dead_code)]
impl Font {
    pub const REGULAR: Font = Font::new(Family::Sans, false);
    pub const BOLD: Font = Font::new(Family::Sans, true);

    pub const fn new(family: Family, bold: bool) -> Self {
        Self { family, bold }
    }

    /// The name the page's resources give it.
    fn resource(&self) -> &'static str {
        match (self.family, self.bold) {
            (Family::Sans, false) => "F1",
            (Family::Sans, true) => "F2",
            (Family::Serif, false) => "F3",
            (Family::Serif, true) => "F4",
            (Family::Mono, false) => "F5",
            (Family::Mono, true) => "F6",
        }
    }
}

const FONT_NAMES: [&str; 6] = [
    "Helvetica",
    "Helvetica-Bold",
    "Times-Roman",
    "Times-Bold",
    "Courier",
    "Courier-Bold",
];

/// A JPEG to embed.
#[derive(Debug, Clone)]
pub struct JpegImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A page being drawn. Positions are in millimetres from its top-left.
pub struct PdfPage {
    width_mm: f32,
    height_mm: f32,
    content: String,
    /// Indexes into the document's images used on this page.
    images: Vec<usize>,
    /// The see-through levels (percent) the page paints with.
    alphas: Vec<u32>,
}

impl PdfPage {
    pub fn new(width_mm: f32, height_mm: f32) -> Self {
        Self {
            width_mm,
            height_mm,
            content: String::new(),
            images: Vec::new(),
            alphas: Vec::new(),
        }
    }

    fn x(&self, mm: f32) -> f32 {
        mm * PT
    }

    fn y(&self, mm: f32) -> f32 {
        (self.height_mm - mm) * PT
    }

    fn colour(rgb: [u8; 3]) -> String {
        format!(
            "{:.3} {:.3} {:.3}",
            rgb[0] as f32 / 255.0,
            rgb[1] as f32 / 255.0,
            rgb[2] as f32 / 255.0
        )
    }

    pub fn fill_page(&mut self, rgb: [u8; 3]) {
        let _ = writeln!(
            self.content,
            "{} rg 0 0 {:.2} {:.2} re f",
            Self::colour(rgb),
            self.width_mm * PT,
            self.height_mm * PT
        );
    }

    /// A thin dashed line, for fold marks.
    pub fn dashed_line(&mut self, from: (f32, f32), to: (f32, f32), rgb: [u8; 3], width_pt: f32) {
        let _ = writeln!(
            self.content,
            "{} RG {width_pt:.2} w [2 2] 0 d {:.2} {:.2} m {:.2} {:.2} l S [] 0 d",
            Self::colour(rgb),
            self.x(from.0),
            self.y(from.1),
            self.x(to.0),
            self.y(to.1)
        );
    }

    /// Draws image `index` of the document in the box.
    pub fn image(&mut self, index: usize, x: f32, y: f32, w: f32, h: f32) {
        if !self.images.contains(&index) {
            self.images.push(index);
        }
        let _ = writeln!(
            self.content,
            "q {:.2} 0 0 {:.2} {:.2} {:.2} cm /Im{index} Do Q",
            w * PT,
            h * PT,
            self.x(x),
            self.y(y + h)
        );
    }

    /// Text with its baseline at `y`, centred on `x` when `centred`.
    pub fn text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size_pt: f32,
        font: Font,
        rgb: [u8; 3],
        centred: bool,
    ) {
        let left = if centred {
            x - text_width_mm(text, size_pt, font) / 2.0
        } else {
            x
        };
        let name = font.resource();
        let _ = writeln!(
            self.content,
            "BT /{name} {size_pt:.2} Tf {} rg {:.2} {:.2} Td ({}) Tj ET",
            Self::colour(rgb),
            self.x(left),
            self.y(y),
            escape(text)
        );
    }

    /// The path of a rounded rectangle, in the page's own units.
    fn rounded_path(&self, x: f32, y: f32, w: f32, h: f32, radius_mm: f32) -> String {
        let (l, b) = (self.x(x), self.y(y + h));
        let (w, h) = (w * PT, h * PT);
        let r = (radius_mm * PT).clamp(0.0, w.min(h) / 2.0);
        if r < 0.05 {
            return format!("{l:.2} {b:.2} {w:.2} {h:.2} re");
        }
        let k = r * 0.5523;
        format!(
            "{:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} l \
             {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
             {:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c h",
            l + r, b,
            l + w - r, b,
            l + w - r + k, b, l + w, b + r - k, l + w, b + r,
            l + w, b + h - r,
            l + w, b + h - r + k, l + w - r + k, b + h, l + w - r, b + h,
            l + r, b + h,
            l + r - k, b + h, l, b + h - r + k, l, b + h - r,
            l, b + r,
            l, b + r - k, l + r - k, b, l + r, b,
        )
    }

    /// Fills a rectangle with rounded corners, more or less see-through.
    pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius_mm: f32, rgb: [u8; 3], alpha: f32) {
        let pct = (alpha.clamp(0.0, 1.0) * 100.0).round() as u32;
        let path = self.rounded_path(x, y, w, h, radius_mm);
        if pct < 100 {
            if !self.alphas.contains(&pct) {
                self.alphas.push(pct);
            }
            let _ = writeln!(self.content, "q /GS{pct} gs {} rg {path} f Q", Self::colour(rgb));
        } else {
            let _ = writeln!(self.content, "{} rg {path} f", Self::colour(rgb));
        }
    }

    /// Draws what `draw` adds, cut off outside a rounded rectangle.
    pub fn clipped_to(&mut self, x: f32, y: f32, w: f32, h: f32, radius_mm: f32, draw: impl FnOnce(&mut PdfPage)) {
        let path = self.rounded_path(x, y, w, h, radius_mm);
        let _ = writeln!(self.content, "q {path} W n");
        draw(self);
        self.content.push_str("Q\n");
    }

    /// Draws what `draw` adds turned `degrees` clockwise about `centre`.
    pub fn rotated_by(&mut self, centre: (f32, f32), degrees: f32, draw: impl FnOnce(&mut PdfPage)) {
        if degrees.abs() < 0.01 {
            draw(self);
            return;
        }
        // On the page's own upward axes a clockwise turn is the reverse.
        let phi = (-degrees).to_radians();
        let (sin, cos) = phi.sin_cos();
        let (cx, cy) = (self.x(centre.0), self.y(centre.1));
        let tx = cx - (cos * cx - sin * cy);
        let ty = cy - (sin * cx + cos * cy);
        let _ = writeln!(self.content, "q {cos:.5} {sin:.5} {:.5} {cos:.5} {tx:.2} {ty:.2} cm", -sin);
        draw(self);
        self.content.push_str("Q\n");
    }

    /// Draws what `draw` adds turned half a circle about `centre`.
    pub fn rotated_about(&mut self, centre: (f32, f32), draw: impl FnOnce(&mut PdfPage)) {
        let _ = writeln!(
            self.content,
            "q -1 0 0 -1 {:.2} {:.2} cm",
            2.0 * self.x(centre.0),
            2.0 * self.y(centre.1)
        );
        draw(self);
        self.content.push_str("Q\n");
    }
}

/// Characters in a PDF string: Latin-1 as is, anything else a question mark.
fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let byte = match c {
            // Typographic marks WinAnsi has beyond Latin-1.
            '\u{2018}' => 0x91,
            '\u{2019}' => 0x92,
            '\u{201C}' => 0x93,
            '\u{201D}' => 0x94,
            '\u{2022}' => 0x95,
            '\u{2013}' => 0x96,
            '\u{2014}' => 0x97,
            '\u{2026}' => 0x85,
            '\u{20AC}' => 0x80,
            '\u{2122}' => 0x99,
            c if (c as u32) < 256 && !c.is_control() => c as u8,
            _ => b'?',
        };
        match byte {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7e => out.push(byte as char),
            // Octal, so the file stays plain ASCII.
            other => {
                let _ = write!(out, "\\{other:03o}");
            }
        }
    }
    out
}

/// Helvetica's character widths in thousandths of an em, from space.
const WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// Times Roman's, the same way.
const SERIF_WIDTHS: [u16; 95] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278, 500, 500, 500,
    500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444, 921, 722, 667, 667, 722, 611,
    556, 722, 722, 333, 389, 722, 611, 889, 722, 722, 556, 722, 667, 556, 611, 722, 722, 944, 722,
    722, 611, 333, 278, 333, 469, 500, 333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500,
    278, 778, 500, 500, 500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541,
];

/// The width of `text` set at `size_pt`, in millimetres.
pub fn text_width_mm(text: &str, size_pt: f32, font: Font) -> f32 {
    let units: u32 = text
        .chars()
        .map(|c| match (font.family, c as u32) {
            (Family::Mono, _) => 600,
            (Family::Sans, 32..=126) => WIDTHS[c as usize - 32] as u32,
            (Family::Serif, 32..=126) => SERIF_WIDTHS[c as usize - 32] as u32,
            (Family::Sans, _) => 556,
            (Family::Serif, _) => 500,
        })
        .sum();
    let bold = match (font.bold, font.family) {
        (true, Family::Sans) => 1.06,
        (true, Family::Serif) => 1.04,
        _ => 1.0,
    };
    units as f32 / 1000.0 * size_pt * bold / PT
}

/// `text` broken into lines no wider than `width_mm`, at word breaks (a
/// word wider than the line has a line to itself). Newlines start a line.
pub fn wrap_lines(text: &str, width_mm: f32, size_pt: f32, font: Font) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if line.is_empty() || text_width_mm(&candidate, size_pt, font) <= width_mm {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        lines.push(line);
    }
    lines
}

/// Shrinks `size_pt` from `start` until `text` fits `width_mm` (not below
/// `least`).
pub fn fit_text_size(text: &str, width_mm: f32, start: f32, least: f32, font: Font) -> f32 {
    let mut size = start;
    while size > least && text_width_mm(text, size, font) > width_mm {
        size -= 0.5;
    }
    size.max(least)
}

/// The finished document as bytes.
pub fn finish(pages: Vec<PdfPage>, images: &[JpegImage], title: &str) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let mut object = |out: &mut Vec<u8>, body: &[u8]| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };

    // 1 catalog, 2 page tree, 3-8 fonts, 9 info, then images, then each
    // page's content stream followed by the page itself.
    let first_image = 10;
    let first_page = first_image + images.len();
    let page_obj = |i: usize| first_page + 2 * i + 1;
    object(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..pages.len())
        .map(|i| format!("{} 0 R", page_obj(i)))
        .collect();
    object(
        &mut out,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        )
        .as_bytes(),
    );
    for font in FONT_NAMES {
        object(
            &mut out,
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /{font} /Encoding /WinAnsiEncoding >>"
            )
            .as_bytes(),
        );
    }
    object(
        &mut out,
        format!("<< /Title ({}) /Producer (RustyEdit) >>", escape(title)).as_bytes(),
    );
    for image in images {
        let mut body = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            image.width,
            image.height,
            image.data.len()
        )
        .into_bytes();
        body.extend_from_slice(&image.data);
        body.extend_from_slice(b"\nendstream");
        object(&mut out, &body);
    }
    for (i, page) in pages.iter().enumerate() {
        let gstates: String = page
            .alphas
            .iter()
            .map(|pct| {
                format!(
                    "/GS{pct} << /Type /ExtGState /ca {:.2} >> ",
                    *pct as f32 / 100.0
                )
            })
            .collect();
        let xobjects: String = page
            .images
            .iter()
            .map(|n| format!("/Im{n} {} 0 R ", first_image + n))
            .collect();
        let content = page.content.as_bytes();
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content);
        stream.extend_from_slice(b"\nendstream");
        object(&mut out, &stream);
        object(
            &mut out,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] \
                 /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R /F4 6 0 R /F5 7 0 R /F6 8 0 R >> \
                 /XObject << {xobjects}>> /ExtGState << {gstates}>> >> \
                 /Contents {} 0 R >>",
                page.width_mm * PT,
                page.height_mm * PT,
                first_page + 2 * i
            )
            .as_bytes(),
        );
    }

    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 9 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_jpeg() -> JpegImage {
        let mut data = Vec::new();
        let image = image::RgbImage::from_pixel(8, 6, image::Rgb([200, 30, 30]));
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 80)
            .encode_image(&image)
            .unwrap();
        JpegImage {
            data,
            width: 8,
            height: 6,
        }
    }

    fn text_of(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|&b| if b.is_ascii() { b as char } else { '.' })
            .collect()
    }

    #[test]
    fn a_document_has_pages_images_text_and_a_valid_xref() {
        let mut page = PdfPage::new(210.0, 297.0);
        page.fill_page([255, 255, 255]);
        page.image(0, 10.0, 10.0, 100.0, 75.0);
        page.text("Hello (zine)", 105.0, 200.0, 20.0, Font::BOLD, [0, 0, 0], true);
        let mut second = PdfPage::new(210.0, 297.0);
        second.rotated_about((105.0, 148.0), |p| p.image(0, 0.0, 0.0, 50.0, 50.0));
        let bytes = finish(vec![page, second], &[tiny_jpeg()], "My zine");
        let text = text_of(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 2"));
        assert!(text.contains("/Subtype /Image"));
        assert!(text.contains("/DCTDecode"));
        assert!(text.contains("(Hello \\(zine\\)) Tj"));
        assert!(text.contains("/Title (My zine)"));
        assert!(text.contains("q -1 0 0 -1"));
        assert!(text.trim_end().ends_with("%%EOF"));

        // Every xref entry points at its "n 0 obj".
        let xref_at: usize = text
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let table = &text[xref_at..];
        let entries: Vec<&str> = table
            .lines()
            .skip(3)
            .take_while(|l| l.ends_with(" n "))
            .collect();
        assert!(entries.len() >= 9);
        for (i, entry) in entries.iter().enumerate() {
            let at: usize = entry[..10].parse().unwrap();
            assert!(
                text[at..].starts_with(&format!("{} 0 obj", i + 1)),
                "object {} at {at}",
                i + 1
            );
        }
    }

    #[test]
    fn text_is_measured_and_fitted() {
        let narrow = text_width_mm("iii", 12.0, Font::REGULAR);
        let wide = text_width_mm("WWW", 12.0, Font::REGULAR);
        assert!(wide > narrow * 3.0);
        let long = "A very long title for a small panel";
        let size = fit_text_size(long, 40.0, 30.0, 6.0, Font::BOLD);
        assert!((6.0..30.0).contains(&size));
        assert!(text_width_mm(long, size, Font::BOLD) <= 41.0);
    }

    #[test]
    fn strings_are_escaped_to_latin_1() {
        assert_eq!(escape("a(b)\\"), "a\\(b\\)\\\\");
        assert_eq!(escape("caf\u{e9}"), "caf\\351");
        assert_eq!(escape("\u{4e2d}"), "?");
        // Curly quotes and dashes are in WinAnsi.
        assert_eq!(escape("\u{201C}hi\u{201D} \u{2014} it\u{2019}s"), "\\223hi\\224 \\227 it\\222s");
    }
}
