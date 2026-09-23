//! Which of the assets an address holds can be followed honestly. An asset the address only ever
//! received and that has no price is noise (an unsolicited token) and is left out. One it sent or
//! swapped, or that cannot be represented, matters to the result, and without it the result would
//! be wrong, so the figures are refused instead.

use std::collections::BTreeSet;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoverageError {
    #[error("no price for {} which the wallet moved: {}", if .0.len() == 1 { "a token" } else { "tokens" }, .0.join(", "))]
    Unpriced(Vec<String>),
    #[error("an amount of {} is too large to follow exactly", .0.join(", "))]
    Unrepresentable(Vec<String>),
}

/// The followed assets left out because they are unpriced and were only ever received, or an
/// error naming what makes the figures unavailable.
pub fn check_coverage(followed: &BTreeSet<String>, active: &BTreeSet<String>, priced: &BTreeSet<String>, unrepresentable: &BTreeSet<String>) -> Result<Vec<String>, CoverageError> {
    let mut excluded = Vec::new();
    let mut unpriced_active = Vec::new();
    let mut too_large = Vec::new();
    for asset in followed {
        let is_priced = priced.contains(asset);
        if is_priced && unrepresentable.contains(asset) {
            too_large.push(asset.clone());
        } else if !is_priced {
            if active.contains(asset) {
                unpriced_active.push(asset.clone());
            } else {
                excluded.push(asset.clone());
            }
        }
    }
    if !too_large.is_empty() {
        return Err(CoverageError::Unrepresentable(too_large));
    }
    if !unpriced_active.is_empty() {
        return Err(CoverageError::Unpriced(unpriced_active));
    }
    Ok(excluded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_unpriced_token_that_was_only_received_is_left_out_and_named() {
        let excluded = check_coverage(&set(&["e:native", "e:0xspam", "e:0xusdc"]), &set(&["e:native"]), &set(&["e:native", "e:0xusdc"]), &set(&[])).unwrap();
        assert_eq!(excluded, vec!["e:0xspam"]);
    }

    #[test]
    fn an_unpriced_token_the_wallet_moved_makes_the_figures_unavailable() {
        let error = check_coverage(&set(&["e:native", "e:0xmoved"]), &set(&["e:0xmoved"]), &set(&["e:native"]), &set(&[])).unwrap_err();
        assert_eq!(error, CoverageError::Unpriced(vec!["e:0xmoved".into()]));
        assert!(error.to_string().contains("e:0xmoved"));
    }

    #[test]
    fn a_priced_asset_whose_amount_cannot_be_held_makes_them_unavailable_but_an_unpriced_one_is_just_noise() {
        assert_eq!(check_coverage(&set(&["e:0xa"]), &set(&[]), &set(&["e:0xa"]), &set(&["e:0xa"])), Err(CoverageError::Unrepresentable(vec!["e:0xa".into()])));
        assert_eq!(check_coverage(&set(&["e:0xb"]), &set(&[]), &set(&[]), &set(&["e:0xb"])).unwrap(), vec!["e:0xb"]);
    }

    #[test]
    fn everything_priced_is_followed_and_nothing_is_left_out() {
        assert!(check_coverage(&set(&["e:native"]), &set(&["e:native"]), &set(&["e:native"]), &set(&[])).unwrap().is_empty());
    }
}
