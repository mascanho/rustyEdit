//! XMP sidecar files: a small `.xmp` file next to each photo holding its
//! rating, reject flag, colour label, keywords and descriptive metadata in
//! the standard places other apps (Lightroom, Bridge, Capture One,
//! darktable) read, plus optionally the edits.
//!
//! - Naming: RAW files use `IMG_0001.xmp` (what Lightroom and Bridge
//!   expect); other files use `photo.jpg.xmp`, so a RAW and a JPEG of the
//!   same shot never share one.
//! - Safety: a sidecar is only ever (over)written if this app made it.
//!   Sidecars from other apps can hold much more than this app knows, so
//!   they're left alone. Writes go to a temporary file renamed into place.
//! - Edits: the basic tone sliders are written as Camera Raw settings
//!   (`crs:`) that Lightroom understands, and this app's complete settings
//!   in its own namespace, so they come back exactly.

use std::path::{Path, PathBuf};

use rustyedit_core::{Asset, ColorLabel, DevelopSettings, Flag, Rating};

/// Marks sidecars this app wrote (and may therefore rewrite).
const OWN_NAMESPACE: &str = "https://rustyedit.app/xmp/1.0/";

/// Camera RAW formats, whose sidecars drop the extension (as Lightroom
/// and Bridge name them).
const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "ari", "arw", "bay", "cr2", "cr3", "crw", "dcr", "dng", "erf", "fff", "iiq", "k25",
    "kdc", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "raf", "raw", "rw2", "rwl", "sr2",
    "srf", "srw", "x3f",
];

