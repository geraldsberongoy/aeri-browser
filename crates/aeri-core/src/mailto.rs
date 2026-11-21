//! `mailto:` links written in a webmail (Search `Webmail.swift`): the link
//! is read as RFC 6068 has it, and the chosen webmail's compose page is
//! built with the draft filled in.
//!
//! Addresses come before the `?`, comma separated; then `to`, `cc`, `bcc`,
//! `subject` and `body` fields, each percent-encoded. `+` is a plus, never a
//! space. Other fields (In-Reply-To and the rest) are left out: a webmail's
//! compose page has nowhere to put them. Every value is encoded down to the
//! unreserved characters, so nothing in one field can start another.

use crate::prefs::Webmail;

/// Most recipients kept per field.
const MAX_RECIPIENTS: usize = 50;
/// Longest subject kept, in characters.
const MAX_SUBJECT: usize = 500;
/// Longest compose address; the body is cut to fit. Webmails start refusing
/// addresses somewhere past 8 KB.
const MAX_LENGTH: usize = 8000;

/// A `mailto:` link, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MailDraft {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
}

impl MailDraft {
    /// The draft a `mailto:` link holds; `None` for any other address.
    pub fn parse(link: &str) -> Option<Self> {
        let scheme = link.get(..7)?;
        if !scheme.eq_ignore_ascii_case("mailto:") {
            return None;
        }
        let mut rest = &link[7..];
        // A fragment means nothing to a mailto (RFC 6068 §2).
        if let Some(hash) = rest.find('#') {
            rest = &rest[..hash];
        }
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let mut draft = Self {
            to: addresses(path),
            ..Self::default()
        };
        let (mut subject, mut body) = (None, None);
        for field in query.split('&').filter(|field| !field.is_empty()) {
            let (name, value) = field.split_once('=').unwrap_or((field, ""));
            match decode(name).to_ascii_lowercase().as_str() {
                "to" => draft.to.extend(addresses(value)),
                "cc" => draft.cc.extend(addresses(value)),
                "bcc" => draft.bcc.extend(addresses(value)),
                // The first of each: a second subject can't be meant.
                "subject" => subject = subject.or_else(|| Some(decode(value))),
                "body" => body = body.or_else(|| Some(decode(value))),
                _ => {}
            }
        }
        for list in [&mut draft.to, &mut draft.cc, &mut draft.bcc] {
            list.truncate(MAX_RECIPIENTS);
        }
        // One line: a subject has no line breaks to keep.
        draft.subject = subject
            .unwrap_or_default()
            .split(['\r', '\n'])
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(MAX_SUBJECT)
            .collect();
        // Line breaks come as CRLF (%0D%0A); a webmail's text box wants \n.
        draft.body = body
            .unwrap_or_default()
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        Some(draft)
    }

    /// The draft as a clean `mailto:` again: for Proton Mail, and for the
    /// system's mail app.
    pub fn mailto(&self) -> String {
        let mut fields = Vec::new();
        if !self.cc.is_empty() {
            fields.push(format!("cc={}", encode(&self.cc.join(","), "@,+")));
        }
        if !self.bcc.is_empty() {
            fields.push(format!("bcc={}", encode(&self.bcc.join(","), "@,+")));
        }
        if !self.subject.is_empty() {
            fields.push(format!("subject={}", encode(&self.subject, "")));
        }
        if !self.body.is_empty() {
            fields.push(format!(
                "body={}",
                encode(&self.body.replace('\n', "\r\n"), "")
            ));
        }
        let query = if fields.is_empty() {
            String::new()
        } else {
            format!("?{}", fields.join("&"))
        };
        format!("mailto:{}{query}", encode(&self.to.join(","), "@,+"))
    }
}

impl Webmail {
    /// The compose page of this webmail for the `mailto:` link `link`, or
    /// `None` when no webmail is chosen or `link` is not a `mailto:`.
    pub fn compose_mailto(self, link: &str) -> Option<String> {
        let draft = MailDraft::parse(link)?;
        self.compose_draft(&draft)
    }

    /// The compose page with `draft` filled in, the body cut to keep the
    /// address within [`MAX_LENGTH`].
    pub fn compose_draft(self, draft: &MailDraft) -> Option<String> {
        let whole = self.address(draft)?;
        if whole.len() <= MAX_LENGTH {
            return Some(whole);
        }
        let chars: Vec<char> = draft.body.chars().collect();
        let with = |count: usize| MailDraft {
            body: chars[..count].iter().collect(),
            ..draft.clone()
        };
        let (mut low, mut high) = (0, chars.len());
        let mut best = with(0);
        while low <= high {
            let mid = (low + high) / 2;
            let trial = with(mid);
            if self.address(&trial)?.len() <= MAX_LENGTH {
                best = trial;
                low = mid + 1;
            } else if mid == 0 {
                break;
            } else {
                high = mid - 1;
            }
        }
        self.address(&best)
    }

