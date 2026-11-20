//! Decimal amount strings from balance documents.
//!
//! The capture shows `purchasedBalance` and `grantedBalance` as decimal
//! strings, and the gold the client displayed matched their sum. Amounts stay
//! strings on the wire; this helper only adds them.

use std::fmt;

/// `purchasedBalance + grantedBalance`, using the scale of the wider input.
pub fn displayed_balance(purchased: &str, granted: &str) -> Result<String, DecimalError> {
    let left = parse_amount(purchased)?;
    let right = parse_amount(granted)?;
    let scale = left.scale.max(right.scale);
    let left = left.align(scale)?;
    let right = right.align(scale)?;
    let sum = left
        .unscaled
        .checked_add(right.unscaled)
        .ok_or(DecimalError)?;
    Ok(format_amount(sum, scale))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalError;

impl fmt::Display for DecimalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not a decimal amount")
    }
}

impl std::error::Error for DecimalError {}

struct Amount {
    unscaled: u128,
    scale: u32,
}

impl Amount {
    fn align(self, scale: u32) -> Result<Self, DecimalError> {
        let mut unscaled = self.unscaled;
        let mut current = self.scale;
        while current < scale {
            unscaled = unscaled.checked_mul(10).ok_or(DecimalError)?;
            current += 1;
        }
        Ok(Self { unscaled, scale })
    }
}

fn parse_amount(raw: &str) -> Result<Amount, DecimalError> {
    if raw.is_empty() || raw.starts_with('+') || raw.starts_with('-') {
        return Err(DecimalError);
    }
    let (whole, fraction) = match raw.split_once('.') {
        Some((whole, fraction)) => {
            if whole.is_empty() || fraction.is_empty() || fraction.contains('.') {
                return Err(DecimalError);
            }
            (whole, fraction)
        }
        None => (raw, ""),
    };
    if !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(DecimalError);
    }
    if whole.len() + fraction.len() > 38 {
        return Err(DecimalError);
    }
    let unscaled = format!("{whole}{fraction}")
        .parse::<u128>()
        .map_err(|_| DecimalError)?;
    Ok(Amount {
        unscaled,
        scale: u32::try_from(fraction.len()).map_err(|_| DecimalError)?,
    })
}

fn format_amount(unscaled: u128, scale: u32) -> String {
    if scale == 0 {
        return unscaled.to_string();
    }
    let digits = unscaled.to_string();
    let scale = scale as usize;
    if digits.len() <= scale {
        let zeros = "0".repeat(scale - digits.len());
        return format!("0.{zeros}{digits}");
    }
    let split = digits.len() - scale;
    format!("{}.{}", &digits[..split], &digits[split..])
}

#[cfg(test)]
mod tests {
    use super::displayed_balance;

    #[test]
    fn gold_display_matches_the_recorded_sum() {
        assert_eq!(displayed_balance("0", "2595").as_deref(), Ok("2595"));
    }

    #[test]
    fn aligns_fractional_scales() {
        assert_eq!(displayed_balance("1.20", "3.4").as_deref(), Ok("4.60"));
        assert_eq!(displayed_balance("10", "0.50").as_deref(), Ok("10.50"));
        assert_eq!(displayed_balance("54.99", "0.01").as_deref(), Ok("55.00"));
        assert_eq!(displayed_balance("0.10", "0.20").as_deref(), Ok("0.30"));
    }

    #[test]
    fn rejects_amounts_that_are_not_plain_decimals() {
        for raw in ["", "-", "1.", ".5", "1.2.3", "2595 gold", "+1", "1e2"] {
            assert!(displayed_balance(raw, "1").is_err(), "{raw}");
        }
    }
}
