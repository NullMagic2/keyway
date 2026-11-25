//! Module: keyway_core::language
//! Purpose: The languages Keyway's own texts come in, and the tags that name them.
//! Created: 2026-11-25
//! Architecture: The configuration stores a tag. The settings window and the desktop extensions
//! hold the texts, and the extensions know the same tags and names.

use serde::{Deserialize, Serialize};

/// One language of Keyway's texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Language {
    English,
    Spanish,
    BrazilianPortuguese,
    Greek,
    EuropeanPortuguese,
}

impl Language {
    /// In the order the taskbar menu offers them.
    pub const ALL: [Self; 5] = [
        Self::English,
        Self::Spanish,
        Self::BrazilianPortuguese,
        Self::Greek,
        Self::EuropeanPortuguese,
    ];

    /// The tag the configuration and the desktop extensions know this language by.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
            Self::BrazilianPortuguese => "pt-BR",
            Self::Greek => "el",
            Self::EuropeanPortuguese => "pt-PT",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|language| language.tag() == tag)
    }

    /// Reads a locale name such as pt_BR.UTF-8. Plain pt means Brazilian Portuguese, as that
    /// is how the other Keyway-related projects treat it.
    pub fn from_locale(locale: &str) -> Option<Self> {
        let name = locale.split(['.', '@']).next().unwrap_or_default();
        let (language, region) = name.split_once(['_', '-']).unwrap_or((name, ""));

        match (language, region) {
            ("en", _) => Some(Self::English),
            ("es", _) => Some(Self::Spanish),
            ("pt", "PT") => Some(Self::EuropeanPortuguese),
            ("pt", _) => Some(Self::BrazilianPortuguese),
            ("el", _) => Some(Self::Greek),
            _ => None,
        }
    }

    /// The locales whose translations of other software suit this language, best first.
    /// English has none, because software is written in it.
    pub const fn locales(self) -> &'static [&'static str] {
        match self {
            Self::English => &[],
            Self::Spanish => &["es"],
            Self::BrazilianPortuguese => &["pt_BR", "pt"],
            Self::Greek => &["el"],
            Self::EuropeanPortuguese => &["pt_PT", "pt"],
        }
    }
}

impl TryFrom<String> for Language {
    type Error = String;

    fn try_from(tag: String) -> Result<Self, Self::Error> {
        Self::from_tag(&tag).ok_or_else(|| format!("unknown language: {tag}"))
    }
}

impl From<Language> for String {
    fn from(language: Language) -> Self {
        language.tag().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_is_found_by_its_tag() {
        for language in Language::ALL {
            assert_eq!(Language::from_tag(language.tag()), Some(language));
        }

        assert_eq!(Language::from_tag("pt"), None);
        assert_eq!(Language::from_tag(""), None);
    }

    #[test]
    fn locale_names_choose_the_right_language() {
        let brazilian = Some(Language::BrazilianPortuguese);
        let european = Some(Language::EuropeanPortuguese);

        assert_eq!(Language::from_locale("pt_BR.UTF-8"), brazilian);
        assert_eq!(Language::from_locale("pt"), brazilian);
        assert_eq!(Language::from_locale("pt_PT"), european);
        assert_eq!(Language::from_locale("pt_PT@euro"), european);
        assert_eq!(Language::from_locale("el_GR.UTF-8"), Some(Language::Greek));
        assert_eq!(Language::from_locale("es_AR"), Some(Language::Spanish));
        assert_eq!(Language::from_locale("en_US"), Some(Language::English));
        assert_eq!(Language::from_locale("ja_JP"), None);
        assert_eq!(Language::from_locale(""), None);
    }
}
