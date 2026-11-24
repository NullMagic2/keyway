//! Module: keyway::agent
//! Purpose: Decide what each router report and desktop change means, with no I/O of its own.
//! Created: 2026-11-08
//! Architecture: run.rs performs the actions returned here; the desktop stays the only
//! authority on which layouts exist and which is active.

use keyway_core::alt_code::AltCodeEntry;
use keyway_core::code_pages::{CodePageCatalog, LayoutCodePages};
use keyway_core::config::{Config, SwitchOrder};
use keyway_core::hotkey::ShortcutAction;
use keyway_core::layout_id::LayoutId;
use keyway_core::protocol::{AgentMessage, PROTOCOL_VERSION, RouterMessage};
use keyway_desktop::desktop_shell::{DesktopSnapshot, InputSource};

/// One thing the agent must do, in the order returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentAction {
    Activate(LayoutId),
    CommitText(String),
    ShowFlags(bool),
    Send(AgentMessage),
    DropRouter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Next,
    Previous,
}

/// Returns the nearest source after or before the active one that takes part in switching,
/// wrapping around the list. None means there is nothing to switch to.
pub fn neighbour_in_list(
    sources: &[InputSource],
    active: &LayoutId,
    forward: bool,
    takes_part: impl Fn(&LayoutId) -> bool,
) -> Option<LayoutId> {
    let position = sources.iter().position(|source| source.id == *active)?;
    let count = sources.len();
    let step = if forward { 1 } else { count - 1 };

    (1..count)
        .map(|distance| &sources[(position + distance * step) % count].id)
        .find(|id| takes_part(id))
        .cloned()
}

#[derive(Clone, Debug)]
pub struct Agent {
    config: Config,
    catalog: CodePageCatalog,
    layout_pages: LayoutCodePages,
    desktop: Option<DesktopSnapshot>,
    recent: Vec<LayoutId>,
    router_ready: bool,
}

impl Agent {
    pub fn new(config: Config, catalog: CodePageCatalog, layout_pages: LayoutCodePages) -> Self {
        Self {
            config,
            catalog,
            layout_pages,
            desktop: None,
            recent: Vec::new(),
            router_ready: false,
        }
    }

    pub const fn desktop_available(&self) -> bool {
        self.desktop.is_some()
    }

    pub fn config_changed(&mut self, config: Config) -> Vec<AgentAction> {
        let flags_changed = config.indicator.flags != self.config.indicator.flags;
        let mut actions = Vec::new();

        self.config = config;

        if flags_changed && self.desktop_available() {
            actions.push(AgentAction::ShowFlags(self.config.indicator.flags));
        }

        if self.router_ready {
            actions.push(self.configure_message());
        }

        actions
    }

    /// Records a new desktop state. A newly started extension learns the flag choice, and the
    /// router learns if Right Alt's role changed.
    pub fn desktop_changed(&mut self, snapshot: DesktopSnapshot) -> Vec<AgentAction> {
        let mut actions = Vec::new();

        if !self.desktop_available() {
            actions.push(AgentAction::ShowFlags(self.config.indicator.flags));
        }

        let altgr_changed = self
            .desktop
            .as_ref()
            .is_some_and(|old| old.right_alt_is_altgr != snapshot.right_alt_is_altgr);

        self.recent.retain(|id| {
            *id != snapshot.active && snapshot.sources.iter().any(|source| source.id == *id)
        });
        self.recent.insert(0, snapshot.active.clone());

        let allow_right_alt = !snapshot.right_alt_is_altgr;
        self.desktop = Some(snapshot);

        if altgr_changed && self.router_ready {
            actions.push(AgentAction::Send(AgentMessage::SetAllowRightAlt(
                allow_right_alt,
            )));
        }

        actions
    }

    pub fn desktop_lost(&mut self) {
        self.desktop = None;
    }

    pub fn router_connected(&mut self) -> Vec<AgentAction> {
        vec![AgentAction::Send(AgentMessage::Hello {
            version: PROTOCOL_VERSION,
        })]
    }

