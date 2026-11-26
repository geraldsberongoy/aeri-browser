//! What the rows of the index ask of the model: the predicates of the
//! dependent rows (Search `SettingsIndex.swift`:773-796), and the words that
//! follow a choice.

use aeri_core::keymap::Shortcut;
use aeri_core::layout::TabPlace;
use aeri_core::omnibox::keyword_site;
use aeri_core::prefs::{AppPrefs, Engine, NewTabPlace, Peek, valid_template};
use aeri_core::proxy::{Proxy, ProxyMode};

use super::Need;
use crate::shell::settings::SettingsModel;

// "sidebar" is the tabs down a side; "hides" is the column kept out until
// the pointer reaches the edge.
pub(super) fn sidebar(m: &SettingsModel) -> bool {
    m.layout.tabs == TabPlace::Side
}

pub(super) fn horizontal(m: &SettingsModel) -> bool {
    m.layout.tabs.horizontal()
}

pub(super) fn titles_in_column(m: &SettingsModel) -> bool {
    sidebar(m) && !m.layout.rail
}

pub(super) fn fold_applies(m: &SettingsModel) -> bool {
    sidebar(m) && m.layout.sidebar_pinned
}

pub(super) fn cards_applies(m: &SettingsModel) -> bool {
    sidebar(m) && m.layout.cards
}

pub(super) const SIDE: Need = Need {
    holds: sidebar,
    text: "Only with the tabs down a side",
};

/// The labels of the Search with menu: the named engines, then Custom.
pub(super) const ENGINE_LABELS: &[&str] = &[
    "Google",
    "DuckDuckGo",
    "Bing",
    "Ecosia",
    "Startpage",
    "Kagi",
    "Brave Search",
    "Qwant",
    "Custom",
];

/// The labels of the Shift-Return menu: Off, then the engines.
pub(super) const SHIFT_LABELS: &[&str] = &[
    "Off",
    "Google",
    "DuckDuckGo",
    "Bing",
    "Ecosia",
    "Startpage",
    "Kagi",
    "Brave Search",
    "Qwant",
    "Custom",
];

pub(super) fn places_after_current(m: &SettingsModel) -> bool {
    m.prefs.tabs.new_place == NewTabPlace::Next
}

/// The ends of the row are the top and bottom of a column, or the left and
/// right of a strip.
pub(super) fn new_place_labels(m: &SettingsModel) -> &'static [&'static str] {
    if sidebar(m) {
        &["After current", "Top", "Bottom"]
    } else {
        &["After current", "Leftmost", "Rightmost"]
    }
}

pub(super) fn custom_engine(m: &SettingsModel) -> bool {
    matches!(m.prefs.browsing.engine, Engine::Custom(_))
}

pub(super) fn custom_template(m: &SettingsModel) -> String {
    match &m.prefs.browsing.engine {
        Engine::Custom(template) => template.clone(),
        _ => String::new(),
    }
}

pub(super) fn set_custom_template(prefs: &mut AppPrefs, template: &str) {
    prefs.browsing.engine = Engine::Custom(template.to_owned());
}

pub(super) fn custom_shift(m: &SettingsModel) -> bool {
    matches!(m.prefs.browsing.shift_engine, Some(Engine::Custom(_)))
}

pub(super) fn shift_template(m: &SettingsModel) -> String {
    match &m.prefs.browsing.shift_engine {
        Some(Engine::Custom(template)) => template.clone(),
        _ => String::new(),
    }
}

pub(super) fn set_shift_template(prefs: &mut AppPrefs, template: &str) {
    prefs.browsing.shift_engine = Some(Engine::Custom(template.to_owned()));
}

/// Search's `shiftDetail`.
pub(super) fn shift_detail(m: &SettingsModel) -> String {
    match &m.prefs.browsing.shift_engine {
        None => "Off: Shift-Return does what Return does".to_owned(),
        Some(Engine::Custom(template)) if !valid_template(template) => {
            "An http or https address with %s where the words go. Until then, Google".to_owned()
        }
        Some(Engine::Custom(template)) => {
            format!(
                "Shift-Return sends what you typed to {}",
                keyword_site(template)
            )
        }
        Some(engine) => format!("Shift-Return sends what you typed to {}", engine.name()),
    }
}

/// Search's `suggestDetail`: said plainly, turned on, what you type leaves
/// the Mac as you type it.
pub(super) fn suggest_detail(m: &SettingsModel) -> String {
    match &m.prefs.browsing.engine {
        Engine::Custom(_) => {
            "A custom engine has no suggestions to give, so nothing is sent".to_owned()
        }
        engine => format!("What you type is sent to {} as you type", engine.name()),
    }
}

