//! Exact arbitrary-precision decimal arithmetic.
//!
//! `03_TYPES_AND_VALUES/02`: INTEGER is an "Unbounded signed whole number. It
//! never wraps, saturates, or overflows to a different LCL value", and DECIMAL
//! is an "Exact finite base-10 decimal with an unbounded integer coefficient
//! and a finite non-negative count of fractional digits. Infinity, NaN, and
//! underflow-to-zero are not DECIMAL values."
//!
//! A machine integer or float cannot represent either, so this module carries
//! its own base-10 magnitude. It exists for one purpose: to decide the
//! statically knowable value questions the canonical model puts at this stage —
//!
//! > Every scalar division overload … has static result type DECIMAL … After
//! > reducing the quotient to lowest terms, it has a finite base-10 DECIMAL
//! > representation if and only if the denominator has no prime factors other
//! > than 2 and 5. Otherwise evaluation produces error.numeric.non_terminating.
//!
//! and `19_DIVISION_BY_ZERO.invalid.lcl`'s rule that "A mathematical-zero
//! denominator is invalid even in the direct first-argument context of ROUND".
//!
//! Everything here is pure, total and allocation-bounded by its inputs.

use std::cmp::Ordering;
use std::fmt;

/// An arbitrary-precision signed integer, base 10.
///
/// `digits` is least-significant-first with no leading zeros, so zero is the
/// empty magnitude and is never negative. Base 10 keeps literal decoding,
/// rendering and the factor-of-2-and-5 test exact and obvious; the operands are
/// source literals, so schoolbook algorithms are the right size of tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Integer {
    negative: bool,
    digits: Vec<u8>,
}

impl Integer {
    pub fn zero() -> Integer {
        Integer {
            negative: false,
            digits: Vec::new(),
        }
    }

    pub fn from_u64(value: u64) -> Integer {
        let mut digits = Vec::new();
        let mut rest = value;
        while rest > 0 {
            digits.push((rest % 10) as u8);
            rest /= 10;
        }
        Integer {
            negative: false,
            digits,
        }
    }

    /// Decode a run of ASCII decimal digits. Returns `None` for empty input or
    /// any non-digit byte.
    pub fn parse_digits(text: &str) -> Option<Integer> {
        if text.is_empty() {
            return None;
        }
        let mut digits = Vec::with_capacity(text.len());
        for byte in text.bytes().rev() {
            if !byte.is_ascii_digit() {
                return None;
            }
            digits.push(byte - b'0');
        }
        let mut value = Integer {
            negative: false,
            digits,
        };
        value.trim();
        Some(value)
    }

