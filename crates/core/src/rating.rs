use serde::{Deserialize, Serialize};

/// A star rating from 0 (no rating) to 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct Rating(u8);

impl Rating {
    pub fn new(value: u8) -> Self {
        Self(value.min(5))
    }

    pub fn value(&self) -> u8 {
        self.0
    }
}

impl From<u8> for Rating {
    fn from(value: u8) -> Self {
        Rating::new(value)
    }
}
