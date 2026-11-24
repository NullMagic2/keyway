//! Module: keyway_core::layout_id
//! Purpose: Validated identifier of a desktop input source, such as gr+simple.
//! Created: 2026-11-08
//! Architecture: Names layouts in configuration, shortcuts and code page lookups.

use serde::{Deserialize, Serialize};
use std::fmt;

const MAX_LAYOUT_ID_BYTES: usize = 128;
const VARIANT_SEPARATOR: char = '+';

/// A non-empty input source id with no whitespace or control characters.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LayoutId(String);

impl LayoutId {
    /// Returns the id without its variant, so gr+simple gives gr.
    pub fn base(&self) -> &str {
        match self.0.split_once(VARIANT_SEPARATOR) {
            Some((base, _variant)) => base,
            None => &self.0,
        }
    }

    /// Returns the variant, if the id has one, so gr+simple gives simple.
    pub fn variant(&self) -> Option<&str> {
        self.0
            .split_once(VARIANT_SEPARATOR)
            .map(|(_base, variant)| variant)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for LayoutId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let has_invalid_character = value.chars().any(|c| c.is_whitespace() || c.is_control());

        if value.is_empty() || value.len() > MAX_LAYOUT_ID_BYTES || has_invalid_character {
            return Err(format!("invalid layout id: {value:?}"));
        }

        Ok(Self(value))
    }
}

impl From<LayoutId> for String {
    fn from(id: LayoutId) -> Self {
        id.0
    }
}

impl fmt::Display for LayoutId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_drops_the_variant() {
        let with_variant = LayoutId::try_from("gr+simple".to_string()).unwrap();
        let without_variant = LayoutId::try_from("br".to_string()).unwrap();

        assert_eq!(with_variant.base(), "gr");
        assert_eq!(without_variant.base(), "br");
        assert_eq!(with_variant.variant(), Some("simple"));
        assert_eq!(without_variant.variant(), None);
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(LayoutId::try_from(String::new()).is_err());
        assert!(LayoutId::try_from("gr simple".to_string()).is_err());
    }
}