    fn trim(&mut self) {
        while self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.negative = false;
        }
    }

    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    pub fn is_negative(&self) -> bool {
        self.negative
    }

    pub fn negated(&self) -> Integer {
        let mut out = self.clone();
        if !out.is_zero() {
            out.negative = !out.negative;
        }
        out
    }

    pub fn abs(&self) -> Integer {
        Integer {
            negative: false,
            digits: self.digits.clone(),
        }
    }

    /// The number of decimal digits in the magnitude; zero has none.
    pub fn digit_count(&self) -> usize {
        self.digits.len()
    }

    fn cmp_magnitude(a: &[u8], b: &[u8]) -> Ordering {
        a.len().cmp(&b.len()).then_with(|| {
            for (left, right) in a.iter().zip(b).rev() {
                match left.cmp(right) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            Ordering::Equal
        })
    }

    fn add_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
        let mut carry = 0u8;
        for index in 0..a.len().max(b.len()) {
            let sum =
                a.get(index).copied().unwrap_or(0) + b.get(index).copied().unwrap_or(0) + carry;
            out.push(sum % 10);
            carry = sum / 10;
        }
        if carry > 0 {
            out.push(carry);
        }
        out
    }

    /// `a - b`, requiring `|a| >= |b|`.
    fn sub_magnitude(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0i8;
        for (index, left) in a.iter().enumerate() {
            let mut digit = *left as i8 - b.get(index).copied().unwrap_or(0) as i8 - borrow;
            if digit < 0 {
                digit += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            out.push(digit as u8);
        }
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    pub fn add(&self, other: &Integer) -> Integer {
        if self.negative == other.negative {
            let mut out = Integer {
                negative: self.negative,
                digits: Integer::add_magnitude(&self.digits, &other.digits),
            };
            out.trim();
            return out;
        }
        match Integer::cmp_magnitude(&self.digits, &other.digits) {
            Ordering::Equal => Integer::zero(),
            Ordering::Greater => {
                let mut out = Integer {
                    negative: self.negative,
                    digits: Integer::sub_magnitude(&self.digits, &other.digits),
                };
                out.trim();
                out
            }
            Ordering::Less => {
                let mut out = Integer {
                    negative: other.negative,
                    digits: Integer::sub_magnitude(&other.digits, &self.digits),
                };
                out.trim();
                out
            }
        }
    }

    pub fn sub(&self, other: &Integer) -> Integer {
        self.add(&other.negated())
    }

    pub fn mul(&self, other: &Integer) -> Integer {
        if self.is_zero() || other.is_zero() {
            return Integer::zero();
        }
        let mut digits = vec![0u8; self.digits.len() + other.digits.len()];
        for (i, &a) in self.digits.iter().enumerate() {
            let mut carry = 0u8;
            for (j, &b) in other.digits.iter().enumerate() {
                let index = i + j;
                let total = digits[index] + a * b + carry;
                digits[index] = total % 10;
                carry = total / 10;
            }
            let mut index = i + other.digits.len();
            while carry > 0 {
                let total = digits[index] + carry;
                digits[index] = total % 10;
                carry = total / 10;
                index += 1;
            }
        }
        let mut out = Integer {
            negative: self.negative != other.negative,
            digits,
        };
        out.trim();
        out
    }

    /// Multiply by `10^power`.
    pub fn shift_left(&self, power: usize) -> Integer {
        if self.is_zero() {
            return Integer::zero();
        }
        let mut digits = vec![0u8; power];
        digits.extend_from_slice(&self.digits);
        Integer {
            negative: self.negative,
            digits,
        }
    }

    /// Truncating division with remainder. `None` when `divisor` is zero.
    ///
    /// Schoolbook long division in base 10: each step tries the ten possible
    /// quotient digits, so no estimate can be wrong.
    pub fn divmod(&self, divisor: &Integer) -> Option<(Integer, Integer)> {
        if divisor.is_zero() {
            return None;
        }
        let dividend = self.abs();
        let magnitude = divisor.abs();
        if Integer::cmp_magnitude(&dividend.digits, &magnitude.digits) == Ordering::Less {
            return Some((Integer::zero(), self.clone()));
        }

        let mut quotient = vec![0u8; dividend.digits.len()];
        let mut remainder = Integer::zero();
        for index in (0..dividend.digits.len()).rev() {
            // remainder = remainder * 10 + digit
            remainder = remainder.shift_left(1);
            remainder = remainder.add(&Integer::from_u64(dividend.digits[index] as u64));
            let mut digit = 0u8;
            while Integer::cmp_magnitude(&remainder.digits, &magnitude.digits) != Ordering::Less {
                remainder = remainder.sub(&magnitude);
                digit += 1;
            }
            quotient[index] = digit;
        }
        let mut q = Integer {
            negative: self.negative != divisor.negative,
            digits: quotient,
        };
        q.trim();
        let mut r = remainder;
        r.negative = self.negative && !r.is_zero();
        Some((q, r))
    }

    /// Divide out every factor of `factor`, returning the count removed.
    fn factor_out(&self, factor: u64) -> (Integer, u32) {
        let divisor = Integer::from_u64(factor);
        let mut value = self.clone();
        let mut count = 0u32;
        while !value.is_zero() {
            let Some((quotient, remainder)) = value.divmod(&divisor) else {
                break;
            };
            if !remainder.is_zero() {
                break;
            }
            value = quotient;
            count = count.saturating_add(1);
        }
        (value, count)
    }

    pub fn gcd(&self, other: &Integer) -> Integer {
        let mut a = self.abs();
        let mut b = other.abs();
        while !b.is_zero() {
            let Some((_, remainder)) = a.divmod(&b) else {
                break;
            };
            a = b;
            b = remainder.abs();
        }
        a
    }

    pub fn compare(&self, other: &Integer) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => Integer::cmp_magnitude(&self.digits, &other.digits),
            (true, true) => Integer::cmp_magnitude(&other.digits, &self.digits),
        }
    }

    /// The value as an `i64`, when it fits.
    pub fn to_i64(&self) -> Option<i64> {
        let mut value: i64 = 0;
        for &digit in self.digits.iter().rev() {
            value = value.checked_mul(10)?.checked_add(digit as i64)?;
        }
        if self.negative {
            value = value.checked_neg()?;
        }
        Some(value)
    }
}