    pub fn router_disconnected(&mut self) {
        self.router_ready = false;
    }

    pub fn router_message(&mut self, message: RouterMessage) -> Vec<AgentAction> {
        match message {
            RouterMessage::Hello { version } if version == PROTOCOL_VERSION => {
                self.router_ready = true;
                vec![self.configure_message()]
            }
            RouterMessage::Hello { .. } => vec![AgentAction::DropRouter],
            RouterMessage::Shortcut(action) => self.shortcut(&action),
            RouterMessage::AltCode(entry) => self.alt_code(&entry),
        }
    }

    fn configure_message(&self) -> AgentAction {
        let allow_right_alt = self
            .desktop
            .as_ref()
            .is_some_and(|desktop| !desktop.right_alt_is_altgr);

        AgentAction::Send(AgentMessage::Configure(
            self.config.router_config(allow_right_alt),
        ))
    }

    fn shortcut(&self, action: &ShortcutAction) -> Vec<AgentAction> {
        let target = match action {
            ShortcutAction::NextLayout => self.switch_target(Direction::Next),
            ShortcutAction::PreviousLayout => self.switch_target(Direction::Previous),
            ShortcutAction::SelectLayout(id) => self.existing_source(id),
        };

        target.map(AgentAction::Activate).into_iter().collect()
    }

    /// Leaves out every layout with its own shortcut, though it may be the one to leave.
    fn switch_target(&self, direction: Direction) -> Option<LayoutId> {
        let desktop = self.desktop.as_ref()?;
        let switching = &self.config.switching;
        let takes_part = |id: &LayoutId| switching.cycles_through(id);

        // The active layout is the first of the recent ones. Layouts not used since the login
        // come after them, in the list's order, or the shortcut could never reach them.
        let unused = desktop
            .sources
            .iter()
            .map(|source| &source.id)
            .filter(|id| !self.recent.contains(id));
        let mut earlier = self
            .recent
            .iter()
            .skip(1)
            .chain(unused)
            .filter(|id| takes_part(id));

        match switching.order {
            SwitchOrder::List => neighbour_in_list(
                &desktop.sources,
                &desktop.active,
                direction == Direction::Next,
                takes_part,
            ),
            SwitchOrder::Recent if direction == Direction::Next => earlier.next().cloned(),
            SwitchOrder::Recent => earlier.next_back().cloned(),
        }
    }

    /// A direct shortcut may name a layout the user has since removed.
    fn existing_source(&self, id: &LayoutId) -> Option<LayoutId> {
        let desktop = self.desktop.as_ref()?;

        desktop
            .sources
            .iter()
            .any(|source| source.id == *id)
            .then(|| id.clone())
    }

