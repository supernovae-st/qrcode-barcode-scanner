//! Option admission for callers whose numbers are IEEE doubles — the WASM
//! and Node bindings. One mapping of a JavaScript `number` onto the core's
//! typed options, so both surfaces accept and refuse the same values
//! (spec/02-profiles.md § Option domains). Integer surfaces (Rust, Python,
//! Kotlin/Swift, the CLI) need none of this: [`crate::Scanner`] validates
//! the typed options themselves before reading the input.

use crate::{Result, ScanConfig, ScanError};

/// The largest integer a JavaScript `number` holds exactly
/// (`Number.MAX_SAFE_INTEGER`): the ceiling of `max_pixels` read from one.
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

/// [`ScanConfig::MAX_BUDGET_MS`] as a double, exact (below 2^53).
const MAX_BUDGET: f64 = 4_294_967_295.0;

/// `budget_ms` from a JavaScript number. A value that is not above 0
/// (`0`, `-0`, a negative, `NaN`) and `+Infinity` are unbounded (`None`);
/// a positive finite value is floored to whole milliseconds, so
/// `0 < ms < 1` is a zero-millisecond budget, never an unbounded one; a
/// value whose whole part is above [`ScanConfig::MAX_BUDGET_MS`] is refused.
///
/// # Errors
/// `QRS-006` for a finite budget above the maximum.
pub fn budget_ms_from_number(ms: f64) -> Result<Option<u64>> {
    if ms.is_nan() || ms <= 0.0 || ms.is_infinite() {
        return Ok(None);
    }
    let whole = ms.floor();
    if whole > MAX_BUDGET {
        return Err(ScanError::invalid_option(
            "budget_ms",
            format!(
                "{ms} ms is above the maximum {} ms (0 = unbounded)",
                ScanConfig::MAX_BUDGET_MS
            ),
        ));
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole number in 0..=MAX_BUDGET_MS, checked above"
    )]
    Ok(Some(whole as u64))
}

/// `max_dimension` from a JavaScript number: a whole number of pixels from
/// 1 to `u32::MAX`.
///
/// # Errors
/// `QRS-006` for `NaN`, an infinity, a fraction, a value below 1 or above
/// `u32::MAX` — never a wrapped, truncated or disabled cap.
pub fn max_dimension_from_number(value: f64) -> Result<u32> {
    let whole = whole_number("max_dimension", value, u64::from(u32::MAX))?;
    u32::try_from(whole).map_err(|_| out_of_domain("max_dimension", value, u64::from(u32::MAX)))
}

/// `max_pixels` from a JavaScript number: a whole number of pixels from 1
/// to [`MAX_SAFE_INTEGER`].
///
/// # Errors
/// `QRS-006` for `NaN`, an infinity, a fraction, a value below 1 or above
/// [`MAX_SAFE_INTEGER`].
pub fn max_pixels_from_number(value: f64) -> Result<u64> {
    whole_number("max_pixels", value, MAX_SAFE_INTEGER)
}

/// A whole number from 1 to `max` (`max` is exact as a double: at most
/// [`MAX_SAFE_INTEGER`]).
fn whole_number(option: &str, value: f64, max: u64) -> Result<u64> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "max is u32::MAX or MAX_SAFE_INTEGER, both exact doubles"
    )]
    let ceiling = max as f64;
    #[expect(clippy::float_cmp, reason = "an exact whole-number test")]
    let whole = value.trunc() == value;
    if !(value.is_finite() && whole && value >= 1.0 && value <= ceiling) {
        return Err(out_of_domain(option, value, max));
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole number in 1..=max, checked above"
    )]
    Ok(value as u64)
}

fn out_of_domain(option: &str, value: f64, max: u64) -> ScanError {
    ScanError::invalid_option(
        option,
        format!("{value} is not a whole number from 1 to {max}"),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn code(result: Result<impl std::fmt::Debug>) -> &'static str {
        result.unwrap_err().code()
    }

    /// Every value a refused limit can take: never a wrapped, truncated,
    /// defaulted or disabled cap.
    const NOT_A_LIMIT: [f64; 8] = [
        0.0,
        -0.0,
        -1.0,
        0.5,
        1.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];

    #[test]
    fn the_double_ceilings_are_the_configured_ones() {
        #[expect(clippy::cast_precision_loss, reason = "exact below 2^53")]
        let max = ScanConfig::MAX_BUDGET_MS as f64;
        assert!((max - MAX_BUDGET).abs() < f64::EPSILON);
        assert_eq!(MAX_SAFE_INTEGER, 9_007_199_254_740_991);
    }

    #[test]
    fn a_budget_number_maps_to_the_documented_domain() {
        // unbounded: not above 0, NaN included, and +Infinity
        for ms in [
            0.0,
            -0.0,
            -1.0,
            -0.5,
            f64::NAN,
            f64::NEG_INFINITY,
            f64::INFINITY,
        ] {
            assert_eq!(budget_ms_from_number(ms).unwrap(), None, "{ms}");
        }
        // whole milliseconds, floored: (0, 1) is a zero-millisecond budget
        for (ms, want) in [
            (f64::MIN_POSITIVE, 0),
            (0.5, 0),
            (1.0, 1),
            (1.9, 1),
            (1500.0, 1500),
            (4_294_967_295.0, 4_294_967_295),
            (4_294_967_295.9, 4_294_967_295),
        ] {
            assert_eq!(budget_ms_from_number(ms).unwrap(), Some(want), "{ms}");
        }
        // finite and above the maximum: refused, never wrapped or unbounded
        for ms in [
            4_294_967_296.0,
            4_294_967_297.0,
            9_007_199_254_740_993.0,
            1e300,
            f64::MAX,
        ] {
            let err = budget_ms_from_number(ms).unwrap_err();
            assert_eq!(err.code(), "QRS-006", "{ms}");
            assert!(err.to_string().contains("budget_ms"), "{err}");
        }
    }

    #[test]
    fn a_dimension_number_is_a_whole_number_from_one_to_u32_max() {
        for (value, want) in [(1.0, 1), (4096.0, 4096), (4_294_967_295.0, u32::MAX)] {
            assert_eq!(max_dimension_from_number(value).unwrap(), want);
        }
        for value in NOT_A_LIMIT
            .into_iter()
            .chain([4096.7, 4_294_967_296.0, 1e300])
        {
            let err = max_dimension_from_number(value).unwrap_err();
            assert_eq!(err.code(), "QRS-006", "{value}");
            assert!(err.to_string().contains("max_dimension"), "{err}");
        }
    }

    #[test]
    fn a_pixel_number_is_a_whole_number_from_one_to_the_safe_integer() {
        for (value, want) in [
            (1.0, 1),
            (16_000_000.0, 16_000_000),
            (9_007_199_254_740_991.0, MAX_SAFE_INTEGER),
        ] {
            assert_eq!(max_pixels_from_number(value).unwrap(), want);
        }
        for value in NOT_A_LIMIT
            .into_iter()
            .chain([9_007_199_254_740_992.0, 1e300])
        {
            let err = max_pixels_from_number(value).unwrap_err();
            assert_eq!(err.code(), "QRS-006", "{value}");
            assert!(err.to_string().contains("max_pixels"), "{err}");
        }
        assert_eq!(code(max_pixels_from_number(f64::MAX)), "QRS-006");
    }
}
