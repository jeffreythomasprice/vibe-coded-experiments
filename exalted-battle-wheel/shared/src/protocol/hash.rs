use serde::Serialize;
use std::fmt;

/// A cheap fingerprint of an app's whole state, used in the two-phase-commit vote to check that
/// every node would produce (or did produce) the same result before anyone commits. FNV-1a rather
/// than `DefaultHasher`: the standard library explicitly does not guarantee `DefaultHasher`'s
/// algorithm is stable across compiler releases, and two peers in the same room may be running
/// different cached wasm builds.
///
/// This only agrees across nodes if `serde_json::to_vec` is byte-identical for equal values,
/// which holds only as long as the hashed type's serialized graph contains no `HashMap`/`HashSet`
/// (unordered) and no floats (`NaN`/signed-zero/precision quirks). Adding either to a type that
/// gets hashed would make otherwise-healthy rooms disconnect at random.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StateHash(pub u64);

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Hashes `value`'s JSON encoding. Panics if `value` cannot be encoded — every type this is
/// called with is already required to be `Serialize` for the wire protocol, so a failure here
/// would mean a logic error (e.g. a non-string map key), not bad input.
pub fn hash_of<T: Serialize>(value: &T) -> StateHash {
    let bytes = serde_json::to_vec(value).expect("hashed values must be encodable");
    StateHash(fnv1a(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_values_hash_the_same() {
        assert_eq!(hash_of(&vec![1, 2, 3]), hash_of(&vec![1, 2, 3]));
    }

    #[test]
    fn different_values_hash_differently() {
        assert_ne!(hash_of(&vec![1, 2, 3]), hash_of(&vec![1, 2, 4]));
    }

    #[test]
    fn displays_as_fixed_width_hex() {
        assert_eq!(StateHash(0xabc).to_string(), "0000000000000abc");
    }
}
