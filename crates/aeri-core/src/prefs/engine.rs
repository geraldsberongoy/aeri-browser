//! Search engines and keywords: where an address-bar search goes.

use serde::{Deserialize, Serialize};

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
            Self::Startpage => "https://www.startpage.com/osuggestions?q=%s",
            Self::Kagi => "https://kagi.com/api/autosuggest?q=%s",
            Self::Brave => "https://search.brave.com/api/suggest?q=%s",
            Self::Qwant => "https://api.qwant.com/api/suggest/?q=%s&client=opensearch",
            Self::Custom(_) => return None,
        };
        Some(fill(template, query))
    }
}

pub(super) fn fill(template: &str, query: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
