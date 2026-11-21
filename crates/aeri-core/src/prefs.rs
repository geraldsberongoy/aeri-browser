//! App-wide preferences (everything of Search's Settings that is not the
//! window layout, `layout.rs`, or the look, `appearance`). One choice for
//! every window, kept in `app.db` as one JSON value under `prefs`. Missing
//! keys take their defaults and unknown ones are ignored, so older and newer
//! values keep reading; a group whose stored value cannot be read is its
//! defaults. The defaults are Search's (`Prefs.swift`; the inventory is
//! `docs` notes "search-settings" §2).
//!
//! The groups follow Search's Settings pages: [`General`], [`Tabs`],
//! [`Browsing`], [`Spaces`], [`Privacy`], [`Mouse`], [`Launcher`] and the
//! [`AppearanceExtras`] the layout does not hold.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::keymap::Chord;

/// The page zoom, in percent, where every site starts.
pub const MIN_ZOOM: u16 = 50;
pub const DEFAULT_ZOOM: u16 = 100;
pub const MAX_ZOOM: u16 = 300;
pub const ZOOM_STEP: u16 = 5;

/// Where the Downloads list opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadsOpen {
    #[default]
    Tab,
    Float,
    SidePanel,
}

/// The webmail `mailto:` links open in; `None` leaves them to the mail app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Webmail {
    #[default]
    None,
    Gmail,
    Outlook,
    Fastmail,
    Proton,
}