    /// Gmail, Outlook and Fastmail take each field as its own parameter.
    /// Proton Mail takes a whole `mailto:` in the fragment, as its own
    /// handler (`/inbox#mailto=%s`) does.
    fn address(self, draft: &MailDraft) -> Option<String> {
        let (to, cc, bcc) = (draft.to.join(","), draft.cc.join(","), draft.bcc.join(","));
        let query = |pairs: [(&str, &str); 5]| -> String {
            pairs
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(name, value)| format!("{name}={}", encode(value, "")))
                .collect::<Vec<_>>()
                .join("&")
        };
        let with = |base: &str, separator: &str, rest: String| {
            if rest.is_empty() {
                base.to_owned()
            } else {
                format!("{base}{separator}{rest}")
            }
        };
        Some(match self {
            Self::None => return None,
            Self::Gmail => with(
                "https://mail.google.com/mail/?view=cm&fs=1",
                "&",
                query([
                    ("to", &to),
                    ("cc", &cc),
                    ("bcc", &bcc),
                    ("su", &draft.subject),
                    ("body", &draft.body),
                ]),
            ),
            Self::Outlook => with(
                "https://outlook.live.com/mail/0/deeplink/compose",
                "?",
                query([
                    ("to", &to),
                    ("cc", &cc),
                    ("bcc", &bcc),
                    ("subject", &draft.subject),
                    ("body", &draft.body),
                ]),
            ),
            Self::Fastmail => with(
                "https://app.fastmail.com/mail/compose",
                "?",
                query([
                    ("to", &to),
                    ("cc", &cc),
                    ("bcc", &bcc),
                    ("subject", &draft.subject),
                    ("body", &draft.body),
                ]),
            ),
            Self::Proton => format!(
                "https://mail.proton.me/inbox#mailto={}",
                encode(&draft.mailto(), "")
            ),
        })
    }
}

/// The addresses of a field, percent-decoded and trimmed; empty ones and
/// ones with control characters are dropped.
fn addresses(text: &str) -> Vec<String> {
    decode(text)
        .split(',')
        .map(|address| address.trim().to_owned())
        .filter(|address| !address.is_empty() && !address.chars().any(char::is_control))
        .collect()
}

/// `%XX` decoded as UTF-8; text that is not valid percent-encoding is kept
/// as it came rather than lost.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |byte: u8| (byte as char).to_digit(16);
        if bytes[at] == b'%'
            && let (Some(high), Some(low)) = (
                bytes.get(at + 1).copied().and_then(hex),
                bytes.get(at + 2).copied().and_then(hex),
            )
        {
            out.push((high * 16 + low) as u8);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// Everything but the unreserved characters (RFC 3986) and `keeping`
/// percent-encoded: `&`, `=`, `#`, `+`, `?`, `%` and spaces included.
fn encode(text: &str, keeping: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || keeping.as_bytes().contains(&byte)
        {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_draft() {
        let draft = MailDraft::parse(
            "MAILTO:a@b.c,d@e.f?cc=g@h.i&subject=Hi%20there&body=one%0D%0Atwo&x-ignored=1#frag",
        )
        .unwrap();
        assert_eq!(draft.to, ["a@b.c", "d@e.f"]);
        assert_eq!(draft.cc, ["g@h.i"]);
        assert_eq!(draft.subject, "Hi there");
        assert_eq!(draft.body, "one\ntwo");
        assert!(MailDraft::parse("https://a.b").is_none());
        // A plus stays a plus.
        assert_eq!(MailDraft::parse("mailto:a+b@c.d").unwrap().to, ["a+b@c.d"]);
    }

    #[test]
    fn webmail_pages() {
        let link = "mailto:a@b.c?subject=Hi&body=x%26y";
        assert_eq!(Webmail::None.compose_mailto(link), None);
        assert_eq!(
            Webmail::Gmail.compose_mailto(link).as_deref(),
            Some("https://mail.google.com/mail/?view=cm&fs=1&to=a%40b.c&su=Hi&body=x%26y")
        );
        assert_eq!(
            Webmail::Outlook.compose_mailto(link).as_deref(),
            Some(
                "https://outlook.live.com/mail/0/deeplink/compose?to=a%40b.c&subject=Hi&body=x%26y"
            )
        );
        assert_eq!(
            Webmail::Fastmail.compose_mailto("mailto:").as_deref(),
            Some("https://app.fastmail.com/mail/compose")
        );
        assert_eq!(
            Webmail::Proton.compose_mailto("mailto:a@b.c").as_deref(),
            Some("https://mail.proton.me/inbox#mailto=mailto%3Aa%40b.c")
        );
    }

    #[test]
    fn a_long_body_is_cut_to_fit() {
        let link = format!("mailto:a@b.c?body={}", "x".repeat(20_000));
        let page = Webmail::Gmail.compose_mailto(&link).unwrap();
        assert!(page.len() <= MAX_LENGTH && page.len() > MAX_LENGTH - 10);
    }
}
