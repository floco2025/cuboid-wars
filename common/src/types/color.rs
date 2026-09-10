use std::fmt;

use bincode::{Decode, Encode};
use serde::{Deserialize, Deserializer, de};

// An sRGB colour, written `#rrggbb` in JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HexColor(pub [u8; 3]);

impl HexColor {
    pub fn parse(text: &str) -> Result<Self, String> {
        let digits = text
            .strip_prefix('#')
            .filter(|digits| digits.len() == 6 && digits.is_ascii());
        let byte = |from: usize| digits.and_then(|digits| u8::from_str_radix(&digits[from..from + 2], 16).ok());
        match (byte(0), byte(2), byte(4)) {
            (Some(r), Some(g), Some(b)) => Ok(Self([r, g, b])),
            _ => Err(format!("expected a color like #rrggbb, got {text:?}")),
        }
    }
}

impl fmt::Display for HexColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[cfg(test)]
#[path = "tests/color.rs"]
mod tests;