impl Webmail {
    /// The compose address for a `mailto:` link's `to` and the rest of its
    /// query (`subject=…&body=…`, may be empty). `None` for no webmail.
    pub fn compose_url(self, to: &str, query: &str) -> Option<String> {
        let encode = |text: &str| -> String {
            url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
        };
        let to = encode(to);
        let extra = if query.is_empty() {
            String::new()
        } else {
            format!("&{query}")
        };
        Some(match self {
            Self::None => return None,
            Self::Gmail => format!("https://mail.google.com/mail/?view=cm&fs=1&to={to}{extra}"),
            Self::Outlook => {
                format!("https://outlook.live.com/mail/0/deeplink/compose?to={to}{extra}")
            }
            Self::Fastmail => format!("https://app.fastmail.com/mail/compose?to={to}{extra}"),
            Self::Proton => format!("https://mail.proton.me/u/0/compose?to={to}{extra}"),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct General {
    /// Ask before ⌘Q closes the browser.
    pub confirm_quit: bool,
    /// Where downloads are saved; `None`: `~/Downloads`.
    pub downloads_dir: Option<PathBuf>,
    /// A save sheet for every download.
    pub downloads_ask: bool,
    /// The downloads button always shows, not only while a file comes in.
    pub downloads_button_always: bool,
    pub downloads_open: DownloadsOpen,
    pub webmail: Webmail,
}

/// Where a new tab goes in the row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewTabPlace {
    #[default]
    Next,
    Top,
    End,
}

/// How long a tab left alone stays before it closes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseAfter {
    #[default]
    Never,
    H12,
    Day,
    Week,
    Month,
}

impl CloseAfter {
    pub const ALL: [Self; 5] = [Self::Never, Self::H12, Self::Day, Self::Week, Self::Month];

    /// Hours, `None` for never (Search's `tabs.close.after`: 0/12/24/168/720).
    pub fn hours(self) -> Option<u32> {
        match self {
            Self::Never => None,
            Self::H12 => Some(12),
            Self::Day => Some(24),
            Self::Week => Some(168),
            Self::Month => Some(720),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tabs {
    pub new_place: NewTabPlace,
    /// Links opened in front go beside their page (with [`NewTabPlace::Next`]).
    pub front_beside: bool,
    /// Closing the last tab leaves a new one.
    pub new_after_last: bool,
    /// Closing a tab goes back to the one before.
    pub close_goes_back: bool,
    /// ⌘Z reopens a tab just closed.
    pub undo_reopens: bool,
    /// ⌃Tab goes to a link opened behind first.
    pub switcher_behind_first: bool,
    pub switcher_leaves_out_pins: bool,
    /// Background tabs load when first visited.
    pub lazy_background: bool,
    /// Sized pop-ups open in small windows.
    pub popup_windows: bool,
    /// Resting tabs fade.
    pub sleep_dim: bool,
    pub close_after_hours: CloseAfter,
    /// Every window lists the same open tabs in a space.
    pub mirror: bool,
}

/// A search provider. `Custom` holds a template with `%s` for the query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    #[default]
    Google,
    DuckDuckGo,
    Bing,
    Ecosia,
    Startpage,
    Kagi,
    Brave,
    Qwant,
    Custom(String),
}

impl Engine {
    /// The named engines, in Search's menu order.
    pub const NAMED: [Engine; 8] = [
        Engine::Google,
        Engine::DuckDuckGo,
        Engine::Bing,
        Engine::Ecosia,
        Engine::Startpage,
        Engine::Kagi,
        Engine::Brave,
        Engine::Qwant,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::DuckDuckGo => "DuckDuckGo",
            Self::Bing => "Bing",
            Self::Ecosia => "Ecosia",
            Self::Startpage => "Startpage",
            Self::Kagi => "Kagi",
            Self::Brave => "Brave Search",
            Self::Qwant => "Qwant",
            Self::Custom(_) => "Custom",
        }
    }

    /// The results address with `%s` where the query goes. A custom
    /// template that is not an address with `%s` is Google's.
    pub fn search_template(&self) -> &str {
        match self {
            Self::Google => "https://www.google.com/search?q=%s",
            Self::DuckDuckGo => "https://duckduckgo.com/?q=%s",
            Self::Bing => "https://www.bing.com/search?q=%s",
            Self::Ecosia => "https://www.ecosia.org/search?q=%s",
            Self::Startpage => "https://www.startpage.com/do/search?q=%s",
            Self::Kagi => "https://kagi.com/search?q=%s",
            Self::Brave => "https://search.brave.com/search?q=%s",
            Self::Qwant => "https://www.qwant.com/?q=%s",
            Self::Custom(template) if valid_template(template) => template,
            Self::Custom(_) => Self::Google.search_template(),
        }
    }

    /// The same template with `{query}`, as `omnibox::classify` takes it.
    pub fn omnibox_template(&self) -> String {
        self.search_template().replace("%s", "{query}")
    }

    /// The address that searches for `query`.
    pub fn search_url(&self, query: &str) -> String {
        fill(self.search_template(), query)
    }

    /// Where suggestions for `query` come from, `None` for a custom engine.
    pub fn suggest_url(&self, query: &str) -> Option<String> {
        let template = match self {
            Self::Google => "https://suggestqueries.google.com/complete/search?client=firefox&q=%s",
            Self::DuckDuckGo => "https://duckduckgo.com/ac/?type=list&q=%s",
            Self::Bing => "https://api.bing.com/osjson.aspx?query=%s",
            Self::Ecosia => "https://ac.ecosia.org/autocomplete?type=list&q=%s",
            Self::Startpage => "https://www.startpage.com/suggestions?q=%s&format=opensearch",
            Self::Kagi => "https://kagi.com/api/autosuggest?q=%s",
            Self::Brave => "https://search.brave.com/api/suggest?q=%s",
            Self::Qwant => "https://api.qwant.com/v3/suggest?q=%s",
            Self::Custom(_) => return None,
        };
        Some(fill(template, query))
    }
}

fn fill(template: &str, query: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    template.replace("%s", &encoded)
}

/// An address with `%s` in it that stays an address once filled.
pub fn valid_template(template: &str) -> bool {
    template.contains("%s")
        && url::Url::parse(&template.replace("%s", "x"))
            .is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

/// A word before a search that sends it to a site: `yt cats`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keyword {
    pub keyword: String,
    /// An address with `%s`.
    pub template: String,
}

/// Whether the floating video comes out of its tab on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FloatLeave {
    Never,
    #[default]
    Tabs,
    TabsOrApps,
}

/// The floating video's window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FloatStyle {
    /// Aeri's own window.
    #[default]
    Window,
    PictureInPicture,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Browsing {
    pub engine: Engine,
    /// What Shift-Return searches with; `None`: Return's own.
    pub shift_engine: Option<Engine>,
    /// Send what is typed to the engine for suggestions.
    pub suggest: bool,
    pub keywords: Vec<Keyword>,
    /// Links from other apps open in a small window.
    pub links_little: bool,
    /// ...but go to a tab that already has the page.
    pub links_little_tabs: bool,
    pub reader_twins: bool,
    pub reader_follows: bool,
    pub float_style: FloatStyle,
    pub float_leave: FloatLeave,
    pub float_minimize: bool,
    /// Videos wait for a click.
    pub media_click: bool,
    pub escape_twice: bool,
    pub keep_window_full_screen: bool,
    /// Where every site's zoom starts, 50..=300 by 5.
    pub page_zoom_percent: u16,
    pub capture: bool,
    pub site_styles: bool,
    pub show_link_targets: bool,
}

impl Default for Browsing {
    fn default() -> Self {
        Self {
            engine: Engine::default(),
            shift_engine: None,
            suggest: false,
            keywords: Vec::new(),
            links_little: false,
            links_little_tabs: false,
            reader_twins: false,
            reader_follows: false,
            float_style: FloatStyle::default(),
            float_leave: FloatLeave::default(),
            float_minimize: false,
            media_click: false,
            escape_twice: false,
            keep_window_full_screen: false,
            page_zoom_percent: DEFAULT_ZOOM,
            capture: false,
            site_styles: false,
            show_link_targets: true,
        }
    }
}

impl Browsing {
    /// The address a typed `word rest` goes to, if `word` is a site
    /// shortcut.
    pub fn keyword_url(&self, input: &str) -> Option<String> {
        let (word, rest) = input.trim().split_once(char::is_whitespace)?;
        let rest = rest.trim();
        if rest.is_empty() {
            return None;
        }
        let keyword = self
            .keywords
            .iter()
            .find(|k| k.keyword.eq_ignore_ascii_case(word) && valid_template(&k.template))?;
        Some(fill(&keyword.template, rest))
    }
}

/// The nearest valid page zoom: 50..=300, a multiple of 5.
pub fn clamp_zoom(percent: u16) -> u16 {
    let stepped = (percent.saturating_add(ZOOM_STEP / 2) / ZOOM_STEP) * ZOOM_STEP;
    stepped.clamp(MIN_ZOOM, MAX_ZOOM)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Spaces {
    /// ⌘1-⌘9 switch spaces (tabs move to ⌃1-⌃9).
    pub spaces_command_numbers: bool,
    /// Links to chosen sites open in their space.
    pub links_to_spaces: bool,
    /// Links elsewhere open over the pin, and ⌘W takes it back home.
    pub pins_keep_site: bool,
    pub pinned_rows: bool,
    pub pin_folders: bool,
    pub pin_command_new_tab: bool,
    /// ⌘T raises a floating address field over the page.
    pub newtab_floating: bool,
    pub newtab_recent: bool,
    /// "settings" or "new tab" alone in the address field goes there.
    pub address_commands: bool,
    /// Start with a fresh window: only pins, groups and named tabs return.
    pub start_fresh: bool,
}

impl Default for Spaces {
    fn default() -> Self {
        Self {
            spaces_command_numbers: false,
            links_to_spaces: false,
            pins_keep_site: true,
            pinned_rows: false,
            pin_folders: false,
            pin_command_new_tab: false,
            newtab_floating: false,
            newtab_recent: false,
            address_commands: false,
            start_fresh: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Privacy {
    pub block_ads: bool,
    pub prevent_cross_site_tracking: bool,
    /// `gclid`, `fbclid` and the like go before the page loads.
    pub strip_click_ids: bool,
    /// Global Privacy Control with every request.
    pub gpc: bool,
    /// Sites may ask to send notifications.
    pub notifications_ask: bool,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            block_ads: true,
            prevent_cross_site_tracking: true,
            strip_click_ids: true,
            gpc: true,
            notifications_ask: true,
        }
    }
}

/// The click that peeks at a link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Peek {
    Off,
    #[default]
    Shift,
    OptionShift,
    OptionCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Mouse {
    pub peek: Peek,
    /// A peek stays with its tab.
    pub peek_stays: bool,
    /// ⌥-click on a link opens it in Split View.
    pub option_click_split: bool,
    /// Scroll with the middle button.
    pub autoscroll: bool,
    /// Hold a swipe to pick from history.
    pub swipe_history: bool,
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            peek: Peek::default(),
            peek_stays: false,
            option_click_split: false,
            autoscroll: false,
            swipe_history: true,
        }
    }
}

/// Opens the address field from any app.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Launcher {
    pub on: bool,
    #[serde(with = "chord_text")]
    pub key: Option<Chord>,
    /// Opens in a small window.
    pub little: bool,
}

/// Stores a key in GPUI's keystroke form; one that does not read is none.
mod chord_text {
    use super::*;

