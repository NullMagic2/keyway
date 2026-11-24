//! Module: keyway_settings::source_list
//! Purpose: The rules for editing the desktop's input source list.
//! Created: 2026-11-08
//! Architecture: Pure edits; input_sources.rs reads and writes the real list in GSettings.

const XKB_SOURCE_TYPE: &str = "xkb";
const SOURCE_VARIANT_SEPARATOR: char = '+';
const GROUP_VARIANT_SEPARATOR: char = '\t';
const QUERY_LIST_SEPARATOR: char = ',';

/// One input source as GSettings stores it, such as ("xkb", "gr+simple").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEntry {
    pub kind: String,
    pub id: String,
}

impl SourceEntry {
    pub fn xkb(id: &str) -> Self {
        Self {
            kind: XKB_SOURCE_TYPE.to_string(),
            id: id.to_string(),
        }
    }

    pub fn is_xkb(&self) -> bool {
        self.kind == XKB_SOURCE_TYPE
    }

    /// Reads a libgnomekbd layout, such as "gr\tsimple", which is how Cinnamon 6.4 and older
    /// store their list.
    pub fn from_group_layout(layout: &str) -> Self {
        Self::xkb(&layout.replace(GROUP_VARIANT_SEPARATOR, "+"))
    }

    /// The libgnomekbd form of this source. Those lists can only hold XKB layouts.
    pub fn to_group_layout(&self) -> String {
        self.id.replace(
            SOURCE_VARIANT_SEPARATOR,
            &GROUP_VARIANT_SEPARATOR.to_string(),
        )
    }
}

/// Reads the layouts from setxkbmap -query. Cinnamon 6.4 and older leave their list empty
/// until a user edits it, and the X server's layouts are the ones in use until then.
pub fn sources_from_xkb_query(output: &str) -> Vec<SourceEntry> {
    let field = |name: &str| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
            .map(str::trim)
    };

    let Some(layouts) = field("layout") else {
        return Vec::new();
    };

    let mut variants = field("variant")
        .unwrap_or_default()
        .split(QUERY_LIST_SEPARATOR);

    layouts
        .split(QUERY_LIST_SEPARATOR)
        .map(|layout| match variants.next() {
            Some(variant) if !variant.is_empty() => {
                SourceEntry::xkb(&format!("{layout}{SOURCE_VARIANT_SEPARATOR}{variant}"))
            }
            _ => SourceEntry::xkb(layout),
        })
        .collect()
}

/// Adds a source at the end. Returns false if it is already listed.
pub fn add_source(sources: &mut Vec<SourceEntry>, entry: SourceEntry) -> bool {
    if sources.contains(&entry) {
        return false;
    }

    sources.push(entry);

    true
}

/// Removes a source. Returns false for the last one, because a desktop with no input source
/// has no keyboard layout at all.
pub fn remove_source(sources: &mut Vec<SourceEntry>, index: usize) -> bool {
    if sources.len() <= 1 || index >= sources.len() {
        return false;
    }

    sources.remove(index);

    true
}

/// Moves a source one place. Returns false if it is already at that end of the list.
pub fn move_source(sources: &mut [SourceEntry], index: usize, up: bool) -> bool {
    let target = if up {
        index.checked_sub(1)
    } else {
        index.checked_add(1).filter(|next| *next < sources.len())
    };

    match target {
        Some(target) if index < sources.len() => {
            sources.swap(index, target);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_layouts_convert_both_ways() {
        let greek = SourceEntry::from_group_layout("gr\tsimple");

        assert_eq!(greek, SourceEntry::xkb("gr+simple"));
        assert_eq!(greek.to_group_layout(), "gr\tsimple");
        assert_eq!(SourceEntry::xkb("br").to_group_layout(), "br");
    }

    #[test]
    fn the_x_servers_layouts_pair_with_their_variants() {
        let output = "rules:      evdev\nmodel:      pc105\nlayout:     br,gr\nvariant:    ,simple\noptions:    grp_led:scroll\n";

        assert_eq!(
            sources_from_xkb_query(output),
            [SourceEntry::xkb("br"), SourceEntry::xkb("gr+simple")]
        );
    }

    #[test]
    fn a_query_without_variants_or_layouts_still_reads() {
        assert_eq!(
            sources_from_xkb_query("layout:     us\n"),
            [SourceEntry::xkb("us")]
        );
        assert_eq!(sources_from_xkb_query("rules:      evdev\n"), []);
    }

    fn sample() -> Vec<SourceEntry> {
        vec![
            SourceEntry::xkb("br"),
            SourceEntry::xkb("gr+simple"),
            SourceEntry {
                kind: "ibus".to_string(),
                id: "hangul".to_string(),
            },
        ]
    }

    fn ids(sources: &[SourceEntry]) -> Vec<&str> {
        sources.iter().map(|entry| entry.id.as_str()).collect()
    }

    #[test]
    fn adding_goes_to_the_end_and_refuses_duplicates() {
        let mut sources = sample();

        assert!(add_source(&mut sources, SourceEntry::xkb("us")));
        assert!(!add_source(&mut sources, SourceEntry::xkb("br")));
        assert_eq!(ids(&sources), ["br", "gr+simple", "hangul", "us"]);
    }

    #[test]
    fn removing_never_empties_the_list() {
        let mut sources = sample();

        assert!(remove_source(&mut sources, 1));
        assert!(remove_source(&mut sources, 1));
        assert!(!remove_source(&mut sources, 0));
        assert!(!remove_source(&mut sources, 5));
        assert_eq!(ids(&sources), ["br"]);
    }

    #[test]
    fn moving_stops_at_both_ends() {
        let mut sources = sample();

        assert!(!move_source(&mut sources, 0, true));
        assert!(!move_source(&mut sources, 2, false));
        assert!(!move_source(&mut sources, 9, true));
        assert!(move_source(&mut sources, 0, false));
        assert_eq!(ids(&sources), ["gr+simple", "br", "hangul"]);
        assert!(move_source(&mut sources, 2, true));
        assert_eq!(ids(&sources), ["gr+simple", "hangul", "br"]);
    }

    #[test]
    fn input_method_engines_are_kept_distinct_from_layouts() {
        let sources = sample();

        assert!(sources[0].is_xkb());
        assert!(!sources[2].is_xkb());
    }
}