impl fmt::Display for Integer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return f.write_str("0");
        }
        if self.negative {
            f.write_str("-")?;
        }
        for &digit in self.digits.iter().rev() {
            write!(f, "{digit}")?;
        }
        Ok(())
    }
}

/// An exact decimal: `coefficient * 10^-scale`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decimal {
    coefficient: Integer,
    scale: u32,
}

/// Why an exact quotient has no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivisionDefect {
    /// `error.numeric.division_by_zero`.
    Zero,
    /// `error.numeric.non_terminating`.
    NonTerminating,
    /// The exact result is representable but larger than this stage will
    /// materialize. No diagnostic follows: the value is simply not statically
    /// known, and its check belongs to the demanding layer.
    TooLarge,
}

/// The largest number of digits this stage will materialize for one static
/// value. Beyond it the value is treated as not statically known rather than
/// computed, so a pathological literal costs nothing and produces no verdict.
const DIGIT_LIMIT: usize = 4096;

impl Decimal {
    pub fn from_integer(value: Integer) -> Decimal {
        Decimal {
            coefficient: value,
            scale: 0,
        }
    }

    /// Decode an `INTEGER_LITERAL` lexeme.
    pub fn parse_integer(text: &str) -> Option<Decimal> {
        Some(Decimal::from_integer(Integer::parse_digits(text)?))
    }

    /// Decode a `DECIMAL_LITERAL` lexeme: digits, `.`, one or more digits.
    pub fn parse_decimal(text: &str) -> Option<Decimal> {
        let (whole, fraction) = text.split_once('.')?;
        if fraction.is_empty() {
            return None;
        }
        let joined = format!("{whole}{fraction}");
        Some(Decimal {
            coefficient: Integer::parse_digits(&joined)?,
            scale: u32::try_from(fraction.len()).ok()?,
        })
    }

    pub fn is_zero(&self) -> bool {
        self.coefficient.is_zero()
    }

    pub fn is_negative(&self) -> bool {
        self.coefficient.is_negative()
    }

    pub fn negated(&self) -> Decimal {
        Decimal {
            coefficient: self.coefficient.negated(),
            scale: self.scale,
        }
    }

    /// Both values re-expressed at one common scale.
    fn aligned(&self, other: &Decimal) -> (Integer, Integer, u32) {
        let scale = self.scale.max(other.scale);
        let left = self.coefficient.shift_left((scale - self.scale) as usize);
        let right = other.coefficient.shift_left((scale - other.scale) as usize);
        (left, right, scale)
    }

    pub fn add(&self, other: &Decimal) -> Decimal {
        let (left, right, scale) = self.aligned(other);
        Decimal {
            coefficient: left.add(&right),
            scale,
        }
    }

    pub fn sub(&self, other: &Decimal) -> Decimal {
        let (left, right, scale) = self.aligned(other);
        Decimal {
            coefficient: left.sub(&right),
            scale,
        }
    }

    pub fn mul(&self, other: &Decimal) -> Decimal {
        Decimal {
            coefficient: self.coefficient.mul(&other.coefficient),
            scale: self.scale.saturating_add(other.scale),
        }
    }

    pub fn compare(&self, other: &Decimal) -> Ordering {
        let (left, right, _) = self.aligned(other);
        left.compare(&right)
    }

    /// The exact quotient, or why it has none.
    ///
    /// "Division evaluates the exact mathematical quotient with no fixed global
    /// precision and no implicit rounding."
    #[cfg(test)]
    pub fn divide(&self, other: &Decimal) -> Result<Decimal, DivisionDefect> {
        Rational::of(self, other)?.to_terminating_decimal()
    }