    pub fn serialize<S: Serializer>(key: &Option<Chord>, s: S) -> Result<S::Ok, S::Error> {
        key.as_ref().map(ToString::to_string).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Chord>, D::Error> {
        Ok(Option::<String>::deserialize(d)?.and_then(|text| Chord::parse(&text)))
    }
}

/// What the look and the layout do not hold.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceExtras {
    /// Darken pages that stay light.
    pub dusk: bool,
    /// The tab you are on fills as you scroll.
    pub reading_progress: bool,
    /// A tab sinks a little under the pointer.
    pub tabs_give: bool,
    pub bookmarks_bar: bool,
    /// What is playing in other tabs, at the foot of the sidebar.
    pub sidebar_player: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppPrefs {
    pub general: General,
    pub tabs: Tabs,
    pub browsing: Browsing,
    pub spaces: Spaces,
    pub privacy: Privacy,
    pub mouse: Mouse,
    pub launcher: Launcher,
    pub appearance: AppearanceExtras,
}

fn group<T: DeserializeOwned + Default>(root: &Value, key: &str) -> T {
    root.get(key)
        .and_then(|value| T::deserialize(value).ok())
        .unwrap_or_default()
}

impl AppPrefs {
    /// Preferences for an Aeri that ran before they existed: the search
    /// engine stays DuckDuckGo, as it was.
    pub fn existing_install() -> Self {
        let mut prefs = Self::default();
        prefs.browsing.engine = Engine::DuckDuckGo;
        prefs
    }

