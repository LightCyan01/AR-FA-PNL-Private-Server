use super::super::*;

#[test]
fn penetration_factor_uses_protocol_units() {
    assert_eq!(penetration_factor(0), (300_000, 300_000));
    assert_eq!(penetration_factor(10_000), (400_000, 330_000));
    assert_eq!(penetration_factor(30_000), (600_000, 390_000));
    assert_eq!(penetration_factor(-1), (300_000, 300_000));
}
