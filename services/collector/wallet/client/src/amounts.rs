//! Amounts on a chain are whole numbers of the smallest unit. They are turned into exact
//! decimals of the asset's own unit; one too large to represent exactly is refused, not rounded.

use rust_decimal::Decimal;

/// Wei in one ether, and the decimals of every native asset here.
pub const NATIVE_DECIMALS: u32 = 18;

/// `raw` (an integer string) as a decimal shifted by `decimals` places, or `None` if it is not
/// a whole number or does not fit exactly.
pub fn units(raw: &str, decimals: u32) -> Option<Decimal> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = raw.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let text = if decimals == 0 {
        digits.to_string()
    } else if digits.len() as u32 <= decimals {
        format!("0.{}{}", "0".repeat((decimals as usize) - digits.len()), digits)
    } else {
        let split = digits.len() - decimals as usize;
        format!("{}.{}", &digits[..split], &digits[split..])
    };
    Decimal::from_str_exact(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raw_amount_moves_its_decimal_point_exactly() {
        assert_eq!(units("1000000000000000000", 18).unwrap().to_string(), "1.000000000000000000");
        assert_eq!(units("1500000", 6).unwrap().to_string(), "1.500000");
        assert_eq!(units("1", 18).unwrap().to_string(), "0.000000000000000001");
        assert_eq!(units("0", 18).unwrap().to_string(), "0.000000000000000000");
        assert_eq!(units("42", 0).unwrap().to_string(), "42");
    }

    #[test]
    fn what_is_not_a_whole_number_or_does_not_fit_is_refused() {
        assert!(units("", 18).is_none());
        assert!(units("1.5", 18).is_none());
        assert!(units("-1", 18).is_none());
        // More digits than a decimal holds exactly.
        assert!(units(&"9".repeat(40), 18).is_none());
    }
}
