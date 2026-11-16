// Known-answer vectors: the three `philox4x32 10` lines of
// https://raw.githubusercontent.com/DEShawResearch/random123/main/tests/kat_vectors (fetched 2026-11-16).
use lab_rng::{below, philox4x32_10, RuleRng};

#[test]
fn kat_zero() {
    assert_eq!(philox4x32_10([0; 4], [0; 2]), [0x6627e8d5, 0xe169c58d, 0xbc57ac4c, 0x9b00dbd8]);
}
#[test]
fn kat_ones() {
    assert_eq!(philox4x32_10([u32::MAX; 4], [u32::MAX; 2]), [0x408f276d, 0x41c83b0e, 0xa20bc7c6, 0x6d5451fd]);
}
#[test]
fn kat_pi() {
    assert_eq!(
        philox4x32_10([0x243f6a88, 0x85a308d3, 0x13198a2e, 0x03707344], [0xa4093822, 0x299f31d0]),
        [0xd16cfe09, 0x94fdcceb, 0x5001e420, 0x24126ea1]
    );
}
#[test]
fn draw_layout() {
    let r = RuleRng { seed: 0x1122334455667788, stream: 7 };
    assert_eq!(
        r.draw(9, 0xAABBCCDD00112233, 5),
        philox4x32_10([9, 0x00112233, 0xAABBCCDD, (7 << 16) | 5], [0x55667788, 0x11223344])
    );
}
#[test]
fn below_bounds() {
    assert_eq!(below(0, 10), 0);
    assert_eq!(below(u32::MAX, 10), 9);
    assert_eq!(below(1 << 31, 10), 5);
}
#[test]
fn streams_differ() {
    let a = RuleRng { seed: 1, stream: 1 };
    let b = RuleRng { seed: 1, stream: 2 };
    assert_ne!(a.draw(0, 0, 0), b.draw(0, 0, 0));
}
