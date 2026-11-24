//! Module: keyway_core::layout_labels
//! Purpose: The taskbar's code and flag for each XKB layout, such as PT-BR with Brazil's flag.
//! Created: 2026-11-09
//! Architecture: Embeds data/layout_labels.json from tools/generate_layout_labels.py and the
//! flags in data/flags/. keyway setup installs both, where the extensions read them.

use serde::Deserialize;
use std::collections::BTreeMap;

include!(concat!(env!("OUT_DIR"), "/flag_files.rs"));

/// The table as installed for the extensions, which read it from keyway/ in a data directory.
pub const LABELS_FILE: &str = include_str!("../../../data/layout_labels.json");

/// The flags' license, installed beside them.
pub const FLAGS_LICENSE: &str = include_str!("../../../data/flags/LICENSE");

/// What the taskbar shows for one layout: a language-country code such as PT-BR, which a
/// layout's variants share, and the flag. A layout used in several countries shows a flag its
/// speakers share, such as Spain's for Latin American Spanish. Braille and the like have none.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutLabel {
    pub code: String,
    pub flag: Option<String>,
}

impl LayoutLabel {
    /// The flag as an SVG image.
    pub fn flag_svg(&self) -> Option<&'static [u8]> {
        let flag = self.flag.as_deref()?;

        FLAG_FILES
            .iter()
            .find(|(name, _svg)| *name == flag)
            .map(|(_name, svg)| *svg)
    }
}

/// The file's other fields name its generator and the flags' license, for people reading it.
#[derive(Clone, Debug, Deserialize)]
pub struct LayoutLabels {
    layouts: BTreeMap<String, LayoutLabel>,
}

impl LayoutLabels {
    pub fn load() -> Result<Self, serde_json::Error> {
        serde_json::from_str(LABELS_FILE)
    }

    /// The label of an input source id: an XKB layout or layout+variant. None for an input
    /// method engine, or a layout newer than the table, which then keep the desktop's own
    /// short name.
    pub fn label_for(&self, source_id: &str) -> Option<&LayoutLabel> {
        self.layouts.get(source_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(id: &str) -> Option<LayoutLabel> {
        LayoutLabels::load().unwrap().label_for(id).cloned()
    }

    #[test]
    fn layouts_show_language_and_country_and_variants_show_their_layouts_code() {
        let brazil = label("br").unwrap();
        let greek = label("gr+simple").unwrap();

        assert_eq!(brazil.code, "PT-BR");
        assert_eq!(brazil.flag.as_deref(), Some("br"));
        assert_eq!(greek.code, "EL-GR");
        assert_eq!(greek.flag.as_deref(), Some("gr"));
    }

    #[test]
    fn a_layout_of_several_countries_has_a_shared_flag_and_no_country() {
        let latin_american = label("latam").unwrap();
        let arabic = label("ara+azerty").unwrap();

        assert_eq!(latin_american.code, "ES");
        assert_eq!(latin_american.flag.as_deref(), Some("es"));
        assert_eq!(arabic.code, "AR");
        assert_eq!(arabic.flag.as_deref(), Some("arab"));
    }

    #[test]
    fn a_layout_of_no_country_has_no_flag() {
        assert_eq!(label("brai").unwrap().flag_svg(), None);
    }

    #[test]
    fn every_flag_named_in_the_table_is_embedded() {
        let labels = LayoutLabels::load().unwrap();

        for (id, label) in &labels.layouts {
            assert_eq!(label.flag.is_some(), label.flag_svg().is_some(), "{id}");
        }
    }

    #[test]
    fn unknown_layouts_have_no_label() {
        assert_eq!(label("anthy"), None);
    }
}
