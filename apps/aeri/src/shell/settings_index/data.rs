//! The index's data: the groups and every row (Search `SettingsIndex.swift`
//! :55-560), with the predicates of the dependent rows.

use aeri_core::layout::TabPlace;

use super::{Action, Control, Extra, Group, Item, Need, Page, Picture, Pill, Pref};
use crate::shell::settings::SettingsModel;

// Predicates over the model (SettingsIndex.swift:773-796): "sidebar" is the
// tabs down a side; "hides" is the column kept out until the pointer
// reaches the edge.
fn sidebar(m: &SettingsModel) -> bool {
    m.layout.tabs == TabPlace::Side
}

fn top(m: &SettingsModel) -> bool {
    m.layout.tabs == TabPlace::Top
}

fn titles_in_column(m: &SettingsModel) -> bool {
    sidebar(m) && !m.layout.rail
}

fn fold_applies(m: &SettingsModel) -> bool {
    sidebar(m) && m.layout.sidebar_pinned
}

fn cards_applies(m: &SettingsModel) -> bool {
    sidebar(m) && m.layout.cards
}

const SIDE: Need = Need {
    holds: sidebar,
    text: "Only with the tabs down a side",
};

const BASE: Item = Item {
    id: "",
    group: "",
    icon: "",
    title: "",
    detail: "",
    more: None,
    words: &[],
    wide: false,
    control: Control::Pills(&[]),
    need: None,
    live_detail: None,
};

macro_rules! item {
    ($id:expr, $group:expr, $icon:expr, $title:expr, $detail:expr, $control:expr $(, $field:ident : $value:expr)* $(,)?) => {
        Item {
            id: $id,
            group: $group,
            icon: $icon,
            title: $title,
            detail: $detail,
            control: $control,
            $($field: $value,)*
            ..BASE
        }
    };
}

pub static GROUPS: &[Group] = &[
    Group {
        id: "theme",
        page: Page::Appearance,
        title: "Light and dark",
        icon: "circle.lefthalf.filled",
        extra: Extra::None,
    },
    Group {
        id: "window",
        page: Page::Appearance,
        title: "Window",
        icon: "macwindow",
        extra: Extra::None,
    },
    Group {
        id: "tablook",
        page: Page::Appearance,
        title: "Tabs",
        icon: "textformat.size",
        extra: Extra::None,
    },
    Group {
        id: "motion",
        page: Page::Appearance,
        title: "Motion",
        icon: "wind",
        extra: Extra::None,
    },
    Group {
        id: "themes",
        page: Page::Appearance,
        title: "Theme",
        icon: "paintpalette",
        extra: Extra::Themes,
    },
    Group {
        id: "memory",
        page: Page::Tabs,
        title: "Memory",
        icon: "moon.zzz",
        extra: Extra::None,
    },
    Group {
        id: "tabbar",
        page: Page::Tabs,
        title: "In the tab bar",
        icon: "number",
        extra: Extra::None,
    },
    Group {
        id: "passwords",
        page: Page::Passwords,
        title: "Your passwords",
        icon: "key",
        extra: Extra::None,
    },
    Group {
        id: "clear",
        page: Page::Privacy,
        title: "Clear",
        icon: "trash",
        extra: Extra::None,
    },
    Group {
        id: "permissions",
        page: Page::Privacy,
        title: "Site permissions",
        icon: "checkmark.shield",
        extra: Extra::None,
    },
    Group {
        id: "extensions",
        page: Page::Extensions,
        title: "Installed",
        icon: "puzzlepiece.extension",
        extra: Extra::Extensions,
    },
    Group {
        id: "shortcuts",
        page: Page::Keyboard,
        title: "Shortcuts",
        icon: "command",
        extra: Extra::Shortcuts,
    },
];

const LOOK_PICKS: &[(Picture, &str)] = &[
    (Picture::Light, "Light"),
    (Picture::Dark, "Dark"),
    (Picture::Auto, "Auto"),
];

const TABS_PICKS: &[(Picture, &str)] = &[
    (Picture::Top, "Top"),
    (Picture::Left, "Left"),
    (Picture::Right, "Right"),
    (Picture::IconsOnly, "Icons only"),
];

const CARD_PICKS: &[(Picture, &str)] = &[
    (Picture::Card, "As a card"),
    (Picture::Edge, "Edge to edge"),
];

/// SleepChoice.detail (SettingsIndex.swift:~745), as written in Sleep.swift.
fn sleep_note(m: &SettingsModel) -> &'static str {
    match m.sleep {
        0 => "Every tab keeps its page, however long you leave it.",
        1 => "After two hours away, a tab rests and wakes when you go back to it.",
        2 => "After half an hour away, a tab rests and wakes when you go back to it.",
        _ => "After ten minutes away, a tab rests and wakes when you go back to it.",
    }
}

