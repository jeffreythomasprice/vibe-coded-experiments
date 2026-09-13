//! Validates a STUN server URL before it ever reaches `RtcIceServer::set_urls_str`, which accepts
//! any string and only fails much later when `RTCPeerConnection::new_with_configuration` throws —
//! by then there's no way to say which URL was the problem.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StunUrlError {
    #[error("can't be empty")]
    Empty,
    #[error("can't contain spaces")]
    Whitespace,
    #[error("must start with \"stun:\" or \"stuns:\"")]
    MissingScheme,
    #[error("\"{0}:\" isn't a STUN scheme (no TURN support here \u{2014} see MULTIPLAYER.md)")]
    UnsupportedScheme(String),
    #[error("missing a host after the scheme")]
    MissingHost,
    #[error("a STUN address is just a host and optional port \u{2014} no path, query, or user")]
    HostSyntax,
    #[error("the port must be a number from 1 to 65535")]
    Port,
}

/// Accepts only `stun:`/`stuns:`. Not `turn:`/`turns:`: nothing in this codebase ever sets a
/// `username`/`credential` on an `RtcIceServer`, so an accepted TURN URL would simply fail to
/// relay instead of being rejected with a reason.
pub fn validate(url: &str) -> Result<(), StunUrlError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(StunUrlError::Empty);
    }
    if url.chars().any(char::is_whitespace) {
        return Err(StunUrlError::Whitespace);
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        return Err(StunUrlError::MissingScheme);
    };
    match scheme.to_ascii_lowercase().as_str() {
        "stun" | "stuns" => {}
        "turn" | "turns" => return Err(StunUrlError::UnsupportedScheme(scheme.to_string())),
        _ => return Err(StunUrlError::MissingScheme),
    }
    if rest.is_empty() {
        return Err(StunUrlError::MissingHost);
    }
    if rest.contains(['/', '?', '#', '@']) {
        return Err(StunUrlError::HostSyntax);
    }

    // An IPv6 literal host must be bracketed so its own colons don't get mistaken for a
    // host:port separator — the same rule ordinary URL authorities follow.
    if let Some(after_bracket) = rest.strip_prefix('[') {
        let Some((host, after)) = after_bracket.split_once(']') else {
            return Err(StunUrlError::HostSyntax);
        };
        if host.is_empty() {
            return Err(StunUrlError::MissingHost);
        }
        return match after.strip_prefix(':') {
            None if after.is_empty() => Ok(()),
            None => Err(StunUrlError::HostSyntax),
            Some(port) => validate_port(port),
        };
    }

    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (rest, None),
    };
    if host.is_empty() {
        return Err(StunUrlError::MissingHost);
    }
    match port {
        Some(port) => validate_port(port),
        None => Ok(()),
    }
}

fn validate_port(port: &str) -> Result<(), StunUrlError> {
    match port.parse::<u16>() {
        Ok(1..=u16::MAX) => Ok(()),
        _ => Err(StunUrlError::Port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_stun_and_stuns() {
        assert_eq!(validate("stun:stun.l.google.com:19302"), Ok(()));
        assert_eq!(validate("stuns:example.org"), Ok(()));
    }

    #[test]
    fn accepts_scheme_case_insensitively() {
        assert_eq!(validate("STUN:example.com"), Ok(()));
    }

    #[test]
    fn accepts_host_with_no_port() {
        assert_eq!(validate("stun:example.com"), Ok(()));
    }

    #[test]
    fn accepts_bracketed_ipv6() {
        assert_eq!(validate("stun:[2001:db8::1]:3478"), Ok(()));
        assert_eq!(validate("stun:[2001:db8::1]"), Ok(()));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(validate(""), Err(StunUrlError::Empty));
        assert_eq!(validate("   "), Err(StunUrlError::Empty));
    }

    #[test]
    fn rejects_whitespace() {
        assert_eq!(validate("stun:ho st"), Err(StunUrlError::Whitespace));
    }

    #[test]
    fn rejects_missing_scheme() {
        assert_eq!(validate("stun.l.google.com"), Err(StunUrlError::MissingScheme));
        assert_eq!(validate("hello"), Err(StunUrlError::MissingScheme));
    }

    #[test]
    fn rejects_turn_by_name() {
        assert_eq!(validate("turn:example.org"), Err(StunUrlError::UnsupportedScheme("turn".to_string())));
        assert_eq!(validate("turns:example.org"), Err(StunUrlError::UnsupportedScheme("turns".to_string())));
    }

    #[test]
    fn rejects_missing_host() {
        assert_eq!(validate("stun:"), Err(StunUrlError::MissingHost));
        assert_eq!(validate("stun:[]"), Err(StunUrlError::MissingHost));
        assert_eq!(validate("stun::3478"), Err(StunUrlError::MissingHost));
    }

    #[test]
    fn rejects_extra_url_parts() {
        assert_eq!(validate("stun:example.com/path"), Err(StunUrlError::HostSyntax));
        assert_eq!(validate("stun:example.com?query"), Err(StunUrlError::HostSyntax));
        assert_eq!(validate("stun:user@example.com"), Err(StunUrlError::HostSyntax));
    }

    #[test]
    fn rejects_bad_ports() {
        assert_eq!(validate("stun:example.com:0"), Err(StunUrlError::Port));
        assert_eq!(validate("stun:example.com:abc"), Err(StunUrlError::Port));
        assert_eq!(validate("stun:example.com:99999"), Err(StunUrlError::Port));
        assert_eq!(validate("stun:[2001:db8::1]:abc"), Err(StunUrlError::Port));
    }
}
