use crate::net::error::SignalError;
use crate::net::sdp::{self, Address, Candidate, CandidateKind, SessionDescription, Setup};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

/// The verbatim SDP text, tagged so `decode` can tell it apart from the compact format below.
/// Long (a raw SDP is 1.5-3KB) — fine for a textarea paste, too dense for a comfortable link or
/// QR code — but it preserves every candidate a browser offers, including the `.local` mDNS ones
/// Chrome emits for same-machine peers, which the compact format below cannot carry at all. Kept
/// permanently, not just as a stepping stone: it is the only format that lets two tabs on one
/// machine connect reliably, since mDNS host candidates are often the only ones they share.
const TAG_VERBATIM: u8 = 0x00;

/// The candidate-stripped, binary-packed encoding: everything after the tag byte is
/// `compact_encode`'s output. Short enough for a comfortable link or a scannable QR, at the cost
/// of dropping mDNS candidates entirely — see `sdp::parse`.
const TAG_COMPACT: u8 = 0x01;

/// Wraps a session description for manual copy/paste. Tries the compact encoding first — parsing
/// `sdp_text`, and packing what it finds into a tight binary layout — and falls back to carrying
/// the verbatim text whenever that isn't possible (no usable non-mDNS candidate, or a field too
/// long to fit the format's `u8` length prefixes). The two formats are indistinguishable to a
/// caller: `decode` reads the tag byte and reverses whichever one was actually used.
pub fn encode(sdp_text: &str) -> String {
    if let Some(description) = sdp::parse(sdp_text)
        && let Some(packed) = compact_encode(&description)
    {
        let mut framed = Vec::with_capacity(packed.len() + 1);
        framed.push(TAG_COMPACT);
        framed.extend(packed);
        return URL_SAFE_NO_PAD.encode(framed);
    }
    let mut framed = Vec::with_capacity(sdp_text.len() + 1);
    framed.push(TAG_VERBATIM);
    framed.extend_from_slice(sdp_text.as_bytes());
    URL_SAFE_NO_PAD.encode(framed)
}

pub fn decode(code: &str) -> Result<String, SignalError> {
    let bytes = URL_SAFE_NO_PAD.decode(code.trim()).map_err(|_| SignalError::NotBase64)?;
    let (&tag, rest) = bytes.split_first().ok_or(SignalError::Corrupted)?;
    match tag {
        TAG_VERBATIM => String::from_utf8(rest.to_vec()).map_err(|_| SignalError::NotUtf8),
        TAG_COMPACT => compact_decode(rest).map(|description| sdp::rebuild(&description)),
        _ => Err(SignalError::Corrupted),
    }
}

/// `None` rather than an error: every failure here (a field too long for a `u8` length prefix) is
/// something `encode` just falls back from, never something a caller needs to know the reason
/// for. `ufrag`/`pwd` are ordinary browser-generated ICE credentials, comfortably short in
/// practice — this only ever matters as a safety net, not a real limit anyone hits.
fn compact_encode(description: &SessionDescription) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.push(match description.setup {
        Setup::ActPass => 0,
        Setup::Active => 1,
        Setup::Passive => 2,
    });
    let ufrag = description.ufrag.as_bytes();
    let pwd = description.pwd.as_bytes();
    bytes.push(u8::try_from(ufrag.len()).ok()?);
    bytes.extend_from_slice(ufrag);
    bytes.push(u8::try_from(pwd.len()).ok()?);
    bytes.extend_from_slice(pwd);
    bytes.extend_from_slice(&description.fingerprint);
    bytes.push(u8::try_from(description.candidates.len()).ok()?);
    for candidate in &description.candidates {
        let type_bits: u8 = match candidate.kind {
            CandidateKind::Host => 0,
            CandidateKind::ServerReflexive => 1,
            CandidateKind::PeerReflexive => 2,
            CandidateKind::Relay => 3,
        };
        let (family_bit, address_bytes): (u8, &[u8]) = match &candidate.address {
            Address::V4(octets) => (0, octets),
            Address::V6(octets) => (0b100, octets),
        };
        bytes.push(type_bits | family_bit);
        bytes.extend_from_slice(address_bytes);
        bytes.extend_from_slice(&candidate.port.to_be_bytes());
    }
    Some(bytes)
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], SignalError> {
    let slice = bytes.get(*cursor..*cursor + len).ok_or(SignalError::Corrupted)?;
    *cursor += len;
    Ok(slice)
}

fn take_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, SignalError> {
    Ok(take(bytes, cursor, 1)?[0])
}