    fn alt_code(&self, entry: &AltCodeEntry) -> Vec<AgentAction> {
        let Some(desktop) = &self.desktop else {
            return Vec::new();
        };

        let layout = desktop
            .sources
            .iter()
            .find(|source| source.id == desktop.active)
            .map_or(&desktop.active, |source| &source.xkb_id);
        let pages = self
            .layout_pages
            .pages_for(layout, &self.config.alt_codes.code_pages);
        let character = entry.decode(
            pages,
            self.config.alt_codes.unicode_above_255,
            &self.catalog,
        );

        character
            .map(|decoded| AgentAction::CommitText(decoded.to_string()))
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyway_core::alt_code::AltCodeRadix;

    fn id(text: &str) -> LayoutId {
        LayoutId::try_from(text.to_string()).unwrap()
    }

    fn source(text: &str) -> InputSource {
        InputSource {
            id: id(text),
            xkb_id: id(text),
            display_name: text.to_string(),
            short_name: text.to_string(),
        }
    }

    fn snapshot(sources: &[&str], active: &str, right_alt_is_altgr: bool) -> DesktopSnapshot {
        DesktopSnapshot {
            sources: sources.iter().map(|text| source(text)).collect(),
            active: id(active),
            right_alt_is_altgr,
        }
    }

    fn agent_with(config_text: &str) -> Agent {
        Agent::new(
            Config::parse(config_text).unwrap(),
            CodePageCatalog::load().unwrap(),
            LayoutCodePages::load().unwrap(),
        )
    }

    fn switching_agent(config_text: &str, active: &str) -> Agent {
        let mut agent = agent_with(config_text);
        agent.desktop_changed(snapshot(&["br", "gr+simple", "us"], active, true));

        agent
    }

    fn shortcut(agent: &mut Agent, action: ShortcutAction) -> Vec<AgentAction> {
        agent.router_message(RouterMessage::Shortcut(action))
    }

    fn typed(agent: &mut Agent, digits: &str) -> Vec<AgentAction> {
        let entry = AltCodeEntry::new(AltCodeRadix::Decimal, digits.to_string()).unwrap();

        agent.router_message(RouterMessage::AltCode(entry))
    }

    fn commit(text: &str) -> Vec<AgentAction> {
        vec![AgentAction::CommitText(text.to_string())]
    }

    fn activate(text: &str) -> Vec<AgentAction> {
        vec![AgentAction::Activate(id(text))]
    }

    #[test]
    fn switching_follows_the_list_and_wraps() {
        let mut first = switching_agent("", "br");
        let mut last = switching_agent("", "us");

        assert_eq!(
            shortcut(&mut first, ShortcutAction::NextLayout),
            activate("gr+simple")
        );
        assert_eq!(
            shortcut(&mut first, ShortcutAction::PreviousLayout),
            activate("us")
        );
        assert_eq!(
            shortcut(&mut last, ShortcutAction::NextLayout),
            activate("br")
        );
    }

    #[test]
    fn switching_passes_over_a_layout_with_its_own_shortcut() {
        let config = "[switching.direct]\n\"gr+simple\" = \"Ctrl+Shift+2\"";
        let mut first = switching_agent(config, "br");
        let mut last = switching_agent(config, "us");
        let mut own = switching_agent(config, "gr+simple");

        assert_eq!(
            shortcut(&mut first, ShortcutAction::NextLayout),
            activate("us")
        );
        assert_eq!(
            shortcut(&mut last, ShortcutAction::PreviousLayout),
            activate("br")
        );
        assert_eq!(
            shortcut(&mut first, ShortcutAction::SelectLayout(id("gr+simple"))),
            activate("gr+simple")
        );
        assert_eq!(
            shortcut(&mut own, ShortcutAction::NextLayout),
            activate("us")
        );
        assert_eq!(
            shortcut(&mut own, ShortcutAction::PreviousLayout),
            activate("br")
        );
    }

    #[test]
    fn switching_needs_another_layout_without_its_own_shortcut() {
        let one_left =
            "[switching.direct]\nbr = \"Ctrl+Shift+1\"\n\"gr+simple\" = \"Ctrl+Shift+2\"";
        let none_left = format!("{one_left}\nus = \"Ctrl+Shift+3\"");
        let mut on_the_one_left = switching_agent(one_left, "us");
        let mut away_from_it = switching_agent(one_left, "br");
        let mut with_none_left = switching_agent(&none_left, "br");

        assert_eq!(
            shortcut(&mut on_the_one_left, ShortcutAction::NextLayout),
            []
        );
        assert_eq!(
            shortcut(&mut on_the_one_left, ShortcutAction::PreviousLayout),
            []
        );
        assert_eq!(
            shortcut(&mut away_from_it, ShortcutAction::NextLayout),
            activate("us")
        );
        assert_eq!(
            shortcut(&mut with_none_left, ShortcutAction::NextLayout),
            []
        );
    }

    #[test]
    fn one_layout_has_nothing_to_switch_to() {
        let mut agent = agent_with("");
        agent.desktop_changed(snapshot(&["br"], "br", true));

        assert_eq!(shortcut(&mut agent, ShortcutAction::NextLayout), []);
    }

    #[test]
    fn recent_order_returns_to_the_last_layout() {
        let config = "[switching]\norder = \"recent\"";
        let sources = ["br", "gr+simple", "us"];
        let mut agent = agent_with(config);
        agent.desktop_changed(snapshot(&sources, "br", true));
        agent.desktop_changed(snapshot(&sources, "us", true));

        assert_eq!(
            shortcut(&mut agent, ShortcutAction::NextLayout),
            activate("br")
        );

        agent.desktop_changed(snapshot(&sources, "br", true));

        assert_eq!(
            shortcut(&mut agent, ShortcutAction::NextLayout),
            activate("us")
        );
    }

    #[test]
    fn recent_order_reaches_layouts_not_used_yet() {
        let config = "[switching]\norder = \"recent\"";
        let sources = ["br", "gr+simple", "us"];
        let mut agent = switching_agent(config, "br");

        assert_eq!(
            shortcut(&mut agent, ShortcutAction::NextLayout),
            activate("gr+simple")
        );
        assert_eq!(
            shortcut(&mut agent, ShortcutAction::PreviousLayout),
            activate("us")
        );

        agent.desktop_changed(snapshot(&sources, "gr+simple", true));

        assert_eq!(
            shortcut(&mut agent, ShortcutAction::NextLayout),
            activate("br")
        );
        assert_eq!(
            shortcut(&mut agent, ShortcutAction::PreviousLayout),
            activate("us")
        );
    }

    #[test]
    fn recent_order_passes_over_a_layout_with_its_own_shortcut() {
        let config = "[switching]\norder = \"recent\"\n[switching.direct]\nus = \"Ctrl+Shift+3\"";
        let sources = ["br", "gr+simple", "us"];
        let mut agent = agent_with(config);
        agent.desktop_changed(snapshot(&sources, "br", true));
        agent.desktop_changed(snapshot(&sources, "gr+simple", true));
        agent.desktop_changed(snapshot(&sources, "us", true));

        assert_eq!(
            shortcut(&mut agent, ShortcutAction::NextLayout),
            activate("gr+simple")
        );
        assert_eq!(
            shortcut(&mut agent, ShortcutAction::PreviousLayout),
            activate("br")
        );

        agent.desktop_changed(snapshot(&sources, "gr+simple", true));

        assert_eq!(
            shortcut(&mut agent, ShortcutAction::NextLayout),
            activate("br")
        );
        assert_eq!(
            shortcut(&mut agent, ShortcutAction::PreviousLayout),
            activate("br")
        );
    }

    #[test]
    fn recent_order_forgets_removed_layouts() {
        let config = "[switching]\norder = \"recent\"";
        let mut agent = agent_with(config);
        agent.desktop_changed(snapshot(&["br", "us"], "us", true));
        agent.desktop_changed(snapshot(&["br", "us"], "br", true));
        agent.desktop_changed(snapshot(&["br"], "br", true));

        assert_eq!(shortcut(&mut agent, ShortcutAction::NextLayout), []);
        assert_eq!(shortcut(&mut agent, ShortcutAction::PreviousLayout), []);
    }

    #[test]
    fn a_direct_shortcut_needs_the_layout_to_exist() {
        let mut agent = switching_agent("", "br");

        assert_eq!(
            shortcut(&mut agent, ShortcutAction::SelectLayout(id("us"))),
            activate("us")
        );
        assert_eq!(
            shortcut(&mut agent, ShortcutAction::SelectLayout(id("fr"))),
            []
        );
    }

    #[test]
    fn alt_codes_use_the_active_layouts_code_pages() {
        let mut brazilian = switching_agent("", "br");
        let mut greek = switching_agent("", "gr+simple");

        assert_eq!(typed(&mut brazilian, "130"), commit("é"));
        assert_eq!(typed(&mut brazilian, "0128"), commit("€"));
        assert_eq!(typed(&mut greek, "152"), commit("α"));
        assert_eq!(typed(&mut greek, "0225"), commit("α"));
    }

    #[test]
    fn input_method_engines_use_their_underlying_layout() {
        let mut agent = agent_with("");
        let mut engine = source("hangul");
        engine.xkb_id = id("gr");
        agent.desktop_changed(DesktopSnapshot {
            sources: vec![engine],
            active: id("hangul"),
            right_alt_is_altgr: true,
        });

        assert_eq!(typed(&mut agent, "152"), commit("α"));
    }

    #[test]
    fn configured_code_pages_win() {
        let config = "[alt_codes.code_pages]\nbr = { oem = 737, ansi = 1252 }";
        let mut agent = switching_agent(config, "br");

        assert_eq!(typed(&mut agent, "152"), commit("α"));
    }

    #[test]
    fn nothing_is_typed_for_empty_codes_or_without_a_desktop() {
        let mut agent = switching_agent("", "br");

        assert_eq!(typed(&mut agent, "0129"), []);

        agent.desktop_lost();

        assert!(!agent.desktop_available());
        assert_eq!(typed(&mut agent, "130"), []);
        assert_eq!(shortcut(&mut agent, ShortcutAction::NextLayout), []);
    }

    #[test]
    fn the_router_is_configured_after_its_hello() {
        let mut agent = switching_agent("", "us");
        agent.desktop_changed(snapshot(&["us"], "us", false));

        assert_eq!(
            agent.router_connected(),
            [AgentAction::Send(AgentMessage::Hello {
                version: PROTOCOL_VERSION
            })]
        );

        let reply = agent.router_message(RouterMessage::Hello {
            version: PROTOCOL_VERSION,
        });
        let [AgentAction::Send(AgentMessage::Configure(config))] = reply.as_slice() else {
            panic!("expected a configuration, got {reply:?}");
        };

        assert!(config.alt_codes.allow_right_alt);
    }

    #[test]
    fn a_router_with_another_protocol_is_dropped() {
        let mut agent = agent_with("");
        let wrong_version = PROTOCOL_VERSION + 1;

        assert_eq!(
            agent.router_message(RouterMessage::Hello {
                version: wrong_version
            }),
            [AgentAction::DropRouter]
        );
    }

    #[test]
    fn right_alt_changes_reach_a_ready_router_only() {
        let mut agent = switching_agent("", "br");
        let unready = agent.desktop_changed(snapshot(&["br", "us"], "us", false));

        assert_eq!(unready, []);

        agent.router_message(RouterMessage::Hello {
            version: PROTOCOL_VERSION,
        });

        let ready = agent.desktop_changed(snapshot(&["br", "us"], "br", true));
        let unchanged = agent.desktop_changed(snapshot(&["br", "us"], "br", true));

        assert_eq!(
            ready,
            [AgentAction::Send(AgentMessage::SetAllowRightAlt(false))]
        );
        assert_eq!(unchanged, []);
    }

    #[test]
    fn a_new_config_is_sent_only_to_a_ready_router() {
        let mut agent = switching_agent("", "br");
        let config = Config::parse("[switching]\nnext = \"Ctrl+Shift\"").unwrap();

        assert_eq!(agent.config_changed(config.clone()), []);

        agent.router_message(RouterMessage::Hello {
            version: PROTOCOL_VERSION,
        });
        let sent = agent.config_changed(config);

        assert!(matches!(
            sent.as_slice(),
            [AgentAction::Send(AgentMessage::Configure(_))]
        ));

        agent.router_disconnected();

        let after_loss = agent.config_changed(Config::default());

        assert_eq!(after_loss, []);
    }

    #[test]
    fn the_extension_learns_the_flag_choice_when_it_appears_and_when_it_changes() {
        let mut agent = agent_with("[indicator]\nflags = false");
        let without_flags = Config::parse("[indicator]\nflags = false").unwrap();

        assert_eq!(
            agent.desktop_changed(snapshot(&["br"], "br", true)),
            [AgentAction::ShowFlags(false)]
        );
        assert_eq!(agent.desktop_changed(snapshot(&["br"], "br", true)), []);
        assert_eq!(
            agent.config_changed(Config::default()),
            [AgentAction::ShowFlags(true)]
        );

        agent.desktop_lost();

        assert_eq!(agent.config_changed(without_flags), []);
        assert_eq!(
            agent.desktop_changed(snapshot(&["br"], "br", true)),
            [AgentAction::ShowFlags(false)]
        );
    }
}