    /// Reads stored preferences; anything unreadable is its default.
    pub fn from_json(json: &str) -> Self {
        let root: Value = serde_json::from_str(json).unwrap_or(Value::Null);
        let mut prefs = Self {
            general: group(&root, "general"),
            tabs: group(&root, "tabs"),
            browsing: group(&root, "browsing"),
            spaces: group(&root, "spaces"),
            privacy: group(&root, "privacy"),
            mouse: group(&root, "mouse"),
            launcher: group(&root, "launcher"),
            appearance: group(&root, "appearance"),
        };
        prefs.browsing.page_zoom_percent = clamp_zoom(prefs.browsing.page_zoom_percent);
        prefs
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("preferences always serialise")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_searchs() {
        let p = AppPrefs::default();
        assert_eq!(p.browsing.engine, Engine::Google);
        assert!(p.browsing.show_link_targets && !p.browsing.suggest);
        assert_eq!(p.browsing.page_zoom_percent, 100);
        assert_eq!(p.browsing.float_leave, FloatLeave::Tabs);
        assert!(p.spaces.pins_keep_site && !p.spaces.pinned_rows);
        assert!(p.privacy.block_ads && p.privacy.gpc && p.privacy.notifications_ask);
        assert!(p.privacy.strip_click_ids && p.privacy.prevent_cross_site_tracking);
        assert_eq!(p.mouse.peek, Peek::Shift);
        assert!(p.mouse.swipe_history && !p.mouse.autoscroll);
        assert_eq!(p.tabs.new_place, NewTabPlace::Next);
        assert_eq!(p.tabs.close_after_hours, CloseAfter::Never);
        assert_eq!(p.general.downloads_open, DownloadsOpen::Tab);
        assert_eq!(p.general.webmail, Webmail::None);
        assert!(!p.general.confirm_quit && p.launcher.key.is_none());
    }

    #[test]
    fn json_round_trips() {
        let mut p = AppPrefs::default();
        p.general.downloads_dir = Some("/tmp/dl".into());
        p.tabs.close_after_hours = CloseAfter::Week;
        p.browsing.engine = Engine::Custom("https://x.example/?q=%s".into());
        p.browsing.shift_engine = Some(Engine::Kagi);
        p.browsing.keywords.push(Keyword {
            keyword: "yt".into(),
            template: "https://www.youtube.com/results?search_query=%s".into(),
        });
        p.browsing.page_zoom_percent = 125;
        p.mouse.peek = Peek::OptionCommand;
        p.launcher.key = Chord::parse("shift-cmd-space");
        assert_eq!(AppPrefs::from_json(&p.to_json()), p);
    }

    #[test]
    fn old_empty_or_bad_json_reads_to_defaults() {
        assert_eq!(AppPrefs::from_json("{}"), AppPrefs::default());
        assert_eq!(AppPrefs::from_json("not json"), AppPrefs::default());
        assert_eq!(
            AppPrefs::from_json(r#"{"future":{"a":1}}"#),
            AppPrefs::default()
        );
        let partial = AppPrefs::from_json(
            r#"{"tabs":{"mirror":true,"later":1},"browsing":{"engine":"nope"}}"#,
        );
        assert!(partial.tabs.mirror);
        assert_eq!(partial.browsing, Browsing::default());
        let one = AppPrefs::from_json(r#"{"browsing":{"suggest":true}}"#);
        assert!(one.browsing.suggest && one.browsing.show_link_targets);
        assert_eq!(
            AppPrefs::from_json(r#"{"launcher":{"key":"bogus-key"}}"#)
                .launcher
                .key,
            None
        );
    }

    #[test]
    fn zoom_stays_in_range_and_on_steps() {
        assert_eq!(clamp_zoom(10), 50);
        assert_eq!(clamp_zoom(123), 125);
        assert_eq!(clamp_zoom(999), 300);
        assert_eq!(
            AppPrefs::from_json(r#"{"browsing":{"pageZoomPercent":7}}"#)
                .browsing
                .page_zoom_percent,
            50
        );
    }

    #[test]
    fn engines_encode_queries() {
        assert_eq!(
            Engine::Google.search_url("a b&c"),
            "https://www.google.com/search?q=a+b%26c"
        );
        assert_eq!(
            Engine::DuckDuckGo.search_url("rust"),
            "https://duckduckgo.com/?q=rust"
        );
        for engine in Engine::NAMED {
            assert!(valid_template(engine.search_template()), "{engine:?}");
            assert!(
                engine.suggest_url("é").unwrap().contains("%C3%A9"),
                "{engine:?}"
            );
        }
        let custom = Engine::Custom("https://x.example/s?q=%s".into());
        assert_eq!(custom.search_url("a/b"), "https://x.example/s?q=a%2Fb");
        assert_eq!(custom.omnibox_template(), "https://x.example/s?q={query}");
        assert_eq!(custom.suggest_url("a"), None);
        let bad = Engine::Custom("no placeholder".into());
        assert_eq!(bad.search_url("a"), Engine::Google.search_url("a"));
    }

    #[test]
    fn existing_installs_keep_duckduckgo() {
        assert_eq!(
            AppPrefs::existing_install().browsing.engine,
            Engine::DuckDuckGo
        );
    }

    #[test]
    fn site_shortcuts_and_webmail() {
        let mut browsing = Browsing::default();
        browsing.keywords.push(Keyword {
            keyword: "yt".into(),
            template: "https://www.youtube.com/results?search_query=%s".into(),
        });
        assert_eq!(
            browsing.keyword_url("YT cats rule").as_deref(),
            Some("https://www.youtube.com/results?search_query=cats+rule")
        );
        assert_eq!(browsing.keyword_url("yt"), None);
        assert_eq!(browsing.keyword_url("other cats"), None);
        assert_eq!(Webmail::None.compose_url("a@b.c", ""), None);
        assert_eq!(
            Webmail::Gmail.compose_url("a@b.c", "subject=Hi").as_deref(),
            Some("https://mail.google.com/mail/?view=cm&fs=1&to=a%40b.c&subject=Hi")
        );
    }
}
