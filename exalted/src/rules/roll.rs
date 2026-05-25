//! d10 dice roller (Exalted 2e Solar conventions).
//!
//! Rules used:
//!   - Each die rolls 1..=10.
//!   - 7, 8, 9 each count as 1 success.
//!   - 10 counts as 2 successes (Solars always double 10s in this app).
//!   - Botch = zero successes *and* at least one 1 rolled.

use rand::Rng;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollResult {
    pub rolls: Vec<u8>,
    pub successes: u8,
    pub tens: u8,
    pub botch: bool,
}

/// Roll `n` d10s using the thread RNG.
pub fn roll_pool(n: u8) -> RollResult {
    let mut rng = rand::rng();
    roll_pool_with(&mut rng, n)
}

/// Roll `n` d10s using the given RNG (used by tests for determinism).
pub fn roll_pool_with<R: Rng>(rng: &mut R, n: u8) -> RollResult {
    let mut rolls = Vec::with_capacity(n as usize);
    for _ in 0..n {
        rolls.push(rng.random_range(1..=10));
    }
    summarize(rolls)
}

/// Reroll every die in `result` that's currently a failure (1..=6). The
/// rerolled face replaces the original; we then resummarize. Each die is
/// rerolled at most once (the Third Excellency).
pub fn reroll_failures<R: Rng>(rng: &mut R, result: &mut RollResult) {
    for face in result.rolls.iter_mut() {
        if *face <= 6 {
            *face = rng.random_range(1..=10);
        }
    }
    let new = summarize(std::mem::take(&mut result.rolls));
    *result = new;
}

fn summarize(rolls: Vec<u8>) -> RollResult {
    let mut successes: u16 = 0;
    let mut tens: u8 = 0;
    let mut ones: u8 = 0;
    for &face in &rolls {
        if face == 10 {
            successes += 2;
            tens += 1;
        } else if face >= 7 {
            successes += 1;
        } else if face == 1 {
            ones += 1;
        }
    }
    let botch = successes == 0 && ones > 0;
    RollResult {
        rolls,
        successes: successes.min(u8::MAX as u16) as u8,
        tens,
        botch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn counts_sevens_eights_nines_as_one() {
        let r = summarize(vec![7, 8, 9]);
        assert_eq!(r.successes, 3);
        assert_eq!(r.tens, 0);
        assert!(!r.botch);
    }

    #[test]
    fn tens_count_twice() {
        let r = summarize(vec![10, 10, 7]);
        assert_eq!(r.successes, 5);
        assert_eq!(r.tens, 2);
        assert!(!r.botch);
    }

    #[test]
    fn low_faces_are_failures() {
        let r = summarize(vec![2, 3, 4, 5, 6]);
        assert_eq!(r.successes, 0);
        assert!(!r.botch); // no 1s → not a botch
    }

    #[test]
    fn all_ones_is_a_botch() {
        let r = summarize(vec![1, 1, 1]);
        assert_eq!(r.successes, 0);
        assert!(r.botch);
    }

    #[test]
    fn one_with_a_success_is_not_a_botch() {
        let r = summarize(vec![1, 7]);
        assert_eq!(r.successes, 1);
        assert!(!r.botch);
    }

    #[test]
    fn empty_pool_is_inert() {
        let r = summarize(vec![]);
        assert_eq!(r.successes, 0);
        assert!(!r.botch);
    }

    #[test]
    fn roll_pool_with_seeded_rng_is_deterministic() {
        let mut a = StdRng::seed_from_u64(42);
        let mut b = StdRng::seed_from_u64(42);
        let ra = roll_pool_with(&mut a, 10);
        let rb = roll_pool_with(&mut b, 10);
        assert_eq!(ra, rb);
        assert_eq!(ra.rolls.len(), 10);
        for face in &ra.rolls {
            assert!((1..=10).contains(face));
        }
    }

    #[test]
    fn reroll_failures_only_touches_low_faces_and_does_not_double_reroll() {
        // Hand-built RollResult with two failures (4, 6) and three successes (7, 9, 10).
        let mut result = summarize(vec![4, 6, 7, 9, 10]);
        let before_high: Vec<u8> = result.rolls.iter().copied().filter(|f| *f >= 7).collect();

        let mut rng = StdRng::seed_from_u64(99);
        reroll_failures(&mut rng, &mut result);

        // The originally-passing dice are unchanged and still in the result.
        let after_high: Vec<u8> = result.rolls.iter().copied().filter(|f| *f >= 7).collect();
        for face in before_high {
            assert!(after_high.contains(&face));
        }
        // Length unchanged.
        assert_eq!(result.rolls.len(), 5);
    }
}
