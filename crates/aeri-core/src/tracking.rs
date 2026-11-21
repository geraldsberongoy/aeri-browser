//! Click identifiers: query parameters whose only job is to tie a visit to
//! an ad click or an email (`gclid`, `fbclid`, ...). Aeri removes them from
//! an address before the page loads (`privacy.strip_click_ids`).

/// Parameter names that are click identifiers, lowercase. Sorted for the
/// binary search in [`is_click_id`].
const CLICK_IDS: &[&str] = &[
    "__hsfp",
    "__hssc",
    "__hstc",
    "_hsenc",
    "_hsmi",
    "_openstat",
    "dclid",
    "fbclid",
    "gbraid",
    "gclid",
    "gclsrc",
    "igshid",
    "li_fat_id",
    "mc_eid",
    "mkt_tok",
    "msclkid",
    "oly_anon_id",
    "oly_enc_id",
    "rb_clickid",
    "ttclid",
    "twclid",
    "vero_id",
    "wbraid",
    "wickedid",
    "yclid",
];

/// Whether the query parameter called `name` is a click identifier.
pub fn is_click_id(name: &str) -> bool {
    // Names are short; lowercase into a stack buffer so nothing allocates.
    let mut buffer = [0u8; 16];
    let Some(slot) = buffer.get_mut(..name.len()) else {
        return false;
    };
    slot.copy_from_slice(name.as_bytes());
    slot.make_ascii_lowercase();
    std::str::from_utf8(slot).is_ok_and(|lower| CLICK_IDS.binary_search(&lower).is_ok())
}

/// `url` without its click identifiers, or `None` when it has none (the
/// common case, which allocates nothing). Only `http` and `https`
/// addresses are touched; the fragment and the other parameters keep their
/// text and order.
pub fn strip_click_ids(url: &str) -> Option<String> {
    let is_web = url
        .get(..8)
        .is_some_and(|head| head.eq_ignore_ascii_case("https://"))
        || url
            .get(..7)
            .is_some_and(|head| head.eq_ignore_ascii_case("http://"));
    if !is_web {
        return None;
    }
    let (before_fragment, fragment) = match url.split_once('#') {
        Some((head, tail)) => (head, Some(tail)),
        None => (url, None),
    };
    let (base, query) = before_fragment.split_once('?')?;
    let tracked = |pair: &str| {
        let name = pair.split_once('=').map_or(pair, |(name, _)| name);
        is_click_id(name)
    };
    if !query.split('&').any(tracked) {
        return None;
    }
    let mut out = String::with_capacity(url.len());
    out.push_str(base);
    let mut first = true;
    for pair in query
        .split('&')
        .filter(|pair| !pair.is_empty() && !tracked(pair))
    {
        out.push(if first { '?' } else { '&' });
        out.push_str(pair);
        first = false;
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted() {
        assert!(CLICK_IDS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn removes_a_lone_parameter_and_its_question_mark() {
        assert_eq!(
            strip_click_ids("https://example.com/a?gclid=abc").as_deref(),
            Some("https://example.com/a")
        );
    }

    #[test]
    fn keeps_other_parameters_in_order() {
        assert_eq!(
            strip_click_ids("https://e.com/?a=1&fbclid=x&b=2&msclkid=y&c").as_deref(),
            Some("https://e.com/?a=1&b=2&c")
        );
    }

    #[test]
    fn keeps_the_fragment() {
        assert_eq!(
            strip_click_ids("http://e.com/p?yclid=1#top").as_deref(),
            Some("http://e.com/p#top")
        );
        assert_eq!(
            strip_click_ids("https://e.com/?q=1&twclid=2#a?gclid=3").as_deref(),
            Some("https://e.com/?q=1#a?gclid=3")
        );
    }

    #[test]
    fn names_match_without_case_and_whole() {
        assert_eq!(
            strip_click_ids("https://e.com/?GCLID=1&x=2").as_deref(),
            Some("https://e.com/?x=2")
        );
        assert_eq!(strip_click_ids("https://e.com/?mygclid=1&gclidx"), None);
        assert_eq!(strip_click_ids("https://e.com/?a=gclid=1"), None);
    }

    #[test]
    fn valueless_and_hubspot_ids() {
        assert_eq!(
            strip_click_ids("https://e.com/?_hsenc=1&_hsmi=2&igshid&mc_eid=3&dclid=4&li_fat_id=5")
                .as_deref(),
            Some("https://e.com/")
        );
    }

    #[test]
    fn untouched_addresses_allocate_nothing() {
        assert_eq!(strip_click_ids("https://e.com/path"), None);
        assert_eq!(strip_click_ids("https://e.com/?a=1"), None);
        assert_eq!(strip_click_ids("https://e.com/#gclid=1"), None);
        assert_eq!(strip_click_ids("ftp://e.com/?gclid=1"), None);
        assert_eq!(strip_click_ids("about:blank?gclid=1"), None);
        assert_eq!(strip_click_ids(""), None);
    }
}
