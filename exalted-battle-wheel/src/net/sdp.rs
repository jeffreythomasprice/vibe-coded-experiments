//! Pure SDP text <-> struct conversion — no networking, no browser. `parse` extracts only the
//! handful of fields a data-channel-only connection actually needs (candidates ride inline, so
//! there is no media, no codecs, no trickle plumbing to represent); `rebuild` reconstructs a
//! minimal-but-valid offer/answer from them. Everything else in a real browser-generated SDP
//! (`o=`, `s=`, `extmap-allow-mixed`, `ice-options:trickle`, candidate `generation`/`network-cost`
//! extensions, ...) is either vestigial for an ICE-driven connection or safely re-derived, so
//! `rebuild`'s output is never byte-for-byte what a browser would have produced — only equivalent
//! enough for `setRemoteDescription` to accept and for ICE/DTLS to complete over it.

use std::net::{Ipv4Addr, Ipv6Addr};

/// The DTLS role each side takes (RFC 5763): an offer always proposes `ActPass`, leaving the
/// choice to the answerer, which in every browser observed so far always answers `Active`. Stored
/// rather than re-derived from `Kind` on rebuild, since that convention is exactly that — a
/// convention, not something this code should assume will never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    ActPass,
    Active,
    Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Host,
    ServerReflexive,
    PeerReflexive,
    Relay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Address {
    V4([u8; 4]),
    V6([u8; 16]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub kind: CandidateKind,
    pub address: Address,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDescription {
    pub setup: Setup,
    pub ufrag: String,
    pub pwd: String,
    pub fingerprint: [u8; 32],
    /// Never mDNS (`*.local`): those addresses are meaningless off the machine that minted them,
    /// so `parse` drops them rather than encoding something unusable. A same-machine Chrome pair
    /// relying only on mDNS host candidates ends up with an empty list here — `parse` returns
    /// `None` rather than a description nobody could connect with; see `code.rs`'s fallback.
    pub candidates: Vec<Candidate>,
}

/// Extracts the fields above from a real offer/answer SDP, or `None` if anything required is
/// missing or unparseable. Never returns a description with zero candidates — a compact-encoded
/// blob with nothing to connect to is worse than no compact encoding at all, so the caller should
/// fall back to carrying the verbatim SDP instead.
pub fn parse(sdp: &str) -> Option<SessionDescription> {
    let mut ufrag = None;
    let mut pwd = None;
    let mut fingerprint = None;
    let mut setup = None;
    let mut candidates = Vec::new();

    for line in sdp.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("a=ice-ufrag:") {
            ufrag = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("a=ice-pwd:") {
            pwd = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("a=fingerprint:") {
            let (algorithm, hex) = rest.split_once(' ')?;
            if algorithm == "sha-256" {
                fingerprint = parse_fingerprint(hex);
            }
            // An unsupported algorithm just leaves `fingerprint` unset, failing the whole parse
            // below rather than silently proceeding with the wrong hash width.
        } else if let Some(rest) = line.strip_prefix("a=setup:") {
            setup = match rest {
                "actpass" => Some(Setup::ActPass),
                "active" => Some(Setup::Active),
                "passive" => Some(Setup::Passive),
                _ => None,
            };
        } else if let Some(rest) = line.strip_prefix("a=candidate:")
            && let Some(candidate) = parse_candidate(rest)
        {
            candidates.push(candidate);
        }
    }

    if candidates.is_empty() {
        return None;
    }

    Some(SessionDescription { setup: setup?, ufrag: ufrag?, pwd: pwd?, fingerprint: fingerprint?, candidates })
}

/// `rest` is everything after `a=candidate:`, e.g. `3083668256 1 udp 2113937151 203.0.113.7 40891
/// typ srflx raddr 0.0.0.0 rport 0 generation 0 network-cost 999`. Only the fields this app ever
/// needs are read positionally; foundation, component, priority, and every trailing extension
/// (`generation`, `network-cost`, `raddr`/`rport`) are ignored rather than validated.
fn parse_candidate(rest: &str) -> Option<Candidate> {
    let mut fields = rest.split_whitespace();
    let _foundation = fields.next()?;
    let _component = fields.next()?;
    let transport = fields.next()?;
    if !transport.eq_ignore_ascii_case("udp") {
        return None;
    }
    let _priority = fields.next()?;
    let address = parse_address(fields.next()?)?;
    let port = fields.next()?.parse().ok()?;
    if fields.next()? != "typ" {
        return None;
    }
    let kind = match fields.next()? {
        "host" => CandidateKind::Host,
        "srflx" => CandidateKind::ServerReflexive,
        "prflx" => CandidateKind::PeerReflexive,
        "relay" => CandidateKind::Relay,
        _ => return None,
    };
    Some(Candidate { kind, address, port })
}

fn parse_address(text: &str) -> Option<Address> {
    if text.ends_with(".local") {
        return None;
    }
    if let Ok(v4) = text.parse::<Ipv4Addr>() {
        return Some(Address::V4(v4.octets()));
    }
    if let Ok(v6) = text.parse::<Ipv6Addr>() {
        return Some(Address::V6(v6.octets()));
    }
    None
}

fn parse_fingerprint(hex: &str) -> Option<[u8; 32]> {
    let mut bytes = [0u8; 32];
    let mut count = 0;
    for (index, part) in hex.split(':').enumerate() {
        if index >= 32 {
            return None;
        }
        bytes[index] = u8::from_str_radix(part, 16).ok()?;
        count += 1;
    }
    (count == 32).then_some(bytes)
}

/// Reconstructs a minimal offer/answer SDP a browser will accept for `setRemoteDescription`. Only
/// the *remote* description is ever built this way — the local one is always the browser's own
/// unmodified `createOffer`/`createAnswer` output, so this side's own ICE agent never has to be
/// consistent with a value it didn't generate itself.
pub fn rebuild(desc: &SessionDescription) -> String {
    let mut sdp = String::new();
    sdp.push_str("v=0\r\n");
    sdp.push_str("o=- 0 0 IN IP4 127.0.0.1\r\n");
    sdp.push_str("s=-\r\n");
    sdp.push_str("t=0 0\r\n");
    sdp.push_str("a=group:BUNDLE 0\r\n");
    sdp.push_str("a=msid-semantic: WMS\r\n");
    sdp.push_str("m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n");
    sdp.push_str("c=IN IP4 0.0.0.0\r\n");
    for (index, candidate) in desc.candidates.iter().enumerate() {
        sdp.push_str(&format_candidate(index, candidate));
    }
    sdp.push_str(&format!("a=ice-ufrag:{}\r\n", desc.ufrag));
    sdp.push_str(&format!("a=ice-pwd:{}\r\n", desc.pwd));
    sdp.push_str(&format!("a=fingerprint:sha-256 {}\r\n", format_fingerprint(&desc.fingerprint)));
    let setup = match desc.setup {
        Setup::ActPass => "actpass",
        Setup::Active => "active",
        Setup::Passive => "passive",
    };
    sdp.push_str(&format!("a=setup:{setup}\r\n"));
    sdp.push_str("a=mid:0\r\n");
    sdp.push_str("a=sctp-port:5000\r\n");
    sdp.push_str("a=max-message-size:262144\r\n");
    sdp
}

/// Priority follows RFC 8445's recommended formula (type preference << 24 | local preference << 8
/// | (256 - component id)), with component id fixed at 1 (there is only ever one, for the single
/// data channel) and local preference just decreasing by list position — there is no real
/// multi-interface preference to express, only "try these in the order given".
fn format_candidate(index: usize, candidate: &Candidate) -> String {
    let type_preference: u32 = match candidate.kind {
        CandidateKind::Host => 126,
        CandidateKind::PeerReflexive => 110,
        CandidateKind::ServerReflexive => 100,
        CandidateKind::Relay => 0,
    };
    let local_preference = 65535u32.saturating_sub(index as u32);
    let priority = (type_preference << 24) | (local_preference << 8) | 255;
    let type_str = match candidate.kind {
        CandidateKind::Host => "host",
        CandidateKind::ServerReflexive => "srflx",
        CandidateKind::PeerReflexive => "prflx",
        CandidateKind::Relay => "relay",
    };
    // A real "related address" only means something for a candidate derived from another one
    // (srflx/prflx/relay); host candidates never carry raddr/rport, and including them anyway is
    // exactly the kind of deviation from what browsers actually emit that risks a picky parser.
    let related = match candidate.kind {
        CandidateKind::Host => String::new(),
        _ => " raddr 0.0.0.0 rport 0".to_string(),
    };
    let address = format_address(&candidate.address);
    format!("a=candidate:{} 1 udp {priority} {address} {} typ {type_str}{related}\r\n", index + 1, candidate.port)
}

fn format_address(address: &Address) -> String {
    match address {
        Address::V4(octets) => Ipv4Addr::from(*octets).to_string(),
        Address::V6(octets) => Ipv6Addr::from(*octets).to_string(),
    }
}

fn format_fingerprint(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured verbatim from a real Chrome instance: `new RTCPeerConnection(...)`,
    // `createDataChannel`, `createOffer`, `setLocalDescription`, then read back
    // `pc.localDescription.sdp` once ICE gathering completed. The host candidate's mDNS address
    // and the srflx candidate's real one are both exactly what Chrome produced, unedited.
    const REAL_OFFER: &str = "v=0\r\n\
o=- 522101713798750881 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0\r\n\
a=extmap-allow-mixed\r\n\
a=msid-semantic: WMS\r\n\
m=application 40891 UDP/DTLS/SCTP webrtc-datachannel\r\n\
c=IN IP4 35.136.2.216\r\n\
a=candidate:3083668256 1 udp 2113937151 00f87de4-7677-4aa6-8a7c-f4a88b679991.local 40891 typ host generation 0 network-cost 999\r\n\
a=candidate:3094459396 1 udp 1677729535 35.136.2.216 40891 typ srflx raddr 0.0.0.0 rport 0 generation 0 network-cost 999\r\n\
a=ice-ufrag:OncY\r\n\
a=ice-pwd:Czm9eYLVyrCTZyazGfdjQezV\r\n\
a=ice-options:trickle\r\n\
a=fingerprint:sha-256 78:32:D2:17:47:E5:7C:AE:ED:65:8A:A5:2B:53:12:21:04:99:02:A2:BF:AB:05:5E:A5:B7:62:08:34:EC:C8:E7\r\n\
a=setup:actpass\r\n\
a=mid:0\r\n\
a=sctp-port:5000\r\n\
a=max-message-size:262144\r\n";

    // Same session, the answering side: `setRemoteDescription(offer)`, `createAnswer`,
    // `setLocalDescription`. This one never got a srflx candidate before gathering finished (a
    // sandboxed test network), leaving only the mDNS host candidate — exactly the "nothing usable"
    // case `parse` must reject rather than encode.
    const REAL_ANSWER_MDNS_ONLY: &str = "v=0\r\n\
o=- 3446603954115223089 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0\r\n\
a=extmap-allow-mixed\r\n\
a=msid-semantic: WMS\r\n\
m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n\
c=IN IP4 0.0.0.0\r\n\
a=candidate:2430428655 1 udp 2113937151 00f87de4-7677-4aa6-8a7c-f4a88b679991.local 35529 typ host generation 0 network-cost 999\r\n\
a=ice-ufrag:Lmcs\r\n\
a=ice-pwd:E259CWQbCd/vw4zStWrNEOef\r\n\
a=ice-options:trickle\r\n\
a=fingerprint:sha-256 57:40:3E:2A:63:60:B7:AA:77:DD:F0:64:9D:F9:81:E5:E6:CB:64:57:03:9C:13:D8:F3:07:BD:BC:BA:14:35:BB\r\n\
a=setup:active\r\n\
a=mid:0\r\n\
a=sctp-port:5000\r\n\
a=max-message-size:262144\r\n";

    #[test]
    fn parses_every_field_from_a_real_offer() {
        let desc = parse(REAL_OFFER).expect("should parse");
        assert_eq!(desc.ufrag, "OncY");
        assert_eq!(desc.pwd, "Czm9eYLVyrCTZyazGfdjQezV");
        assert_eq!(desc.setup, Setup::ActPass);
        assert_eq!(
            desc.fingerprint,
            [
                0x78, 0x32, 0xD2, 0x17, 0x47, 0xE5, 0x7C, 0xAE, 0xED, 0x65, 0x8A, 0xA5, 0x2B, 0x53, 0x12, 0x21, 0x04, 0x99, 0x02, 0xA2, 0xBF, 0xAB, 0x05,
                0x5E, 0xA5, 0xB7, 0x62, 0x08, 0x34, 0xEC, 0xC8, 0xE7
            ]
        );
    }

    #[test]
    fn drops_the_mdns_host_candidate_and_keeps_the_srflx_one() {
        let desc = parse(REAL_OFFER).expect("should parse");
        assert_eq!(desc.candidates.len(), 1);
        assert_eq!(desc.candidates[0].kind, CandidateKind::ServerReflexive);
        assert_eq!(desc.candidates[0].address, Address::V4([35, 136, 2, 216]));
        assert_eq!(desc.candidates[0].port, 40891);
    }

    #[test]
    fn rejects_an_sdp_with_no_usable_candidates() {
        assert!(parse(REAL_ANSWER_MDNS_ONLY).is_none());
    }

    #[test]
    fn round_trips_through_rebuild_and_reparse() {
        let original = parse(REAL_OFFER).expect("should parse");
        let rebuilt_text = rebuild(&original);
        let reparsed = parse(&rebuilt_text).expect("rebuilt sdp should itself parse");
        assert_eq!(reparsed, original);
    }

    #[test]
    fn rebuild_omits_raddr_for_host_candidates_but_includes_it_for_srflx() {
        let host_only = SessionDescription {
            setup: Setup::ActPass,
            ufrag: "abcd".to_string(),
            pwd: "0123456789012345678901".to_string(),
            fingerprint: [0u8; 32],
            candidates: vec![Candidate { kind: CandidateKind::Host, address: Address::V4([10, 0, 0, 5]), port: 1234 }],
        };
        let text = rebuild(&host_only);
        let host_line = text.lines().find(|line| line.starts_with("a=candidate:")).unwrap();
        assert!(!host_line.contains("raddr"));

        let srflx = SessionDescription { candidates: vec![Candidate { kind: CandidateKind::ServerReflexive, ..host_only.candidates[0] }], ..host_only };
        let text = rebuild(&srflx);
        let srflx_line = text.lines().find(|line| line.starts_with("a=candidate:")).unwrap();
        assert!(srflx_line.contains("raddr 0.0.0.0 rport 0"));
    }

    #[test]
    fn fingerprint_round_trips_case_insensitively() {
        let lower = "78:32:d2:17:47:e5:7c:ae:ed:65:8a:a5:2b:53:12:21:04:99:02:a2:bf:ab:05:5e:a5:b7:62:08:34:ec:c8:e7";
        assert_eq!(parse_fingerprint(lower), parse_fingerprint(&lower.to_uppercase()));
    }
}
