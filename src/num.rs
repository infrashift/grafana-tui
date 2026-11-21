//! Checked numeric conversions (ADR-0002, rule R10).
//!
//! This module is the one place in non-test code that uses `as`. Each
//! float-to-integer function maps NaN to 0, clamps the value to the target
//! range and then cuts the part after the decimal point. That is the result of the `as` cast that
//! it replaces. Each integer-to-float function rounds to the nearest float, as
//! `as` does. The integer-to-integer functions saturate at the range ends.

/// Converts `value` to `u8`. NaN gives 0. The result saturates at the range ends and is a whole number.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "P10-R10: the one checked conversion point; the value is clamped and NaN is mapped first"
)]
pub(crate) fn f32_to_u8(value: f32) -> u8 {
    if value.is_nan() {
        return 0;
    }
    let clamped = value.clamp(0.0, f32::from(u8::MAX));
    debug_assert!(
        (0.0..=f32::from(u8::MAX)).contains(&clamped),
        "the clamped value is in the u8 range"
    );
    let result = clamped as u8;
    debug_assert!(
        (clamped - f32::from(result)).abs() < 1.0,
        "the conversion removes no more than the fraction"
    );
    result
}

/// Converts `value` to `u16`. NaN gives 0. The result saturates at the range ends and is a whole number.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "P10-R10: the one checked conversion point; the value is clamped and NaN is mapped first"
)]
pub(crate) fn f64_to_u16(value: f64) -> u16 {
    if value.is_nan() {
        return 0;
    }
    let clamped = value.clamp(0.0, f64::from(u16::MAX));
    debug_assert!(
        (0.0..=f64::from(u16::MAX)).contains(&clamped),
        "the clamped value is in the u16 range"
    );
    let result = clamped as u16;
    debug_assert!(
        (clamped - f64::from(result)).abs() < 1.0,
        "the conversion removes no more than the fraction"
    );
    result
}

/// Converts `value` to `u32`. NaN gives 0. The result saturates at the range ends and is a whole number.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "P10-R10: the one checked conversion point; the value is clamped and NaN is mapped first"
)]
pub(crate) fn f64_to_u32(value: f64) -> u32 {
    if value.is_nan() {
        return 0;
    }
    let clamped = value.clamp(0.0, f64::from(u32::MAX));
    debug_assert!(
        (0.0..=f64::from(u32::MAX)).contains(&clamped),
        "the clamped value is in the u32 range"
    );
    let result = clamped as u32;
    debug_assert!(
        (clamped - f64::from(result)).abs() < 1.0,
        "the conversion removes no more than the fraction"
    );
    result
}

/// Converts `value` to `u64`. NaN gives 0. The result saturates at the range ends and is a whole number.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "P10-R10: the one checked conversion point; the value is clamped and NaN is mapped first"
)]
pub(crate) fn f64_to_u64(value: f64) -> u64 {
    if value.is_nan() {
        return 0;
    }
    // `u64::MAX as f64` is 2^64. The cast of 2^64 saturates to `u64::MAX`.
    let clamped = value.clamp(0.0, u64::MAX as f64);
    debug_assert!(clamped >= 0.0, "the clamped value is not negative");
    let result = clamped as u64;
    debug_assert!(
        (clamped - u64_to_f64(result)).abs() < 1.0,
        "the conversion removes no more than the fraction"
    );
    result
}

/// Converts `value` to `usize`. NaN gives 0. The result saturates at the range ends and is a whole number.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "P10-R10: the one checked conversion point; the value is clamped and NaN is mapped first"
)]
pub(crate) fn f64_to_usize(value: f64) -> usize {
    if value.is_nan() {
        return 0;
    }
    let clamped = value.clamp(0.0, usize::MAX as f64);
    debug_assert!(clamped >= 0.0, "the clamped value is not negative");
    let result = clamped as usize;
    debug_assert!(
        (clamped - usize_to_f64(result)).abs() < 1.0,
        "the conversion removes no more than the fraction"
    );
    result
}

/// Converts `value` to `i64`. NaN gives 0. The result saturates at the range ends and is a whole number.
#[allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "P10-R10: the one checked conversion point; the value is clamped and NaN is mapped first"
)]
pub(crate) fn f64_to_i64(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }
    // `i64::MAX as f64` is 2^63. The cast of 2^63 saturates to `i64::MAX`.
    let clamped = value.clamp(i64::MIN as f64, i64::MAX as f64);
    let result = clamped as i64;
    debug_assert!(
        (clamped - i64_to_f64(result)).abs() < 1.0,
        "the conversion removes no more than the fraction"
    );
    debug_assert!(
        i64_to_f64(result).abs() <= clamped.abs(),
        "the conversion rounds toward zero"
    );
    result
}

