//! A zip file with nothing compressed, which is all an EPUB needs (its
//! pictures are JPEGs already, and its first entry must be stored plain).

use std::io::Write;

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// Builds a zip in memory, entry by entry.
#[derive(Default)]
pub struct ZipStore {
    out: Vec<u8>,
    entries: Vec<Entry>,
}

/// 1 January 1980, midnight, in the zip date and time fields.
const DOS_DATE: u16 = (1 << 5) | 1;
const DOS_TIME: u16 = 0;

impl ZipStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file.
    pub fn add(&mut self, name: &str, data: &[u8]) {
        let crc = crc32fast::hash(data);
        let offset = self.out.len() as u32;
        let o = &mut self.out;
        o.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        o.extend_from_slice(&20u16.to_le_bytes()); // version needed
        o.extend_from_slice(&0x0800u16.to_le_bytes()); // names are UTF-8
        o.extend_from_slice(&0u16.to_le_bytes()); // stored
        o.extend_from_slice(&DOS_TIME.to_le_bytes());
        o.extend_from_slice(&DOS_DATE.to_le_bytes());
        o.extend_from_slice(&crc.to_le_bytes());
        o.extend_from_slice(&(data.len() as u32).to_le_bytes());
        o.extend_from_slice(&(data.len() as u32).to_le_bytes());
        o.extend_from_slice(&(name.len() as u16).to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes()); // no extra field
        o.extend_from_slice(name.as_bytes());
        o.extend_from_slice(data);
        self.entries.push(Entry {
            name: name.to_string(),
            crc,
            size: data.len() as u32,
            offset,
        });
    }

    /// The finished file.
    pub fn finish(mut self) -> Vec<u8> {
        let directory_at = self.out.len() as u32;
        let mut directory = Vec::new();
        for e in &self.entries {
            directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            directory.extend_from_slice(&20u16.to_le_bytes()); // made by
            directory.extend_from_slice(&20u16.to_le_bytes()); // needed
            directory.extend_from_slice(&0x0800u16.to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes());
            directory.extend_from_slice(&DOS_TIME.to_le_bytes());
            directory.extend_from_slice(&DOS_DATE.to_le_bytes());
            directory.extend_from_slice(&e.crc.to_le_bytes());
            directory.extend_from_slice(&e.size.to_le_bytes());
            directory.extend_from_slice(&e.size.to_le_bytes());
            directory.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            directory.extend_from_slice(&0u16.to_le_bytes()); // extra
            directory.extend_from_slice(&0u16.to_le_bytes()); // comment
            directory.extend_from_slice(&0u16.to_le_bytes()); // disk
            directory.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
            directory.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            directory.extend_from_slice(&e.offset.to_le_bytes());
            directory.extend_from_slice(e.name.as_bytes());
        }
        let _ = self.out.write_all(&directory);
        let o = &mut self.out;
        o.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        o.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        o.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        o.extend_from_slice(&(directory.len() as u32).to_le_bytes());
        o.extend_from_slice(&directory_at.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}

/// What a zip holds, read back: (name, bytes) for each stored entry. Used
/// to check what was written.
#[cfg(test)]
pub fn read_back(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    let u16_at = |at: usize| u16::from_le_bytes([zip[at], zip[at + 1]]) as usize;
    let u32_at = |at: usize| u32::from_le_bytes([zip[at], zip[at + 1], zip[at + 2], zip[at + 3]]) as usize;
    // The end record is the last 22 bytes.
    let end = zip.len() - 22;
    assert_eq!(u32_at(end), 0x0605_4b50, "end of central directory");
    let count = u16_at(end + 10);
    let mut at = u32_at(end + 16);
    let mut out = Vec::new();
    for _ in 0..count {
        assert_eq!(u32_at(at), 0x0201_4b50);
        let (crc, size) = (u32_at(at + 16) as u32, u32_at(at + 24));
        let (name_len, extra, comment) = (u16_at(at + 28), u16_at(at + 30), u16_at(at + 32));
        let local = u32_at(at + 42);
        let name = String::from_utf8(zip[at + 46..at + 46 + name_len].to_vec()).unwrap();
        // Local header: 30 bytes, then its name and extra field.
        let (ln, le) = (u16_at(local + 26), u16_at(local + 28));
        let start = local + 30 + ln + le;
        let data = zip[start..start + size].to_vec();
        assert_eq!(crc32fast::hash(&data), crc, "{name}");
        out.push((name, data));
        at += 46 + name_len + extra + comment;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_goes_in_comes_out_with_names_and_checksums_intact() {
        let mut zip = ZipStore::new();
        zip.add("mimetype", b"application/epub+zip");
        zip.add("OEBPS/p001.xhtml", "<p>caf\u{e9}</p>".as_bytes());
        zip.add("empty", b"");
        let bytes = zip.finish();
        // The first entry is plain at the very start, as EPUB needs.
        assert_eq!(&bytes[30..38], b"mimetype");
        assert_eq!(&bytes[38..58], b"application/epub+zip");
        let files = read_back(&bytes);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], ("mimetype".to_string(), b"application/epub+zip".to_vec()));
        assert_eq!(files[1].1, "<p>caf\u{e9}</p>".as_bytes());
        assert!(files[2].1.is_empty());
    }
}
