//! What the import filter needs to know about each photo of a folder: its
//! kind, the rating and colour label set in the camera or another program
//! (from the sidecar, or the XMP packet inside the file), and whether the
//! file is protected.

use std::{fs::File, io::Read, path::Path};

use rustyedit_core::{Candidate, ColorLabel, FileKind};

/// How much of a file is searched for its XMP packet.
const XMP_SEARCH_BYTES: u64 = 256 * 1024;

/// Every photo under `folder` with what the filter looks at. Reads each
/// file's start; call off the UI thread.
pub fn read_import_candidates(folder: &Path) -> Vec<Candidate> {
    crate::scan::scan_paths(folder)
        .into_iter()
        .map(|path| candidate(path))
        .collect()
}

fn candidate(path: std::path::PathBuf) -> Candidate {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let kind = FileKind::of_extension(&ext, crate::decode::is_raw_file(&path));
    let (rating, label) = marks(&path);
    Candidate { protected: is_protected(&path), path, kind, rating, label }
}

/// The rating and label set outside this app: the sidecar's, else the
/// file's own.
fn marks(path: &Path) -> (u8, ColorLabel) {
    let data = crate::xmp::read_sidecar(path).or_else(|| embedded_xmp(path));
    match data {
        Some(d) => (
            d.rating.map_or(0, |r| r.value()),
            d.color_label.unwrap_or_default(),
        ),
        None => (0, ColorLabel::None),
    }
}

fn embedded_xmp(path: &Path) -> Option<crate::xmp::SidecarData> {
    let mut bytes = Vec::new();
    File::open(path).ok()?.take(XMP_SEARCH_BYTES).read_to_end(&mut bytes).ok()?;
    packet(&bytes).and_then(crate::xmp::parse_xmp_str)
}

/// The XMP packet in `bytes`, if there is one.
fn packet(bytes: &[u8]) -> Option<&str> {
    let find = |needle: &[u8], from: usize| {
        bytes[from..].windows(needle.len()).position(|w| w == needle).map(|at| at + from)
    };
    let start = find(b"<x:xmpmeta", 0)?;
    let end = find(b"</x:xmpmeta>", start)? + b"</x:xmpmeta>".len();
    std::str::from_utf8(&bytes[start..end]).ok()
}

/// Read-only, or locked (macOS's "immutable" flag).
fn is_protected(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        // UF_IMMUTABLE: Finder's "Locked".
        if meta.st_flags() & 0x2 != 0 {
            return true;
        }
    }
    meta.permissions().readonly()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_rating_and_label_in_an_embedded_packet() {
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="4" xmp:Label="Red"/></rdf:RDF></x:xmpmeta>"#;
        let mut file = vec![0xff, 0xd8, 1, 2, 3];
        file.extend(xmp.as_bytes());
        file.extend([9, 9, 9]);
        let data = crate::xmp::parse_xmp_str(packet(&file).unwrap()).unwrap();
        assert_eq!(data.rating.map(|r| r.value()), Some(4));
        assert_eq!(data.color_label, Some(ColorLabel::Red));
        assert!(packet(b"no packet here").is_none());
    }

    #[test]
    fn candidates_have_their_kind_and_protection() {
        let dir = std::env::temp_dir().join(format!("rustyedit-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jpg"), b"x").unwrap();
        std::fs::write(dir.join("b.dng"), b"x").unwrap();
        let locked = dir.join("c.png");
        std::fs::write(&locked, b"x").unwrap();
        let mut perms = std::fs::metadata(&locked).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&locked, perms).unwrap();

        let found = read_import_candidates(&dir);
        let kinds: Vec<_> = found.iter().map(|c| (c.kind, c.protected)).collect();
        // Make the file writable again so the folder can be removed.
        let mut perms = std::fs::metadata(&locked).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&locked, perms).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            kinds,
            vec![(FileKind::Jpeg, false), (FileKind::Dng, false), (FileKind::Png, true)]
        );
    }
}