fn compact_decode(bytes: &[u8]) -> Result<SessionDescription, SignalError> {
    let mut cursor = 0usize;
    let setup = match take_u8(bytes, &mut cursor)? {
        0 => Setup::ActPass,
        1 => Setup::Active,
        2 => Setup::Passive,
        _ => return Err(SignalError::Corrupted),
    };
    let ufrag_len = take_u8(bytes, &mut cursor)? as usize;
    let ufrag = String::from_utf8(take(bytes, &mut cursor, ufrag_len)?.to_vec()).map_err(|_| SignalError::Corrupted)?;
    let pwd_len = take_u8(bytes, &mut cursor)? as usize;
    let pwd = String::from_utf8(take(bytes, &mut cursor, pwd_len)?.to_vec()).map_err(|_| SignalError::Corrupted)?;
    let fingerprint: [u8; 32] = take(bytes, &mut cursor, 32)?.try_into().map_err(|_| SignalError::Corrupted)?;

    let candidate_count = take_u8(bytes, &mut cursor)? as usize;
    let mut candidates = Vec::with_capacity(candidate_count);
    for _ in 0..candidate_count {
        let type_and_family = take_u8(bytes, &mut cursor)?;
        let kind = match type_and_family & 0b011 {
            0 => CandidateKind::Host,
            1 => CandidateKind::ServerReflexive,
            2 => CandidateKind::PeerReflexive,
            3 => CandidateKind::Relay,
            _ => unreachable!("masked to two bits"),
        };
        let address = if type_and_family & 0b100 == 0 {
            Address::V4(take(bytes, &mut cursor, 4)?.try_into().map_err(|_| SignalError::Corrupted)?)
        } else {
            Address::V6(take(bytes, &mut cursor, 16)?.try_into().map_err(|_| SignalError::Corrupted)?)
        };
        let port = u16::from_be_bytes(take(bytes, &mut cursor, 2)?.try_into().expect("take(_, 2) always returns 2 bytes"));
        candidates.push(Candidate { kind, address, port });
    }
    if candidates.is_empty() {
        return Err(SignalError::Corrupted);
    }
    Ok(SessionDescription { setup, ufrag, pwd, fingerprint, candidates })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Same real Chrome capture `sdp.rs`'s tests use — see there for how it was captured.
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

    // No srflx candidate at all — only the mDNS host one. `encode` must fall back to verbatim.
    const MDNS_ONLY: &str = "v=0\r\n\
o=- 1 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n\
c=IN IP4 0.0.0.0\r\n\
a=candidate:1 1 udp 2113937151 abc.local 12345 typ host generation 0\r\n\
a=ice-ufrag:aaaa\r\n\
a=ice-pwd:bbbbbbbbbbbbbbbbbbbbbb\r\n\
a=fingerprint:sha-256 78:32:D2:17:47:E5:7C:AE:ED:65:8A:A5:2B:53:12:21:04:99:02:A2:BF:AB:05:5E:A5:B7:62:08:34:EC:C8:E7\r\n\
a=setup:actpass\r\n\
a=mid:0\r\n";

    #[test]
    fn compact_round_trips_a_real_offer() {
        let code = encode(REAL_OFFER);
        let decoded = decode(&code).unwrap();
        // Not byte-identical to the input (see `sdp::rebuild`'s doc) — but re-parsing it
        // recovers exactly the fields that matter, which is the only thing a real peer checks.
        assert_eq!(sdp::parse(&decoded).unwrap(), sdp::parse(REAL_OFFER).unwrap());
    }

    #[test]
    fn compact_encoding_is_much_shorter_than_verbatim() {
        let compact = encode(REAL_OFFER);
        let raw_base64_len = (REAL_OFFER.len() + 1).div_ceil(3) * 4;
        assert!(compact.len() < raw_base64_len / 2, "compact ({}) should be well under half of verbatim ({raw_base64_len})", compact.len());
    }

    #[test]
    fn falls_back_to_verbatim_when_there_is_nothing_usable_to_compact() {
        let code = encode(MDNS_ONLY);
        assert_eq!(decode(&code).unwrap(), MDNS_ONLY);
    }

    #[test]
    fn rejects_non_base64() {
        assert_eq!(decode("not!!valid=base64"), Err(SignalError::NotBase64));
    }

    #[test]
    fn rejects_a_truncated_compact_payload() {
        let mut bytes = vec![TAG_COMPACT, 0, 4]; // claims a 4-byte ufrag that isn't there
        bytes.push(b'a');
        assert_eq!(decode(&URL_SAFE_NO_PAD.encode(&bytes)), Err(SignalError::Corrupted));
    }

    #[test]
    fn trims_incidental_whitespace_from_a_pasted_code() {
        let code = format!("  {}  \n", encode(MDNS_ONLY));
        assert_eq!(decode(&code).unwrap(), MDNS_ONLY);
    }
}
