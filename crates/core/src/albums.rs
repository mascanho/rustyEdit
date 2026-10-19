use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A named, hand-picked set of photos. Photos are referenced by path, so an
/// album can gather shots from any number of folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    pub id: u64,
    pub name: String,
    pub photos: Vec<PathBuf>,
}

impl Album {
    pub fn contains(&self, photo: &Path) -> bool {
        self.photos.iter().any(|p| p == photo)
    }
}

/// Every album the user has made. Lives app-wide (not per folder).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Albums {
    pub items: Vec<Album>,
    next_id: u64,
}

impl Albums {
    /// Creates an album named `name` (trimmed; blank names are refused) and
    /// returns its id.
    pub fn create(&mut self, name: &str) -> Option<u64> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Album {
            id,
            name: name.to_string(),
            photos: Vec::new(),
        });
        Some(id)
    }

    pub fn get(&self, id: u64) -> Option<&Album> {
        self.items.iter().find(|a| a.id == id)
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|a| a.id != id);
    }

    /// Adds `photo` to the album, or removes it if it's already there.
    /// Returns whether the photo is now in the album.
    pub fn toggle(&mut self, id: u64, photo: &Path) -> bool {
        let Some(album) = self.items.iter_mut().find(|a| a.id == id) else {
            return false;
        };
        match album.photos.iter().position(|p| p == photo) {
            Some(ix) => {
                album.photos.remove(ix);
                false
            }
            None => {
                album.photos.push(photo.to_path_buf());
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_toggle_delete() {
        let mut albums = Albums::default();
        assert_eq!(albums.create("  "), None);
        let id = albums.create(" Hawaii ").unwrap();
        assert_eq!(albums.get(id).unwrap().name, "Hawaii");

        let photo = Path::new("/a/b.jpg");
        assert!(albums.toggle(id, photo));
        assert!(albums.get(id).unwrap().contains(photo));
        assert!(!albums.toggle(id, photo));
        assert!(albums.get(id).unwrap().photos.is_empty());

        albums.delete(id);
        assert!(albums.items.is_empty());
        // Ids are never reused.
        assert_ne!(albums.create("Again"), Some(id));
    }
}
