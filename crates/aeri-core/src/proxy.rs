//! Settings › Privacy › Proxy (Search `Proxy.swift`): off unless turned on,
//! an HTTP or SOCKS5 proxy everything the browser fetches goes through. The
//! password is the one secret and lives in the keychain, never here.

use std::net::Ipv6Addr;

use serde::{Deserialize, Serialize};

/// Kept direct whatever the proxy says: this Mac and `.local` (Bonjour)
/// names. A proxy that swallowed these would leave a local server unreachable
/// while it was on. Chromium's bypass-rule syntax.
pub const DIRECT: &str = "localhost;127.0.0.1;[::1];*.local";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    /// Off: the Mac's own network settings.
    #[default]
    System,
    Http,
    Socks5,
}

impl ProxyMode {
    pub const ALL: [Self; 3] = [Self::System, Self::Http, Self::Socks5];

    /// Search's menu titles.
    pub fn title(self) -> &'static str {
        match self {
            Self::System => "Off, or the Mac's own",
            Self::Http => "HTTP",
            Self::Socks5 => "SOCKS5",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Proxy {
    pub mode: ProxyMode,
    pub host: String,
    pub port: String,
    pub username: String,
}

impl Proxy {
    /// The host as typed, in the form a proxy address wants: an IPv6 address
    /// is bare here. A port or a scheme typed into the field is refused, as
    /// is a colon that is not part of an IPv6 address.
    pub fn bare_host(&self) -> Option<String> {
        let mut text = self.host.trim();
        if let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            text = inner;
        }
        let refused = text.is_empty()
            || text.contains(char::is_whitespace)
            || text.contains("://")
            || text.contains('/')
            || text.contains('@');
        if refused || (text.contains(':') && text.parse::<Ipv6Addr>().is_err()) {
            return None;
        }
        Some(text.to_owned())
    }

    pub fn port_number(&self) -> Option<u16> {
        self.port.trim().parse().ok().filter(|port| *port > 0)
    }

    /// What Settings says under the fields, or `None` when it is ready.
    pub fn validation(&self) -> Option<&'static str> {
        if self.mode == ProxyMode::System {
            return None;
        }
        if self.bare_host().is_none() {
            return Some(
                "Enter a host name or IP address on its own, without a scheme, a port or a path.",
            );
        }
        if self.port_number().is_none() {
            return Some("Enter a port from 1 to 65535.");
        }
        None
    }

    /// Whether a proxy is set and whole: a proxy is either whole and used,
    /// or absent.
    pub fn is_on(&self) -> bool {
        self.mode != ProxyMode::System && self.validation().is_none()
    }

    /// Chromium's proxy server string (`socks5://host:port`,
    /// `http://host:port`), `None` while the proxy is off or half filled.
    pub fn server(&self) -> Option<String> {
        if !self.is_on() {
            return None;
        }
        let host = self.bare_host()?;
        let port = self.port_number()?;
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host
        };
        let scheme = match self.mode {
            ProxyMode::Socks5 => "socks5",
            _ => "http",
        };
        Some(format!("{scheme}://{host}:{port}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy(mode: ProxyMode, host: &str, port: &str) -> Proxy {
        Proxy {
            mode,
            host: host.into(),
            port: port.into(),
            username: String::new(),
        }
    }

    #[test]
    fn off_by_default_and_when_half_filled() {
        assert_eq!(Proxy::default().server(), None);
        assert_eq!(proxy(ProxyMode::Http, "127.0.0.1", "").server(), None);
        assert!(proxy(ProxyMode::Http, "", "80").validation().is_some());
    }

    #[test]
    fn servers() {
        assert_eq!(
            proxy(ProxyMode::Socks5, "127.0.0.1", "7890")
                .server()
                .as_deref(),
            Some("socks5://127.0.0.1:7890")
        );
        assert_eq!(
            proxy(ProxyMode::Http, "[::1]", "8080").server().as_deref(),
            Some("http://[::1]:8080")
        );
    }

    #[test]
    fn a_port_or_scheme_in_the_host_is_refused() {
        for host in ["localhost:8080", "http://x", "a/b", "u@h", "a b"] {
            assert!(
                proxy(ProxyMode::Http, host, "80").validation().is_some(),
                "{host}"
            );
        }
        assert!(proxy(ProxyMode::Http, "h", "0").validation().is_some());
        assert!(proxy(ProxyMode::Http, "h", "70000").validation().is_some());
    }
}
