//! Trust rules that need no engine (Phase 9): which site a page belongs to,
//! whether a page may open a popup, and where a download is saved.

use std::path::{Path, PathBuf};

/// The name a download gets when its suggested name has nothing usable.
pub const FALLBACK_NAME: &str = "download";
/// The longest file name macOS accepts, in bytes.
const MAX_NAME_BYTES: usize = 255;

/// A web page's site as Aeri keys per-site choices: `scheme://host[:port]`
/// (the default port omitted). `None` for anything that is not http(s).
pub fn site_of(url: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.origin().ascii_serialization())
}

/// Whether a page may open a popup (stage 1: only in response to the
/// Person's click or key press). Without a gesture, only a site the Person
/// always allows may, and Aeri's and extensions' own pages. Chrome's popup
/// blocker enforces this rule with the profile's popup site settings; Aeri
/// uses it to tell when Chrome refused a popup, which Chrome does silently.
pub fn popup_allowed(
    user_gesture: bool,
    opener_url: &str,
    site_allowed: impl Fn(&str) -> bool,
) -> bool {
    if user_gesture {
        return true;
    }
    match site_of(opener_url) {
        Some(site) => site_allowed(&site),
        None => ["chrome://", "chrome-extension://"]
            .iter()
            .any(|scheme| opener_url.starts_with(scheme)),
    }
}

/// A download's suggested name made safe as one file name: no path
/// separators or characters macOS forbids, not hidden, not empty, not too
/// long (the extension kept).
pub fn clean_file_name(suggested: &str) -> String {
    let replaced: String = suggested
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let name = replaced
        .trim()
        .trim_start_matches('.')
        .trim_end_matches(['.', ' '])
        .trim();
    if name.is_empty() {
        return FALLBACK_NAME.to_owned();
    }
    truncate_name(name)
}

/// Shortens `name` to [`MAX_NAME_BYTES`], keeping its extension.
fn truncate_name(name: &str) -> String {
    if name.len() <= MAX_NAME_BYTES {
        return name.to_owned();
    }
    let (stem, ext) = split_extension(name);
    // An absurd extension is not worth keeping.
    let ext = if ext.len() > 16 { "" } else { ext };
    let mut end = MAX_NAME_BYTES - ext.len();
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ext}", &stem[..end])
}

/// `("report", ".txt")`; a leading dot does not start an extension.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    }
}

/// Where a download is saved: `dir` joined with the cleaned suggested name,
/// or, if `taken` says that path is used (a file, a partial download, one
/// in progress), the first free `name (n).ext`. Always directly inside
/// `dir`.
pub fn download_path(dir: &Path, suggested: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let name = clean_file_name(suggested);
    let first = dir.join(&name);
    if !taken(&first) {
        return first;
    }
    let (stem, ext) = split_extension(&name);
    (1u32..)
        .map(|n| dir.join(truncate_name(&format!("{stem} ({n}){ext}"))))
        .find(|path| !taken(path))
        .expect("some numbered name is free")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn sites_are_web_origins() {
        assert_eq!(
            site_of("https://www.example.com/a?b#c").as_deref(),
            Some("https://www.example.com")
        );
        assert_eq!(
            site_of("http://127.0.0.1:8766/x").as_deref(),
            Some("http://127.0.0.1:8766")
        );
        assert_eq!(
            site_of("HTTPS://Example.COM:443/").as_deref(),
            Some("https://example.com")
        );
        for url in [
            "about:blank",
            "file:///tmp/a.html",
            "chrome://settings",
            "nonsense",
        ] {
            assert_eq!(site_of(url), None, "{url}");
        }
    }

    #[test]
    fn popups_need_a_gesture_unless_the_site_is_allowed() {
        let allowed = |site: &str| site == "https://ok.test";
        assert!(popup_allowed(true, "https://other.test/", allowed));
        assert!(popup_allowed(false, "https://ok.test/page", allowed));
        assert!(!popup_allowed(false, "https://other.test/", allowed));
        assert!(!popup_allowed(false, "https://ok.test.evil/", allowed));
        assert!(!popup_allowed(false, "file:///tmp/a.html", allowed));
        assert!(!popup_allowed(false, "about:blank", allowed));
        assert!(popup_allowed(
            false,
            "chrome-extension://abc/popup.html",
            allowed
        ));
    }

    #[test]
    fn names_are_cleaned_into_one_visible_file_name() {
        for (suggested, clean) in [
            ("report.txt", "report.txt"),
            ("../../.profile", "_.._.profile"),
            ("a/b\\c:d.txt", "a_b_c_d.txt"),
            ("line\nbreak\0.txt", "line_break_.txt"),
            ("  spaced  ", "spaced"),
            (".hidden", "hidden"),
            ("trailing. . ", "trailing"),
            ("", FALLBACK_NAME),
            ("...", FALLBACK_NAME),
            ("..", FALLBACK_NAME),
            ("/", "_"),
        ] {
            assert_eq!(clean_file_name(suggested), clean, "{suggested:?}");
        }
    }

    #[test]
    fn long_names_keep_their_extension_and_char_boundaries() {
        let name = format!("{}.pdf", "é".repeat(300));
        let clean = clean_file_name(&name);
        assert!(clean.len() <= MAX_NAME_BYTES);
        assert!(clean.ends_with("é.pdf"));
    }

    #[test]
    fn downloads_stay_in_the_folder_and_never_overwrite() {
        let dir = Path::new("/Users/me/Downloads");
        let taken: HashSet<PathBuf> = [
            dir.join("report.txt"),
            dir.join("report (1).txt"),
            dir.join("notes"),
        ]
        .into();
        let is_taken = |p: &Path| taken.contains(p);
        assert_eq!(
            download_path(dir, "report.txt", is_taken),
            dir.join("report (2).txt")
        );
        assert_eq!(download_path(dir, "notes", is_taken), dir.join("notes (1)"));
        assert_eq!(download_path(dir, "new.txt", is_taken), dir.join("new.txt"));
        for suggested in ["../../.profile", "/etc/passwd", "..", "a/../../b"] {
            let path = download_path(dir, suggested, is_taken);
            assert_eq!(path.parent(), Some(dir), "{suggested:?} -> {path:?}");
        }
    }
}
