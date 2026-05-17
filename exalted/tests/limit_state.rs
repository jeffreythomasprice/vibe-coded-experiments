use exalted::character::{PoolState, VirtueKind};

#[test]
fn add_limit_saturates_at_break_threshold() {
    let mut s = PoolState::default();
    assert_eq!(s.limit, 0);
    assert!(!s.is_at_limit_break());
    s.add_limit(5);
    assert_eq!(s.limit, 5);
    assert!(!s.is_at_limit_break());
    // Saturating add at the threshold (10), not above.
    s.add_limit(20);
    assert_eq!(s.limit, PoolState::LIMIT_BREAK_THRESHOLD);
    assert!(s.is_at_limit_break());
    // Further additions don't push past the threshold.
    s.add_limit(1);
    assert_eq!(s.limit, PoolState::LIMIT_BREAK_THRESHOLD);
}

#[test]
fn channels_remaining_starts_at_virtue_dots() {
    let s = PoolState::default();
    assert_eq!(s.channels_remaining(VirtueKind::Compassion, 3), 3);
    assert_eq!(s.channels_remaining(VirtueKind::Valor, 0), 0);
}

#[test]
fn use_channel_decrements_remaining() {
    let mut s = PoolState::default();
    assert!(s.use_channel(VirtueKind::Compassion, 3));
    assert_eq!(s.channels_remaining(VirtueKind::Compassion, 3), 2);
    assert!(s.use_channel(VirtueKind::Compassion, 3));
    assert!(s.use_channel(VirtueKind::Compassion, 3));
    assert_eq!(s.channels_remaining(VirtueKind::Compassion, 3), 0);
    // Out of channels for the story; further calls fail.
    assert!(!s.use_channel(VirtueKind::Compassion, 3));
    assert_eq!(s.channels_remaining(VirtueKind::Compassion, 3), 0);
}

#[test]
fn other_virtue_channels_are_independent() {
    let mut s = PoolState::default();
    assert!(s.use_channel(VirtueKind::Compassion, 3));
    assert_eq!(s.channels_remaining(VirtueKind::Valor, 2), 2);
}
