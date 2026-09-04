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
mod tests {
    use super::*;

    #[test]
    fn hex_color_parses_either_case_and_prints_lowercase() {
        assert_eq!(
            HexColor::parse("#ff0000").expect("lowercase rejected"),
            HexColor([255, 0, 0])
        );
        let mixed = HexColor::parse("#0080FF").expect("mixed case rejected");
        assert_eq!(mixed, HexColor([0, 128, 255]));
        assert_eq!(mixed.to_string(), "#0080ff");
    }

    #[test]
    fn hex_color_rejects_anything_but_hash_and_six_digits() {
        for bad in ["ff0000", "#fff", "", "#gggggg", "#aébbb", "#ff00000"] {
            assert!(HexColor::parse(bad).is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn hex_color_deserializes_from_json_with_a_readable_error() {
        assert_eq!(
            serde_json::from_str::<HexColor>("\"#22cc33\"").expect("valid color rejected"),
            HexColor([0x22, 0xcc, 0x33])
        );
        let error = serde_json::from_str::<HexColor>("\"green\"").expect_err("word parsed as a color");
        assert!(error.to_string().contains("#rrggbb"), "{error}");
    }
}