pub(super) fn look_is_not_light(m: &SettingsModel) -> bool {
    m.look != 0
}

pub(super) fn webmail_chosen(m: &SettingsModel) -> bool {
    m.prefs.general.webmail != aeri_core::prefs::Webmail::None
}

pub(super) fn browser_detail(m: &SettingsModel) -> String {
    if m.default_browser {
        "Aeri is the default browser on this Mac".to_owned()
    } else {
        "Make Aeri the browser Mail, Slack and the rest send links to".to_owned()
    }
}

pub(super) const NEEDS_PROXY: Need = Need {
    holds: proxy_on,
    text: "Turn the proxy on first",
};

pub(super) fn mail_detail(m: &SettingsModel) -> String {
    if m.default_mail {
        "Email links from other apps open here".to_owned()
    } else {
        "So links from other apps open your webmail too".to_owned()
    }
}

pub(super) fn proxy_on(m: &SettingsModel) -> bool {
    m.prefs.proxy.mode != ProxyMode::System
}

/// Search's note, with Aeri for Search and what Chromium fetches.
pub(super) fn proxy_detail(m: &SettingsModel) -> String {
    if m.prefs.proxy.mode == ProxyMode::System {
        return "Off: Aeri follows the Mac's own network settings.".to_owned();
    }
    "What Aeri fetches goes through it: pages, favicons, search suggestions, extensions, the \
     Web Store and updates. A video call is the exception: WebRTC sends its own traffic over \
     UDP, which a proxy tunnel doesn't carry, so a call can still show a site your address. \
     This Mac and .local names stay direct, so a local server still opens; a router or NAS \
     reached by its address goes through the proxy. A proxy that can't be reached fails \
     rather than going direct."
        .to_owned()
}

pub(super) fn host_text(m: &SettingsModel) -> String {
    m.prefs.proxy.host.clone()
}

pub(super) fn set_host(prefs: &mut AppPrefs, text: &str) {
    prefs.proxy.host = text.trim().into();
}

pub(super) fn host_valid(text: &str) -> bool {
    text.trim().is_empty()
        || Proxy {
            host: text.into(),
            ..Proxy::default()
        }
        .bare_host()
        .is_some()
}

pub(super) fn port_text(m: &SettingsModel) -> String {
    m.prefs.proxy.port.clone()
}

pub(super) fn set_port(prefs: &mut AppPrefs, text: &str) {
    prefs.proxy.port = text.trim().into();
}

pub(super) fn port_valid(text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || text.parse::<u16>().is_ok_and(|n| n > 0)
}

pub(super) fn user_text(m: &SettingsModel) -> String {
    m.prefs.proxy.username.clone()
}

pub(super) fn set_user(prefs: &mut AppPrefs, text: &str) {
    prefs.proxy.username = text.trim().into();
}

pub(super) fn launcher_on(m: &SettingsModel) -> bool {
    m.prefs.launcher.on
}

/// Where downloads go, as `~/Downloads`.
pub(super) fn downloads_folder(m: &SettingsModel) -> String {
    let folder = m.prefs.general.downloads_dir.as_deref();
    let Some(folder) = folder else {
        return "~/Downloads".to_owned();
    };
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home.and_then(|home| folder.strip_prefix(home).ok().map(|rest| rest.to_owned())) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => folder.display().to_string(),
    }
}

pub(super) fn spaces_on(m: &SettingsModel) -> bool {
    m.prefs.spaces.on()
}

pub(super) const NEEDS_SPACES: Need = Need {
    holds: spaces_on,
    text: "Only with Spaces on",
};

pub(super) fn links_little_on(m: &SettingsModel) -> bool {
    m.prefs.browsing.links_little
}

pub(super) fn peek_on(m: &SettingsModel) -> bool {
    m.prefs.mouse.peek != Peek::Off
}

/// Where the site-style row sends the Person: the palette, by the keys it
/// has now.
pub(super) fn site_style_detail(m: &SettingsModel) -> String {
    let keys = m
        .keys
        .iter()
        .find(|row| row.shortcut == Shortcut::CommandPalette)
        .map(|row| row.keys.as_str())
        .filter(|keys| !keys.is_empty());
    let palette = keys.map_or_else(
        || "the command palette".to_owned(),
        |keys| format!("the command palette ({keys})"),
    );
    format!("Edit Style for the site you're on, in {palette}, puts CSS of yours on it")
}