    /// Round half-to-even to `digits` fractional digits.
    #[cfg(test)]
    pub fn round(&self, digits: u32) -> Result<Decimal, DivisionDefect> {
        Rational::of(self, &Decimal::from_integer(Integer::from_u64(1)))?.round_half_even(digits)
    }

    /// True when this value is an exact whole number.
    pub fn is_integral(&self) -> bool {
        if self.scale == 0 {
            return true;
        }
        let divisor = Integer::from_u64(1).shift_left(self.scale as usize);
        self.coefficient
            .divmod(&divisor)
            .map(|(_, remainder)| remainder.is_zero())
            .unwrap_or(false)
    }

    /// The value as an `i64`, when it is integral and fits.
    pub fn to_i64(&self) -> Option<i64> {
        if !self.is_integral() {
            return None;
        }
        let divisor = Integer::from_u64(1).shift_left(self.scale as usize);
        let (quotient, _) = self.coefficient.divmod(&divisor)?;
        quotient.to_i64()
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scale == 0 {
            return write!(f, "{}", self.coefficient);
        }
        let magnitude = self.coefficient.abs().to_string();
        let scale = self.scale as usize;
        let padded = if magnitude.len() <= scale {
            format!("{}{}", "0".repeat(scale - magnitude.len() + 1), magnitude)
        } else {
            magnitude
        };
        let split = padded.len() - scale;
        if self.coefficient.is_negative() {
            f.write_str("-")?;
        }
        write!(f, "{}.{}", &padded[..split], &padded[split..])
    }
}

/// An exact rational, used only between a division and its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rational {
    numerator: Integer,
    /// Always positive.
    denominator: Integer,
}

impl Rational {
    /// `left / right`, reduced to lowest terms.
    pub fn of(left: &Decimal, right: &Decimal) -> Result<Rational, DivisionDefect> {
        if right.is_zero() {
            return Err(DivisionDefect::Zero);
        }
        // (ca * 10^-sa) / (cb * 10^-sb) = (ca * 10^sb) / (cb * 10^sa)
        let mut numerator = left.coefficient.shift_left(right.scale as usize);
        let mut denominator = right.coefficient.shift_left(left.scale as usize);
        if denominator.is_negative() {
            numerator = numerator.negated();
            denominator = denominator.negated();
        }
        if numerator.digit_count() > DIGIT_LIMIT || denominator.digit_count() > DIGIT_LIMIT {
            return Err(DivisionDefect::TooLarge);
        }
        let divisor = numerator.gcd(&denominator);
        if !divisor.is_zero() {
            let one = Integer::from_u64(1);
            if divisor.compare(&one) != Ordering::Equal {
                numerator = numerator
                    .divmod(&divisor)
                    .map(|(q, _)| q)
                    .unwrap_or(numerator);
                denominator = denominator
                    .divmod(&divisor)
                    .map(|(q, _)| q)
                    .unwrap_or(denominator);
            }
        }
        Ok(Rational {
            numerator,
            denominator,
        })
    }

    /// The exact finite base-10 value, when one exists.
    ///
    /// "After reducing the quotient to lowest terms, it has a finite base-10
    /// DECIMAL representation if and only if the denominator has no prime
    /// factors other than 2 and 5."
    pub fn to_terminating_decimal(&self) -> Result<Decimal, DivisionDefect> {
        if self.numerator.is_zero() {
            return Ok(Decimal::from_integer(Integer::zero()));
        }
        let (after_two, twos) = self.denominator.factor_out(2);
        let (residue, fives) = after_two.factor_out(5);
        if residue.compare(&Integer::from_u64(1)) != Ordering::Equal {
            return Err(DivisionDefect::NonTerminating);
        }
        let scale = twos.max(fives);
        if scale as usize > DIGIT_LIMIT {
            return Err(DivisionDefect::TooLarge);
        }
        // numerator * 10^scale / denominator is exact by construction.
        let scaled = self.numerator.shift_left(scale as usize);
        let (coefficient, remainder) = scaled
            .divmod(&self.denominator)
            .ok_or(DivisionDefect::Zero)?;
        debug_assert!(remainder.is_zero(), "a 2-and-5 denominator divides exactly");
        Ok(Decimal { coefficient, scale })
    }

