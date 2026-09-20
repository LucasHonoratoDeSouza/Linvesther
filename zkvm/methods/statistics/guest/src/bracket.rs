//! Certified fixed-point interval ("bracket") arithmetic at `SCALE`
//! (10^12, matching the protocol specification's fixed return scale).
//! Every `Bracket { lower, upper }` this module produces is constructed
//! to be guaranteed to contain the true rational value it approximates
//! — no `f64` anywhere, per proofs.md/metrics.md's determinism
//! requirement ("Implementação da aproximação deve ter vetores
//! independentes e não usar f64").

pub const SCALE: i128 = 1_000_000_000_000;

/// Floor of the integer square root of a non-negative `n`, via Newton's
/// method (exact, no floating point).
pub fn isqrt_floor(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bracket {
    pub lower: i128,
    pub upper: i128,
}

impl Bracket {
    pub fn exact(v: i128) -> Self {
        Bracket { lower: v, upper: v }
    }
}

/// A certified bracket for `out_scale * sqrt(num/den)` (`den > 0`,
/// `num >= 0`): floor/ceil of the exact rational are computed first,
/// then each is bracketed by the integer square root immediately below
/// and at/above it, so the true irrational value is guaranteed to fall
/// inside `[lower, upper]`.
pub fn sqrt_bracket_scaled(num: i128, den: i128, out_scale: i128) -> Option<Bracket> {
    if den <= 0 || num < 0 {
        return None;
    }
    let scaled_num = num.checked_mul(out_scale)?.checked_mul(out_scale)?;
    let floor_div = scaled_num / den;
    let ceil_div = if scaled_num % den != 0 {
        floor_div + 1
    } else {
        floor_div
    };
    let lower = isqrt_floor(floor_div);
    let mut upper = isqrt_floor(ceil_div);
    if upper.checked_mul(upper)? < ceil_div {
        upper += 1;
    }
    Some(Bracket { lower, upper })
}

/// A certified bracket for `sqrt(v/SCALE) * SCALE`, i.e. the
/// `SCALE`-re-embedded square root of an already-`SCALE`-scaled `v`.
/// Computed as `isqrt(v * SCALE)` directly (a single multiplication)
/// rather than through [`sqrt_bracket_scaled`]'s general `out_scale^2`
/// formula, which would multiply by `SCALE` twice and overflow for
/// large `v` (e.g. a bracket built from a large total-return ratio)
/// even though the final, correctly-ordered result fits easily.
pub fn sqrt_of_scaled(v: i128) -> Option<Bracket> {
    if v < 0 {
        return None;
    }
    let scaled = v.checked_mul(SCALE)?;
    let lower = isqrt_floor(scaled);
    let mut upper = lower;
    if upper.checked_mul(upper)? < scaled {
        upper += 1;
    }
    Some(Bracket { lower, upper })
}

/// The certified bracket for `sqrt` of a value already expressed as a
/// `SCALE`-scaled bracket, re-embedded at `SCALE`. Monotonic, so
/// bracketing each endpoint separately brackets the whole range.
pub fn sqrt_of_bracket(b: Bracket) -> Option<Bracket> {
    let lower = sqrt_of_scaled(b.lower)?.lower;
    let upper = sqrt_of_scaled(b.upper)?.upper;
    Some(Bracket { lower, upper })
}

/// Product of two brackets known to be non-negative throughout,
/// re-normalized back to `SCALE`. Monotonic increasing in both
/// operands when both are non-negative, so the extremes are at
/// `(lower, lower)` and `(upper, upper)`.
pub fn mul_bracket_nonneg(a: Bracket, b: Bracket) -> Option<Bracket> {
    let lower_num = a.lower.checked_mul(b.lower)?;
    let upper_num = a.upper.checked_mul(b.upper)?;
    let lower = lower_num / SCALE;
    let upper = if upper_num % SCALE != 0 {
        upper_num / SCALE + 1
    } else {
        upper_num / SCALE
    };
    Some(Bracket { lower, upper })
}

/// A certified bracket for `t * m / (n * s)`, where `t` and `s` are
/// positive brackets and `m` (which may be negative) and `n > 0` are
/// exact. `t*m/(n*s)` is monotonic in `t` and in `s` individually (in
/// directions that depend on the sign of `m`), so its extremes over the
/// box `[t.lower,t.upper] x [s.lower,s.upper]` are guaranteed to occur
/// at one of the four corners; evaluating all four with both floor and
/// ceil rounding and taking the outer min/max is therefore sound.
pub fn ratio_bracket(t: Bracket, m: i128, n: i128, s: Bracket) -> Option<Bracket> {
    let corners = [
        (t.lower, s.lower),
        (t.lower, s.upper),
        (t.upper, s.lower),
        (t.upper, s.upper),
    ];
    let mut lower = i128::MAX;
    let mut upper = i128::MIN;
    for (tc, sc) in corners {
        let num = tc.checked_mul(m)?;
        let den = n.checked_mul(sc)?;
        if den == 0 {
            return None;
        }
        let fl = num.div_euclid(den);
        let ce = if num.rem_euclid(den) != 0 { fl + 1 } else { fl };
        lower = lower.min(fl);
        upper = upper.max(ce);
    }
    Some(Bracket { lower, upper })
}
