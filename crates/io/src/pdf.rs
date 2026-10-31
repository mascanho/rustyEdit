//! A small PDF writer: pages of filled backgrounds, JPEG pictures, lines
//! and text in the standard Helvetica fonts. Enough for zines, with no
//! other dependency.

use std::fmt::Write as _;

/// Points per millimetre.
const PT: f32 = 72.0 / 25.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Font {
    Regular,
    Bold,
}

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
}

impl PdfPage {
    pub fn new(width_mm: f32, height_mm: f32) -> Self {
        Self {
            width_mm,
            height_mm,
            content: String::new(),
            images: Vec::new(),
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
        let name = match font {
            Font::Regular => "F1",
            Font::Bold => "F2",
        };
        let _ = writeln!(
            self.content,
            "BT /{name} {size_pt:.2} Tf {} rg {:.2} {:.2} Td ({}) Tj ET",
            Self::colour(rgb),
            self.x(left),
            self.y(y),
            escape(text)
        );
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
        let byte = if (c as u32) < 256 && !c.is_control() {
            c as u8
        } else {
            b'?'
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

/// The width of `text` set at `size_pt`, in millimetres.
pub fn text_width_mm(text: &str, size_pt: f32, font: Font) -> f32 {
    let units: u32 = text
        .chars()
        .map(|c| match c as u32 {
            32..=126 => WIDTHS[c as usize - 32] as u32,
            _ => 556,
        })
        .sum();
    let bold = if font == Font::Bold { 1.06 } else { 1.0 };
    units as f32 / 1000.0 * size_pt * bold / PT
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

    // 1 catalog, 2 page tree, 3-4 fonts, 5 info, then images, then each
    // page's content stream followed by the page itself.
    let first_image = 6;
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
    for font in ["Helvetica", "Helvetica-Bold"] {
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
                 /Resources << /Font << /F1 3 0 R /F2 4 0 R >> /XObject << {xobjects}>> >> \
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
            "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n",
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
        page.text("Hello (zine)", 105.0, 200.0, 20.0, Font::Bold, [0, 0, 0], true);
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
        let narrow = text_width_mm("iii", 12.0, Font::Regular);
        let wide = text_width_mm("WWW", 12.0, Font::Regular);
        assert!(wide > narrow * 3.0);
        let long = "A very long title for a small panel";
        let size = fit_text_size(long, 40.0, 30.0, 6.0, Font::Bold);
        assert!((6.0..30.0).contains(&size));
        assert!(text_width_mm(long, size, Font::Bold) <= 41.0);
    }

    #[test]
    fn strings_are_escaped_to_latin_1() {
        assert_eq!(escape("a(b)\\"), "a\\(b\\)\\\\");
        assert_eq!(escape("caf\u{e9}"), "caf\\351");
        assert_eq!(escape("\u{4e2d}"), "?");
    }
}