    /// Round the exact value half-to-even to `digits` fractional digits.
    ///
    /// "When its direct first argument is a division expression, ROUND
    /// evaluates the exact mathematical quotient and rounds it once; this is
    /// the only context in which an otherwise non-terminating quotient is
    /// materialized."
    pub fn round_half_even(&self, digits: u32) -> Result<Decimal, DivisionDefect> {
        if digits as usize > DIGIT_LIMIT {
            return Err(DivisionDefect::TooLarge);
        }
        let scaled = self.numerator.shift_left(digits as usize);
        let (quotient, remainder) = scaled
            .divmod(&self.denominator)
            .ok_or(DivisionDefect::Zero)?;
        if remainder.is_zero() {
            return Ok(Decimal {
                coefficient: quotient,
                scale: digits,
            });
        }
        // Compare 2*|remainder| with the denominator to place the half.
        let twice = remainder.abs().mul(&Integer::from_u64(2));
        let one = Integer::from_u64(1);
        let round_away = match twice.compare(&self.denominator) {
            Ordering::Greater => true,
            Ordering::Less => false,
            // Exactly half: to even.
            Ordering::Equal => {
                let (_, parity) = quotient
                    .divmod(&Integer::from_u64(2))
                    .unwrap_or((Integer::zero(), Integer::zero()));
                !parity.is_zero()
            }
        };
        let coefficient = if round_away {
            if self.numerator.is_negative() {
                quotient.sub(&one)
            } else {
                quotient.add(&one)
            }
        } else {
            quotient
        };
        Ok(Decimal {
            coefficient,
            scale: digits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integer(text: &str) -> Decimal {
        Decimal::parse_integer(text).expect("integer literal")
    }

    fn decimal(text: &str) -> Decimal {
        Decimal::parse_decimal(text).expect("decimal literal")
    }

    #[test]
    fn literals_round_trip() {
        assert_eq!(integer("0").to_string(), "0");
        assert_eq!(integer("1024").to_string(), "1024");
        assert_eq!(decimal("0.125").to_string(), "0.125");
        assert_eq!(decimal("12.50").to_string(), "12.50");
        // Unbounded: 60 digits is not a special case.
        let long = "1".repeat(60);
        assert_eq!(integer(&long).to_string(), long);
    }

    #[test]
    fn arithmetic_is_exact_and_unbounded() {
        let big = integer(&format!("1{}", "0".repeat(40)));
        let sum = big.add(&integer("1"));
        assert_eq!(sum.to_string(), format!("1{}1", "0".repeat(39)));
        let product = big.mul(&big);
        assert_eq!(product.to_string(), format!("1{}", "0".repeat(80)));
        assert_eq!(decimal("0.1").add(&decimal("0.2")).to_string(), "0.3");
        assert_eq!(integer("7").sub(&integer("9")).to_string(), "-2");
        assert_eq!(decimal("1.5").mul(&integer("4")).to_string(), "6.0");
    }

    #[test]
    fn every_scalar_quotient_that_terminates_has_its_exact_value() {
        // `11_EXACT_DIVISION_AND_ROUNDING.lcl` pins these three.
        assert_eq!(integer("4").divide(&integer("2")).unwrap().to_string(), "2");
        assert_eq!(
            integer("1").divide(&integer("8")).unwrap().to_string(),
            "0.125"
        );
        assert_eq!(
            integer("9").divide(&integer("4")).unwrap().to_string(),
            "2.25"
        );
        assert_eq!(
            decimal("1.44").divide(&decimal("1.2")).unwrap().to_string(),
            "1.2"
        );
    }

    #[test]
    fn a_denominator_with_another_prime_factor_does_not_terminate() {
        // `18_NON_TERMINATING_DIVISION.invalid.lcl`.
        assert_eq!(
            integer("1").divide(&integer("3")),
            Err(DivisionDefect::NonTerminating)
        );
        assert_eq!(
            integer("22").divide(&integer("7")),
            Err(DivisionDefect::NonTerminating)
        );
        // 6 = 2 * 3, so the 3 survives reduction only when it must.
        assert_eq!(
            integer("1").divide(&integer("6")),
            Err(DivisionDefect::NonTerminating)
        );
        // …but 3/6 reduces to 1/2 and does terminate.
        assert_eq!(
            integer("3").divide(&integer("6")).unwrap().to_string(),
            "0.5"
        );
    }

    #[test]
    fn a_mathematical_zero_denominator_has_no_quotient() {
        // `19_DIVISION_BY_ZERO.invalid.lcl`.
        assert_eq!(
            integer("1").divide(&integer("0")),
            Err(DivisionDefect::Zero)
        );
        assert_eq!(
            integer("0").divide(&integer("0")),
            Err(DivisionDefect::Zero)
        );
        assert_eq!(
            decimal("1.0").divide(&decimal("0.0")),
            Err(DivisionDefect::Zero)
        );
        // Zero over anything is exactly zero.
        assert_eq!(integer("0").divide(&integer("3")).unwrap().to_string(), "0");
    }

    #[test]
    fn round_evaluates_the_exact_quotient_once_and_ties_go_to_even() {
        let third = Rational::of(&integer("1"), &integer("3")).expect("quotient");
        assert_eq!(third.round_half_even(2).unwrap().to_string(), "0.33");
        assert_eq!(third.round_half_even(0).unwrap().to_string(), "0");

        let two_thirds = Rational::of(&integer("2"), &integer("3")).expect("quotient");
        assert_eq!(two_thirds.round_half_even(2).unwrap().to_string(), "0.67");

        // Exact halves round to even, never away from zero.
        assert_eq!(decimal("2.5").round(0).unwrap().to_string(), "2");
        assert_eq!(decimal("3.5").round(0).unwrap().to_string(), "4");
        assert_eq!(decimal("0.125").round(2).unwrap().to_string(), "0.12");
        assert_eq!(decimal("0.135").round(2).unwrap().to_string(), "0.14");
        // `DECIMAL_LITERAL` carries no sign: negation is the unary operator,
        // so a negative value is built the way the language builds it.
        assert_eq!(decimal("2.5").negated().round(0).unwrap().to_string(), "-2");
        assert_eq!(
            decimal("0.125").negated().round(2).unwrap().to_string(),
            "-0.12"
        );
    }

    #[test]
    fn a_zero_denominator_is_still_invalid_inside_round() {
        assert_eq!(
            Rational::of(&integer("1"), &integer("0")),
            Err(DivisionDefect::Zero)
        );
    }

    #[test]
    fn negatives_and_comparison_are_exact() {
        assert_eq!(integer("1").negated().to_string(), "-1");
        assert_eq!(integer("0").negated().to_string(), "0");
        assert_eq!(
            integer("1").negated().divide(&integer("3")),
            Err(DivisionDefect::NonTerminating)
        );
        assert_eq!(
            integer("1")
                .negated()
                .divide(&integer("4"))
                .unwrap()
                .to_string(),
            "-0.25"
        );
        assert_eq!(integer("10").compare(&decimal("10.00")), Ordering::Equal);
        assert_eq!(integer("2").compare(&decimal("10.00")), Ordering::Less);
        assert_eq!(
            integer("2").negated().compare(&integer("1").negated()),
            Ordering::Less
        );
    }

    #[test]
    fn integral_conversion_is_exact() {
        assert_eq!(integer("100").to_i64(), Some(100));
        assert_eq!(decimal("100.00").to_i64(), Some(100));
        assert_eq!(decimal("100.25").to_i64(), None);
        assert_eq!(integer(&"9".repeat(40)).to_i64(), None);
        assert!(decimal("100.00").is_integral());
        assert!(!decimal("100.25").is_integral());
    }

    #[test]
    fn an_oversized_value_is_declined_rather_than_computed() {
        let huge = Decimal {
            coefficient: Integer::from_u64(1).shift_left(DIGIT_LIMIT + 1),
            scale: 0,
        };
        assert_eq!(huge.divide(&integer("3")), Err(DivisionDefect::TooLarge));
        let third = Rational::of(&integer("1"), &integer("3")).expect("quotient");
        assert_eq!(
            third.round_half_even(u32::MAX),
            Err(DivisionDefect::TooLarge)
        );
    }
}