fn provider_detail(m: &SettingsModel) -> String {
    format!(
        "Saved sign-ins are kept for each profile on its own; this is {}",
        if m.profile.is_empty() {
            "this profile"
        } else {
            &m.profile
        }
    )
}

pub static ITEMS: &[Item] = &[
    // Appearance
    item!("look", "theme", "circle.lefthalf.filled", "Appearance",
        "Light, dark, or whatever the Mac is doing. Pages follow it too",
        Control::Pictures { pref: Pref::Look, picks: LOOK_PICKS },
        words: &["dark mode", "light mode", "theme", "system"], wide: true),
    item!("tabs.layout", "window", "sidebar.left", "Tabs",
        "Across the top of the window, down a side as a column, or as icons only",
        Control::Pictures { pref: Pref::TabsLayout, picks: TABS_PICKS },
        more: Some("Icons only is a narrow column of each tab's icon, the title in a tooltip. Nothing grows under the pointer."),
        words: &["sidebar", "vertical tabs", "column", "left", "right", "top", "tab bar", "position", "icons only", "compact", "collapsed", "narrow", "favicons"],
        wide: true),
    item!("sidebar.hides", "window", "sidebar.squares.left", "Hide the column until the pointer reaches the edge",
        "The page takes the whole window; ⌘S keeps the tabs out",
        Control::Switch(Pref::HideColumn),
        words: &["auto hide", "sidebar"], need: Some(SIDE)),
    item!("sidebar.slides", "window", "rectangle.lefthalf.inset.filled.arrow.left", "Slide the column in and out",
        "A gentler slide when ⌘S or the window's edge brings the column out or puts it away",
        Control::Switch(Pref::Slides),
        more: Some("With Reduce Motion on in System Settings › Accessibility › Display, the column fades in and out instead. With Reduce animations on, it comes and goes at once."),
        words: &["animation", "animate", "smooth", "slide", "sidebar", "arc"], need: Some(SIDE)),
    item!("sidebar.foldIcons", "window", "arrow.down.right.and.arrow.up.left", "⌘S makes the column icons only",
        "The tabs stay in sight as a narrow column of icons; ⌘S again brings the titles back",
        Control::Switch(Pref::FoldIcons),
        more: Some("Off, ⌘S hides the column and the page takes the whole window."),
        words: &["collapse", "collapsed", "compact", "narrow", "hide sidebar", "icons only", "favicons"],
        need: Some(Need { holds: fold_applies, text: "Only with the column down a side and not hidden until the edge" })),
    item!("layout.cards", "window", "rectangle.inset.filled", "The page as a card",
        "A little room around the page, with rounded corners",
        Control::Pictures { pref: Pref::CardLook, picks: CARD_PICKS },
        more: Some("Off, the page reaches the window's edges and the column is white with its hairline."),
        words: &["edge to edge", "rounded", "margin"], wide: true, need: Some(SIDE)),
    item!("layout.cardsFullScreen", "window", "arrow.up.left.and.arrow.down.right", "Keep the card in full screen",
        "Off, the page goes edge to edge when the window fills the screen",
        Control::Switch(Pref::CardsFullScreen),
        words: &["full screen", "fullscreen", "card", "margin", "rounded", "zen", "arc"],
        need: Some(Need { holds: cards_applies, text: "Only with the tabs down a side" })),
    item!("toolbar.left", "window", "arrow.left.arrow.right", "Back, forward and reload on the left",
        "Beside the window's buttons, before the tabs",
        Control::Switch(Pref::NavLeft),
        words: &["toolbar", "buttons", "navigation"],
        need: Some(Need { holds: top, text: "Only with the tabs across the top" })),
    item!("tabs.compact", "window", "rectangle.compress.vertical", "Shorter tab bar",
        "Only a few points above and below the tabs, for more of the page",
        Control::Switch(Pref::CompactStrip),
        words: &["compact", "slim", "thin", "height", "padding", "tab bar", "toolbar"],
        need: Some(Need { holds: top, text: "Only with the tabs across the top" })),
    item!("sidebar.roomy", "tablook", "textformat.size.larger", "Roomier tabs",
        "Taller rows and larger titles in the column",
        Control::Switch(Pref::Roomy),
        words: &["bigger", "larger", "text size", "accessibility"],
        need: Some(Need { holds: titles_in_column, text: "Only with titles in the column" })),
    item!("glyph", "tablook", "app", "Tabs show",
        "Beside the title, and on a pinned square",
        Control::Segmented { pref: Pref::Glyph, labels: &["Letters", "Site icons"], note: None },
        words: &["favicon", "icons", "letters"]),
    item!("tabs.highContrast", "tablook", "bold", "High contrast text",
        "Tab titles in black or white, instead of soft grey",
        Control::Switch(Pref::HighContrast),
        words: &["contrast", "accessibility", "bold", "readable", "black", "white"]),
    item!("motion.light", "motion", "leaf", "Save power in Low Power Mode",
        "Simpler motion while saving power or running hot",
        Control::Switch(Pref::LightMotion),
        more: Some("In Low Power Mode, or while the Mac runs hot. Pages are left as they are."),
        words: &["battery", "animations", "motion"]),
    item!("motion.instant", "motion", "bolt", "Reduce animations",
        "Switching tabs, spaces and panels happens at once, without sliding or fading",
        Control::Switch(Pref::ReduceAnimations),
        more: Some("For Aeri alone, whatever Reduce Motion in System Settings says. The tab ⌘1–⌘9 picks is lit at once, and the page shows at once."),
        words: &["animations", "motion", "instant", "faster", "reduce motion", "transitions"]),
    item!("theme.edit", "themes", "slider.horizontal.3", "Theme for all windows",
        "Colours, texture and motion; a space or a profile can have a theme of its own from the palette",
        Control::Pills(&[Pill { action: Action::EditTheme, label: "Edit Theme for All Windows…", filled: false }]),
        words: &["colour", "color", "texture", "density", "text size", "editor", "recipe"]),
    item!("theme.files", "themes", "square.and.arrow.down.on.square", "Theme files",
        "Keep the theme you're using in your library, or trade one as a file",
        Control::Pills(&[
            Pill { action: Action::SaveTheme, label: "Save to Library", filled: false },
            Pill { action: Action::ImportTheme, label: "Import…", filled: false },
            Pill { action: Action::ExportTheme, label: "Export…", filled: false },
        ]),
        words: &["import", "export", "library", "save", "file"]),
    // Tabs
    item!("tabs.sleep", "memory", "moon.zzz", "Sleep tabs you aren't using",
        "A resting tab lets its page go and comes back where you left it",
        Control::Segmented {
            pref: Pref::Sleep,
            labels: &["Never", "Later", "Normal", "Sooner"],
            note: Some(sleep_note),
        },
        more: Some("Tabs playing sound, in a call or with something typed stay awake, and so do sites you keep awake."),
        words: &["memory", "discard", "suspend", "ram", "rest"], wide: true),
    item!("tabs.shortcuts", "tabbar", "command", "Show tab numbers while ⌘ is held",
        "Which tab each ⌘1–9 selects",
        Control::Switch(Pref::TabNumbers),
        words: &["numbers", "shortcut"]),
    item!("tabs.address", "tabbar", "lock", "Show the address on the tab you're on",
        "Its site in place of its title; the title comes back under the pointer",
        Control::Switch(Pref::AddressOnTab),
        more: Some("Look-alike names in other alphabets show as they really are, and plain http says Not secure. Off, the tab you're on still says so for those two."),
        words: &["url", "domain", "security", "address bar"]),
    // Passwords
    item!("passwords.manage", "passwords", "key.viewfinder", "Your passwords",
        "Look at, change and delete what this profile has saved",
        Control::Pills(&[Pill { action: Action::ManagePasswords, label: "Manage Passwords…", filled: false }]),
        words: &["sign-ins", "keychain", "manage"]),
    item!("passwords.provider", "passwords", "square.and.arrow.down", "Offer to save passwords",
        "",
        Control::Menu {
            pref: Pref::Provider,
            labels: &["Save Passwords in Aeri", "Use a Password-Manager Extension", "Don't Save Passwords"],
        },
        words: &["passwords", "extension", "password manager", "autofill", "save", "profile"],
        live_detail: Some(provider_detail)),
    // Privacy
    item!("clear.history", "clear", "clock.arrow.circlepath", "History",
        "Every address you have been to",
        Control::Pills(&[Pill { action: Action::ClearHistory, label: "Clear History…", filled: false }]),
        words: &["clear history"]),
    item!("clear.data", "clear", "internaldrive", "Cookies, sign-ins and cache",
        "Sign out of every site, or only empty what was fetched to draw pages",
        Control::Pills(&[Pill { action: Action::ClearData, label: "Clear Browsing Data…", filled: false }]),
        words: &["cookies", "sign out", "clear cache", "empty caches", "cache storage", "browsing data"]),
    item!("privacy.sites", "permissions", "arrow.uturn.backward.circle", "Camera, microphone, location and notifications",
        "What each site was allowed or refused",
        Control::Pills(&[Pill { action: Action::SiteSettings, label: "Site Settings…", filled: false }]),
        words: &["permissions", "camera", "microphone", "location", "reset", "notifications"]),
    // Extensions
    item!("extensions.store", "extensions", "arrow.down.app", "Add an extension",
        "Extensions come from the Chrome Web Store and work as they do in Chrome",
        Control::Pills(&[Pill { action: Action::WebStore, label: "Chrome Web Store…", filled: true }]),
        words: &["chrome web store", "install", "add-on", "plugin"]),
];