/// Converts `value` to the nearest `f64`. Above 2^53, not every integer has an exact `f64`.
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "P10-R10: the one checked conversion point; the nearest float is the intended result"
)]
pub(crate) fn u64_to_f64(value: u64) -> f64 {
    value as f64
}

/// Converts `value` to the nearest `f64`. Above 2^53, not every integer has an exact `f64`.
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "P10-R10: the one checked conversion point; the nearest float is the intended result"
)]
pub(crate) fn usize_to_f64(value: usize) -> f64 {
    value as f64
}

/// Converts `value` to the nearest `f64`. Not every integer of more than 2^53 in size has an exact `f64`.
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "P10-R10: the one checked conversion point; the nearest float is the intended result"
)]
pub(crate) fn i64_to_f64(value: i64) -> f64 {
    value as f64
}

/// Converts `value` to the nearest `f32`. Above 2^24, not every integer has an exact `f32`.
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "P10-R10: the one checked conversion point; the nearest float is the intended result"
)]
pub(crate) fn u64_to_f32(value: u64) -> f32 {
    value as f32
}

/// Converts `value` to `u16`. A value above `u16::MAX` gives `u16::MAX`.
pub(crate) fn usize_to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// Converts `value` to `u16`. A negative value gives 0, and a value above `u16::MAX` gives `u16::MAX`.
pub(crate) fn i32_to_u16(value: i32) -> u16 {
    u16::try_from(value.max(0)).unwrap_or(u16::MAX)
}

/// Converts `value` to `u64`. A value above `u64::MAX` gives `u64::MAX`.
pub(crate) fn u128_to_u64(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// Converts `value` to `i64`. A value above `i64::MAX` gives `i64::MAX`.
pub(crate) fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Converts `value` to `u64`. A value above `u64::MAX` gives `u64::MAX`.
pub(crate) fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_to_u8_matches_as() {
        for value in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -1.5,
            0.0,
            0.9,
            254.99,
            255.0,
            300.0,
        ] {
            assert_eq!(f32_to_u8(value), value as u8, "{value}");
        }
    }

    #[test]
    fn f64_to_u16_matches_as() {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.5,
            -7.0,
            0.0,
            1.99,
            65_535.5,
            70_000.0,
        ] {
            assert_eq!(f64_to_u16(value), value as u16, "{value}");
        }
    }

    #[test]
    fn f64_to_u32_matches_as() {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -3.2,
            0.0,
            41.7,
            4_294_967_295.9,
            1e12,
        ] {
            assert_eq!(f64_to_u32(value), value as u32, "{value}");
        }
    }

    #[test]
    fn f64_to_u64_matches_as() {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -3.2,
            0.0,
            41.7,
            1.8e19,
            1e30,
        ] {
            assert_eq!(f64_to_u64(value), value as u64, "{value}");
        }
    }

    #[test]
    fn f64_to_usize_matches_as() {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -3.2,
            0.0,
            2.999,
            1e30,
        ] {
            assert_eq!(f64_to_usize(value), value as usize, "{value}");
        }
    }

    #[test]
    fn f64_to_i64_matches_as() {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1e30,
            -3.7,
            0.0,
            3.7,
            1e30,
        ] {
            assert_eq!(f64_to_i64(value), value as i64, "{value}");
        }
    }

    #[test]
    fn integer_to_float_matches_as() {
        for value in [0_u64, 1, 360, (1 << 53) + 1, u64::MAX] {
            assert_eq!(
                u64_to_f64(value).to_bits(),
                (value as f64).to_bits(),
                "{value}"
            );
            assert_eq!(
                u64_to_f32(value).to_bits(),
                (value as f32).to_bits(),
                "{value}"
            );
        }
        for value in [0_usize, 7, usize::MAX] {
            assert_eq!(
                usize_to_f64(value).to_bits(),
                (value as f64).to_bits(),
                "{value}"
            );
        }
        for value in [i64::MIN, -5, 0, 5, i64::MAX] {
            assert_eq!(
                i64_to_f64(value).to_bits(),
                (value as f64).to_bits(),
                "{value}"
            );
        }
    }

    #[test]
    fn integer_to_integer_saturates() {
        assert_eq!(usize_to_u16(0), 0);
        assert_eq!(usize_to_u16(65_535), u16::MAX);
        assert_eq!(usize_to_u16(65_536), u16::MAX);
        assert_eq!(i32_to_u16(-1), 0);
        assert_eq!(i32_to_u16(24), 24);
        assert_eq!(i32_to_u16(70_000), u16::MAX);
        assert_eq!(u128_to_u64(1_500), 1_500);
        assert_eq!(u128_to_u64(u128::MAX), u64::MAX);
        assert_eq!(u64_to_i64(86_400), 86_400);
        assert_eq!(u64_to_i64(u64::MAX), i64::MAX);
        assert_eq!(usize_to_u64(usize::MAX), u64::MAX);
    }
}
