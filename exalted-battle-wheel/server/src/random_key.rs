//! A short, unambiguous random identifier — Crockford base32 (no I, L, O, or U, so it can be read
//! aloud or typed without confusion). Shared by access codes and connection ids, the two places
//! this server mints an id with nothing else (no client, no prior state) to derive it from.

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 256 is a multiple of 32, so `byte % 32` samples the alphabet with no modulo bias.
pub fn generate(length: usize) -> Result<String, getrandom::Error> {
    let mut bytes = vec![0u8; length];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| char::from(ALPHABET[usize::from(*byte) % ALPHABET.len()])).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_the_requested_length_from_the_expected_alphabet() {
        let key = generate(20).unwrap();
        assert_eq!(key.len(), 20);
        assert!(key.bytes().all(|byte| ALPHABET.contains(&byte)));
    }

    #[test]
    fn generated_keys_are_distinct() {
        assert_ne!(generate(20).unwrap(), generate(20).unwrap());
    }
}