/// Where `photo`'s sidecar lives.
pub fn sidecar_path(photo: &Path) -> PathBuf {
    let is_raw = photo
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RAW_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if is_raw {
        photo.with_extension("xmp")
    } else {
        let mut name = photo.as_os_str().to_owned();
        name.push(".xmp");
        PathBuf::from(name)
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// The sidecar for `asset`.
pub fn to_xmp(asset: &Asset, include_edits: bool) -> String {
    let rating: i32 = if asset.flag == Flag::Rejected {
        -1
    } else {
        asset.rating.value() as i32
    };
    let flag = match asset.flag {
        Flag::Picked => "picked",
        Flag::Rejected => "rejected",
        Flag::Unflagged => "unflagged",
    };
    let mut attrs = vec![
        format!(r#"xmp:Rating="{rating}""#),
        format!(r#"rustyedit:Flag="{flag}""#),
    ];
    if asset.color_label != ColorLabel::None {
        attrs.push(format!(r#"xmp:Label="{}""#, asset.color_label.label()));
    }
    let d = &asset.develop;
    if include_edits && d.is_edited() {
        // The basic sliders that mean the same in Camera Raw.
        attrs.push(r#"crs:HasSettings="True""#.to_string());
        attrs.push(format!(r#"crs:Exposure2012="{:+.2}""#, d.exposure / 50.0));
        for (name, value) in [
            ("Contrast2012", d.contrast),
            ("Highlights2012", d.highlights),
            ("Shadows2012", d.shadows),
            ("Whites2012", d.whites),
            ("Blacks2012", d.blacks),
            ("Vibrance", d.vibrance),
            ("Saturation", d.saturation),
            ("Clarity2012", d.clarity),
            ("Dehaze", d.dehaze),
        ] {
            attrs.push(format!(r#"crs:{name}="{:+.0}""#, value));
        }
        // A plain crop, as Camera Raw writes it (fractions from each edge).
        // Straightened or perspective-corrected crops mean something else
        // there, so they're left to this app's own copy of the settings.
        let (x, y, w, h) = d.crop.rect;
        if d.crop.angle == 0.0 && !d.crop.has_perspective() && !d.crop.is_identity() {
            attrs.push(r#"crs:HasCrop="True""#.to_string());
            attrs.push(format!(r#"crs:CropLeft="{:.6}""#, x));
            attrs.push(format!(r#"crs:CropTop="{:.6}""#, y));
            attrs.push(format!(r#"crs:CropRight="{:.6}""#, x + w));
            attrs.push(format!(r#"crs:CropBottom="{:.6}""#, y + h));
            attrs.push(r#"crs:CropAngle="0""#.to_string());
        }
    }

    let mut body = String::new();
    let lang_alt = |tag: &str, value: &str| {
        format!(
            "   <{tag}>\n    <rdf:Alt>\n     <rdf:li xml:lang=\"x-default\">{}</rdf:li>\n    </rdf:Alt>\n   </{tag}>\n",
            escape(value)
        )
    };
    if !asset.title.trim().is_empty() {
        body.push_str(&lang_alt("dc:title", asset.title.trim()));
    }
    if !asset.caption.trim().is_empty() {
        body.push_str(&lang_alt("dc:description", asset.caption.trim()));
    }
    if !asset.copyright.trim().is_empty() {
        body.push_str(&lang_alt("dc:rights", asset.copyright.trim()));
    }
    if !asset.creator.trim().is_empty() {
        body.push_str(&format!(
            "   <dc:creator>\n    <rdf:Seq>\n     <rdf:li>{}</rdf:li>\n    </rdf:Seq>\n   </dc:creator>\n",
            escape(asset.creator.trim())
        ));
    }
    if !asset.keywords.is_empty() {
        body.push_str("   <dc:subject>\n    <rdf:Bag>\n");
        for keyword in &asset.keywords {
            body.push_str(&format!("     <rdf:li>{}</rdf:li>\n", escape(keyword)));
        }
        body.push_str("    </rdf:Bag>\n   </dc:subject>\n");
    }
    if include_edits && d.is_edited() {
        if let Ok(json) = serde_json::to_string(d) {
            body.push_str(&format!("   <rustyedit:Develop>{}</rustyedit:Develop>\n", escape(&json)));
        }
    }

    format!(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="RustyEdit">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    xmlns:rustyedit="{OWN_NAMESPACE}"
    {}>
{body}  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#,
        attrs.join("\n    ")
    )
}

/// What happened when writing one sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Written,
    /// Already up to date.
    Unchanged,
    /// Another app's sidecar is there; it was left alone.
    Foreign,
}

/// Writes `xmp` as `photo`'s sidecar, unless an identical one is there or
/// another app made the one that is.
pub fn write_sidecar(photo: &Path, xmp: &str) -> anyhow::Result<WriteOutcome> {
    let path = sidecar_path(photo);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing == xmp {
            return Ok(WriteOutcome::Unchanged);
        }
        if !existing.contains(OWN_NAMESPACE) {
            return Ok(WriteOutcome::Foreign);
        }
    }
    let tmp = path.with_extension("xmp.tmp");
    std::fs::write(&tmp, xmp)?;
    std::fs::rename(&tmp, &path)?;
    Ok(WriteOutcome::Written)
}

/// How bringing a catalog's sidecars up to date went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub written: usize,
    /// Left alone because another app made them.
    pub foreign: usize,
    pub failed: usize,
}

/// Brings the sidecars of `assets` up to date. `known` remembers what each
/// photo's sidecar was last found or made to be (a hash), so after the
/// first pass only photos that changed touch the disk. Virtual copies and
/// missing photos have no sidecar.
pub fn sync_sidecars(
    assets: &[Asset],
    include_edits: bool,
    known: &mut std::collections::HashMap<PathBuf, u64>,
) -> SyncReport {
    use std::hash::{Hash, Hasher};
    let mut report = SyncReport::default();
    for asset in assets {
        if asset.missing || asset.is_virtual_copy() {
            continue;
        }
        let xmp = to_xmp(asset, include_edits);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        xmp.hash(&mut hasher);
        let hash = hasher.finish();
        if known.get(&asset.path) == Some(&hash) {
            continue;
        }
        match write_sidecar(&asset.path, &xmp) {
            Ok(outcome) => {
                match outcome {
                    WriteOutcome::Written => report.written += 1,
                    WriteOutcome::Foreign => report.foreign += 1,
                    WriteOutcome::Unchanged => {}
                }
                known.insert(asset.path.clone(), hash);
            }
            Err(_) => report.failed += 1,
        }
    }
    report
}

/// What a sidecar says about a photo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SidecarData {
    pub rating: Option<Rating>,
    pub flag: Option<Flag>,
    pub color_label: Option<ColorLabel>,
    pub keywords: Vec<String>,
    pub title: Option<String>,
    pub caption: Option<String>,
    pub creator: Option<String>,
    pub copyright: Option<String>,
    /// Only from sidecars this app wrote.
    pub develop: Option<DevelopSettings>,
    /// The Camera Raw (`crs:`) edits another app (Lightroom, Bridge)
    /// wrote, as far as this app has matching tools: the basic tone and
    /// colour sliders and a plain crop.
    pub camera_raw: Option<DevelopSettings>,
}

impl SidecarData {
    /// Gives `asset` what the sidecar has (keeping its own where the
    /// sidecar says nothing).
    pub fn apply_to(&self, asset: &mut Asset, with_edits: bool) {
        if let Some(rating) = self.rating {
            asset.rating = rating;
        }
        if let Some(flag) = self.flag {
            asset.flag = flag;
        }
        if let Some(label) = self.color_label {
            asset.color_label = label;
        }
        for keyword in &self.keywords {
            asset.add_keyword(keyword);
        }
        let set = |field: &mut String, value: &Option<String>| {
            if let Some(value) = value {
                *field = value.clone();
            }
        };
        set(&mut asset.title, &self.title);
        set(&mut asset.caption, &self.caption);
        set(&mut asset.creator, &self.creator);
        set(&mut asset.copyright, &self.copyright);
        if with_edits {
            if let Some(develop) = self.develop {
                asset.develop = develop;
            } else if let Some(develop) = self.camera_raw {
                // Another app's edits only start a photo off; they never
                // replace edits made here.
                if !asset.develop.is_edited() {
                    asset.develop = develop;
                }
            }
        }
    }
}

/// Reads `photo`'s sidecar, if it has one this app can make sense of.
pub fn read_sidecar(photo: &Path) -> Option<SidecarData> {
    let text = std::fs::read_to_string(sidecar_path(photo)).ok()?;
    parse_xmp(&text)
}

fn parse_xmp(text: &str) -> Option<SidecarData> {
    const XMP: &str = "http://ns.adobe.com/xap/1.0/";
    const DC: &str = "http://purl.org/dc/elements/1.1/";
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let doc = roxmltree::Document::parse(text).ok()?;
    let mut data = SidecarData::default();
    let own = text.contains(OWN_NAMESPACE);

    // A property may be an attribute of rdf:Description or an element.
    let simple = |ns: &str, name: &str| -> Option<String> {
        doc.descendants().find_map(|node| {
            if node.has_tag_name((RDF, "Description")) {
                if let Some(value) = node.attribute((ns, name)) {
                    return Some(value.to_string());
                }
            }
            (node.has_tag_name((ns, name)) && node.children().all(|c| c.is_text()))
                .then(|| node.text().unwrap_or_default().trim().to_string())
        })
    };
    // The items of a list property (rdf:Bag / Seq / Alt).
    let items = |ns: &str, name: &str| -> Vec<String> {
        doc.descendants()
            .filter(|node| node.has_tag_name((ns, name)))
            .flat_map(|node| {
                node.descendants()
                    .filter(|n| n.has_tag_name((RDF, "li")))
                    .filter_map(|n| n.text().map(|t| t.trim().to_string()))
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
            })
            .collect()
    };

    if let Some(rating) = simple(XMP, "Rating").and_then(|r| r.trim().parse::<f32>().ok()) {
        if rating < 0.0 {
            data.flag = Some(Flag::Rejected);
        } else {
            data.rating = Some(Rating::new(rating.round().clamp(0.0, 5.0) as u8));
        }
    }
    if let Some(flag) = simple(OWN_NAMESPACE, "Flag") {
        data.flag = match flag.as_str() {
            "picked" => Some(Flag::Picked),
            "rejected" => Some(Flag::Rejected),
            "unflagged" => Some(Flag::Unflagged),
            _ => data.flag,
        };
    }
    if let Some(label) = simple(XMP, "Label") {
        data.color_label = ColorLabel::ALL
            .into_iter()
            .find(|l| l.label().eq_ignore_ascii_case(label.trim()));
    }
    data.keywords = items(DC, "subject");
    let first = |name: &str| items(DC, name).into_iter().next();
    data.title = first("title");
    data.caption = first("description");
    data.creator = first("creator");
    data.copyright = first("rights");
    if !own {
        data.camera_raw = camera_raw_settings(&simple);
    }
    if own {
        data.develop = doc
            .descendants()
            .find(|n| n.has_tag_name((OWN_NAMESPACE, "Develop")))
            .and_then(|n| n.text())
            .and_then(|json| serde_json::from_str(json).ok());
    }
    Some(data)
}

/// The edits a Camera Raw (`crs:`) sidecar holds that this app has tools
/// for, or `None` if it has none.
fn camera_raw_settings(simple: &dyn Fn(&str, &str) -> Option<String>) -> Option<DevelopSettings> {
    const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
    let number = |name: &str| {
        simple(CRS, name).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite())
    };
    let mut d = DevelopSettings::default();
    let mut found = false;
    let mut set = |field: &mut f32, value: Option<f32>, scale: f32| {
        if let Some(value) = value {
            *field = (value * scale).clamp(-100.0, 100.0);
            found |= value != 0.0;
        }
    };
    // This app's exposure runs -100..100 for Camera Raw's -5..+5 stops.
    set(&mut d.exposure, number("Exposure2012"), 50.0);
    set(&mut d.contrast, number("Contrast2012"), 1.0);
    set(&mut d.highlights, number("Highlights2012"), 1.0);
    set(&mut d.shadows, number("Shadows2012"), 1.0);
    set(&mut d.whites, number("Whites2012"), 1.0);
    set(&mut d.blacks, number("Blacks2012"), 1.0);
    set(&mut d.vibrance, number("Vibrance"), 1.0);
    set(&mut d.saturation, number("Saturation"), 1.0);
    set(&mut d.clarity, number("Clarity2012"), 1.0);
    set(&mut d.dehaze, number("Dehaze"), 1.0);

    let has_crop = simple(CRS, "HasCrop").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let angle = number("CropAngle").unwrap_or(0.0);
    if has_crop && angle == 0.0 {
        if let (Some(l), Some(t), Some(r), Some(b)) = (
            number("CropLeft"),
            number("CropTop"),
            number("CropRight"),
            number("CropBottom"),
        ) {
            if r > l && b > t {
                d.crop.rect = (l, t, r - l, b - t);
                d.crop = d.crop.clamped();
                found |= !d.crop.is_identity();
            }
        }
    }
    found.then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo() -> Asset {
        let mut asset = Asset::new(0, PathBuf::from("/photos/IMG_1.jpg"));
        asset.rating = Rating::new(4);
        asset.flag = Flag::Picked;
        asset.color_label = ColorLabel::Green;
        asset.keywords = vec!["Beach".into(), "Rock & Roll".into()];
        asset.title = "Sunset <3".into();
        asset.creator = "Marco".into();
        asset.develop.exposure = 25.0;
        asset.develop.temperature = 12.0;
        asset
    }

    #[test]
    fn round_trips_everything_it_writes() {
        let asset = photo();
        let data = parse_xmp(&to_xmp(&asset, true)).unwrap();
        assert_eq!(data.rating, Some(Rating::new(4)));
        assert_eq!(data.flag, Some(Flag::Picked));
        assert_eq!(data.color_label, Some(ColorLabel::Green));
        assert_eq!(data.keywords, asset.keywords);
        assert_eq!(data.title.as_deref(), Some("Sunset <3"));
        assert_eq!(data.creator.as_deref(), Some("Marco"));
        assert_eq!(data.develop, Some(asset.develop));

        let mut fresh = Asset::new(1, asset.path.clone());
        data.apply_to(&mut fresh, true);
        assert_eq!(fresh.rating, asset.rating);
        assert_eq!(fresh.develop, asset.develop);
    }

    #[test]
    fn edits_can_be_left_out_and_rejects_rate_minus_one() {
        let mut asset = photo();
        asset.flag = Flag::Rejected;
        let xmp = to_xmp(&asset, false);
        assert!(xmp.contains(r#"xmp:Rating="-1""#));
        assert!(!xmp.contains("crs:") || !xmp.contains("crs:Exposure2012"));
        assert_eq!(parse_xmp(&xmp).unwrap().develop, None);
    }

    #[test]
    fn reads_a_lightroom_style_sidecar() {
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
 <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/"
   xmp:Rating="3" xmp:Label="Red">
  <dc:subject><rdf:Bag><rdf:li>Travel</rdf:li><rdf:li>Lisbon</rdf:li></rdf:Bag></dc:subject>
 </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let data = parse_xmp(xmp).unwrap();
        assert_eq!(data.rating, Some(Rating::new(3)));
        assert_eq!(data.color_label, Some(ColorLabel::Red));
        assert_eq!(data.keywords, vec!["Travel", "Lisbon"]);
        // Not ours: edits are never taken from it.
        assert_eq!(data.develop, None);
    }

    #[test]
    fn takes_camera_raw_edits_from_a_lightroom_sidecar() {
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
 <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:Exposure2012="+0.50" crs:Contrast2012="+12" crs:Shadows2012="+30" crs:Vibrance="-5"
   crs:HasCrop="True" crs:CropLeft="0.1" crs:CropTop="0.2" crs:CropRight="0.9" crs:CropBottom="0.8" crs:CropAngle="0"/>
 </rdf:RDF></x:xmpmeta>"#;
        let data = parse_xmp(xmp).unwrap();
        let d = data.camera_raw.unwrap();
        assert!((d.exposure - 25.0).abs() < 1e-4);
        assert_eq!((d.contrast, d.shadows, d.vibrance), (12.0, 30.0, -5.0));
        let (x, y, w, h) = d.crop.rect;
        assert!((x - 0.1).abs() < 1e-5 && (y - 0.2).abs() < 1e-5);
        assert!((w - 0.8).abs() < 1e-5 && (h - 0.6).abs() < 1e-5);

        // They start an unedited photo off, and never replace its edits.
        let mut fresh = Asset::new(1, PathBuf::from("/p/a.jpg"));
        data.apply_to(&mut fresh, true);
        assert_eq!(fresh.develop.contrast, 12.0);
        let mut edited = Asset::new(2, PathBuf::from("/p/b.jpg"));
        edited.develop.contrast = -40.0;
        data.apply_to(&mut edited, true);
        assert_eq!(edited.develop.contrast, -40.0);
        // Without "read edits" they are ignored.
        let mut plain = Asset::new(3, PathBuf::from("/p/c.jpg"));
        data.apply_to(&mut plain, false);
        assert!(!plain.develop.is_edited());
    }

    #[test]
    fn writes_a_plain_crop_for_camera_raw() {
        let mut asset = photo();
        asset.develop.crop.rect = (0.1, 0.2, 0.8, 0.6);
        let xmp = to_xmp(&asset, true);
        assert!(xmp.contains(r#"crs:CropRight="0.900000""#));
        assert!(xmp.contains(r#"crs:CropBottom="0.800000""#));
        asset.develop.crop.angle = 3.0;
        assert!(!to_xmp(&asset, true).contains("crs:CropLeft"));
    }

    #[test]
    fn never_overwrites_another_apps_sidecar() {
        let dir = std::env::temp_dir().join(format!("rustyedit-xmp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let photo_path = dir.join("shot.jpg");
        let foreign = "<x:xmpmeta>from another app</x:xmpmeta>";
        std::fs::write(sidecar_path(&photo_path), foreign).unwrap();
        let ours = to_xmp(&photo(), true);
        assert_eq!(write_sidecar(&photo_path, &ours).unwrap(), WriteOutcome::Foreign);
        assert_eq!(std::fs::read_to_string(sidecar_path(&photo_path)).unwrap(), foreign);

        std::fs::remove_file(sidecar_path(&photo_path)).unwrap();
        assert_eq!(write_sidecar(&photo_path, &ours).unwrap(), WriteOutcome::Written);
        assert_eq!(write_sidecar(&photo_path, &ours).unwrap(), WriteOutcome::Unchanged);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn raw_files_use_the_lightroom_name() {
        assert_eq!(sidecar_path(Path::new("/p/IMG_1.CR3")), PathBuf::from("/p/IMG_1.xmp"));
        assert_eq!(sidecar_path(Path::new("/p/IMG_1.jpg")), PathBuf::from("/p/IMG_1.jpg.xmp"));
    }
}
