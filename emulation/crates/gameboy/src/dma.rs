//! The Game Boy (DMG) OAM DMA controller (`0xFF46`).
//!
//! Writing a page byte `XX` to `0xFF46` starts a transfer that copies the 160
//! bytes `XX00-XX9F` into OAM (`0xFE00-0xFE9F`) over **160 M-cycles**
//! (640 T-cycles). During the transfer the CPU can only reach HRAM — which is
//! why games run the DMA kick routine from HRAM and use OAM DMA (usually in
//! V-Blank) as the normal way to update sprites.
//!
//! [`OamDma`] mirrors the [`Timer`](crate::timer::Timer) pattern: a
//! self-contained subsystem that owns **only** the timing/progress state and
//! reports what to do back to its owner
//! ([`SystemBus`](crate::memory::SystemBus)) rather than reaching across the
//! bus itself. It cannot perform the copy — that needs bus reads of the source
//! region plus OAM writes, both of which only `SystemBus` can orchestrate — so
//! [`OamDma::advance`] just reports the range of byte indices that came due and
//! lets the owner do the copying.

/// How long a full transfer takes, in T-cycles: 160 bytes × 4 T-cycles
/// (one M-cycle) per byte.
const TRANSFER_TCYCLES: u32 = OAM_BYTES as u32 * 4;

/// Number of bytes copied by a transfer (the size of OAM).
const OAM_BYTES: u8 = 0xA0;

/// The DMG OAM DMA controller. See the module docs for the transfer model.
#[derive(Debug, Clone, Default)]
pub struct OamDma {
    /// The last value written to `0xFF46`. Doubles as the register readback and
    /// the high byte of the transfer's source address.
    source_page: u8,
    /// Whether a transfer is currently in progress.
    active: bool,
    /// T-cycles elapsed since the current transfer began.
    elapsed: u32,
    /// How many bytes have been copied so far this transfer.
    bytes_copied: u8,
}

impl OamDma {
    /// A powered-on controller: idle, source page `0x00`.
    pub fn new() -> OamDma {
        OamDma::default()
    }

    /// Start (or restart) a transfer sourced from page `page` — a write to
    /// `0xFF46`. A write during an in-flight transfer restarts it from the top.
    pub fn trigger(&mut self, page: u8) {
        tracing::trace!(page = format_args!("{page:#04x}"), "oam dma triggered");
        self.source_page = page;
        self.active = true;
        self.elapsed = 0;
        self.bytes_copied = 0;
    }

    /// The last value written to `0xFF46`, for the register readback.
    pub fn source_page(&self) -> u8 {
        self.source_page
    }

    /// Whether a transfer is in progress (the bus-blocking gate keys off this).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Advance the transfer by `cycles` T-cycles and return the half-open range
    /// `[start, end)` of byte indices that became due to copy during this span.
    /// The transfer deactivates once all [`OAM_BYTES`] have been copied.
    ///
    /// Returning owned indices (rather than borrowing the source/OAM) lets the
    /// owner drop its `&mut OamDma` borrow before it reaches back into the bus to
    /// perform the copy.
    pub fn advance(&mut self, cycles: u32) -> (u8, u8) {
        let start = self.bytes_copied;
        self.elapsed = self.elapsed.saturating_add(cycles);
        // One byte per M-cycle (4 T-cycles), capped at the OAM size.
        let target = (self.elapsed / 4).min(OAM_BYTES as u32) as u8;
        self.bytes_copied = target;
        if self.elapsed >= TRANSFER_TCYCLES {
            self.active = false;
        }
        (start, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_activates_and_records_page() {
        let mut dma = OamDma::new();
        assert!(!dma.is_active());
        dma.trigger(0xC0);
        assert!(dma.is_active());
        assert_eq!(dma.source_page(), 0xC0);
    }

    #[test]
    fn advance_copies_one_byte_per_four_tcycles() {
        let mut dma = OamDma::new();
        dma.trigger(0xC0);
        assert_eq!(dma.advance(4), (0, 1));
        assert_eq!(dma.advance(4), (1, 2));
        assert!(dma.is_active());
    }

    #[test]
    fn advance_completes_after_640_tcycles() {
        let mut dma = OamDma::new();
        dma.trigger(0xC0);
        assert_eq!(dma.advance(640), (0, 0xA0));
        assert!(!dma.is_active());
        // A further advance yields nothing and stays inactive.
        assert_eq!(dma.advance(4), (0xA0, 0xA0));
        assert!(!dma.is_active());
    }

    #[test]
    fn advance_clamps_partial_tcycles_within_a_byte() {
        let mut dma = OamDma::new();
        dma.trigger(0xC0);
        // 3 T-cycles is not yet a whole byte.
        assert_eq!(dma.advance(3), (0, 0));
        // Crossing into the 4th T-cycle makes byte 0 due.
        assert_eq!(dma.advance(1), (0, 1));
    }

    #[test]
    fn retrigger_restarts_from_the_top() {
        let mut dma = OamDma::new();
        dma.trigger(0xC0);
        dma.advance(16); // 4 bytes in
        dma.trigger(0xD0);
        assert_eq!(dma.source_page(), 0xD0);
        assert_eq!(dma.advance(4), (0, 1)); // back at byte 0
    }
}
