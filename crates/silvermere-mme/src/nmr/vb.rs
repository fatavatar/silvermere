//! The arithmetic and string rules NMR's results depend on.
//!
//! NMR is a Visual Basic 6 program, and several columns come out of that language's rules
//! rather than plain arithmetic: rounding is half-to-even, a fraction stored in a whole-number
//! variable is rounded the same way, money-typed ("Currency") variables keep four decimals,
//! string positions count from 1 and a number is read from text by skipping blanks. The port
//! does the same where it changes a value MME sees.

/// Rounds to `digits` decimals, half to even (`Round(x, digits)`).
pub(super) fn round(x: f64, digits: i32) -> f64 {
    let scale = 10f64.powi(digits);
    (x * scale).round_ties_even() / scale
}

/// A fraction stored in a whole-number variable: rounded half to even.
pub(super) fn whole(x: f64) -> i32 {
    x.round_ties_even() as i32
}

/// A value stored in a money-typed variable, which keeps four decimals.
pub(super) fn money(x: f64) -> f64 {
    round(x, 4)
}

/// The number at the start of `text` (`Val`): blanks, tabs and line feeds anywhere are skipped,
/// then an optional sign, digits, a fraction and an exponent are read; anything else ends it.
pub(super) fn val(text: &[u8]) -> f64 {
    let kept: Vec<u8> = text
        .iter()
        .copied()
        .filter(|b| !matches!(b, b' ' | b'\t' | b'\n'))
        .collect();
    let mut end = 0;
    let digits = |from: usize| {
        from + kept[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    if matches!(kept.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    end = digits(end);
    if kept.get(end) == Some(&b'.') {
        end = digits(end + 1);
    }
    if matches!(kept.get(end), Some(b'e' | b'E' | b'd' | b'D')) {
        let mut exp = end + 1;
        if matches!(kept.get(exp), Some(b'+' | b'-')) {
            exp += 1;
        }
        let exp_end = digits(exp);
        if exp_end > exp {
            end = exp_end;
        }
    }
    let number: String = kept[..end]
        .iter()
        .map(|&b| match b {
            b'd' | b'D' => 'e',
            other => char::from(other),
        })
        .collect();
    number.parse().unwrap_or(0.0)
}

/// The 1-based position of `needle` in `hay` at or after `start` (`InStr`), 0 when absent.
///
/// An empty `hay` holds nothing, not even an empty `needle`.
pub(super) fn instr(start: usize, hay: &[u8], needle: &[u8]) -> usize {
    if hay.is_empty() || start == 0 || start > hay.len() {
        return 0;
    }
    if needle.is_empty() {
        return start;
    }
    hay[start - 1..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map_or(0, |i| start + i)
}

/// `len` bytes of `text` from the 1-based position `start` (`Mid`), cut at the end.
pub(super) fn mid(text: &[u8], start: usize, len: usize) -> &[u8] {
    let from = (start.max(1) - 1).min(text.len());
    let to = from.saturating_add(len).min(text.len());
    &text[from..to]
}

/// `text` from the 1-based position `start` to its end.
pub(super) fn mid_to_end(text: &[u8], start: usize) -> &[u8] {
    &text[(start.max(1) - 1).min(text.len())..]
}

/// The byte at the 1-based position `at`, if there is one.
pub(super) fn at(text: &[u8], at: usize) -> Option<u8> {
    at.checked_sub(1).and_then(|i| text.get(i)).copied()
}

/// `text` with ASCII letters lowered (`LCase`); positions do not move.
pub(super) fn lower(text: &[u8]) -> Vec<u8> {
    text.to_ascii_lowercase()
}

/// A number joined into text: whole numbers without a fraction, others as short as they read
/// back (NMR's money values have at most four decimals, which print the same way).
pub(super) fn num(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// A money-typed ("Currency") value: a whole number of ten-thousandths, so sums are exact and
/// a product is rounded half to even to four decimals. NMR keeps drop chances in these.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Money(i64);

impl Money {
    /// One whole.
    pub(super) const ONE: Self = Self(10_000);

    /// A fraction stored in a money-typed variable.
    pub(super) fn from_f64(x: f64) -> Self {
        Self((x * 10_000.0).round_ties_even() as i64)
    }

    /// The product, rounded to four decimals.
    pub(super) fn times(self, other: Self) -> Self {
        Self(div_even(i128::from(self.0) * i128::from(other.0), 10_000) as i64)
    }

    /// The value times 100 rounded to one decimal, as joined into text ("12.5").
    pub(super) fn percent_text(self) -> String {
        let tenths = div_even(i128::from(self.0) * 100, 1_000);
        let sign = if tenths < 0 { "-" } else { "" };
        let (whole, tenth) = (tenths.abs() / 10, tenths.abs() % 10);
        if tenth == 0 {
            format!("{sign}{whole}")
        } else {
            format!("{sign}{whole}.{tenth}")
        }
    }
}

impl std::ops::Add for Money {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::Sub for Money {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

/// `n / d` rounded half to even.
fn div_even(n: i128, d: i128) -> i128 {
    let (q, r) = (n.div_euclid(d), n.rem_euclid(d));
    match (2 * r).cmp(&d) {
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
    }
}

/// The language's random number generator (`Rnd`): a 24-bit linear congruential sequence.
///
/// NMR seeds it from the clock for every simulation; the port seeds it with a fixed value per
/// monster, so an export is repeatable while drawing numbers the same way.
pub(super) struct Rnd {
    state: u32,
}

impl Rnd {
    /// A generator starting from `seed` (only its low 24 bits count).
    pub(super) fn new(seed: u32) -> Self {
        Self {
            state: seed & 0x00FF_FFFF,
        }
    }

    /// The next number in `0.0..1.0`.
    fn next(&mut self) -> f32 {
        self.state = self
            .state
            .wrapping_mul(0x43FD_43FD)
            .wrapping_add(0x00C3_9EC3)
            & 0x00FF_FFFF;
        self.state as f32 / 16_777_216.0
    }

    /// A whole number from `low` to `high` as NMR draws one: `Int((high - low + 1) * Rnd + low)`.
    pub(super) fn between(&mut self, low: i32, high: i32) -> i32 {
        ((high - low + 1) as f32 * self.next() + low as f32).floor() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_is_half_to_even() {
        assert_eq!(round(2.5, 0), 2.0);
        assert_eq!(round(3.5, 0), 4.0);
        assert_eq!(round(0.125, 2), 0.12);
        assert_eq!(whole(-2.5), -2);
        assert_eq!(money(1.0 / 3.0), 0.3333);
    }

    #[test]
    fn val_reads_a_leading_number() {
        assert_eq!(val(b"\n25"), 25.0);
        assert_eq!(val(b"1 2 3"), 123.0);
        assert_eq!(val(b"-4.5x"), -4.5);
        assert_eq!(val(b"25:"), 25.0);
        assert_eq!(val(b"text"), 0.0);
        assert_eq!(val(b"1e2"), 100.0);
    }

    #[test]
    fn instr_counts_from_one() {
        assert_eq!(instr(1, b"abcabc", b"bc"), 2);
        assert_eq!(instr(3, b"abcabc", b"bc"), 5);
        assert_eq!(instr(1, b"abc", b"x"), 0);
        assert_eq!(instr(1, b"", b""), 0);
        assert_eq!(instr(2, b"abc", b""), 2);
        assert_eq!(mid(b"abcdef", 2, 3), b"bcd");
        assert_eq!(mid(b"abc", 3, 9), b"c");
    }

    #[test]
    fn numbers_join_like_the_original() {
        assert_eq!(num(12.0), "12");
        assert_eq!(num(-3.0), "-3");
        assert_eq!(num(12.5), "12.5");
    }

    #[test]
    fn money_keeps_four_decimals() {
        let third = Money::from_f64(1.0 / 3.0);
        assert_eq!(third, Money(3333));
        assert_eq!(third.times(third), Money(1111));
        assert_eq!(Money(125).percent_text(), "1.2");
        assert_eq!(Money(1250).percent_text(), "12.5");
        assert_eq!(Money::ONE.percent_text(), "100");
        assert_eq!((Money::ONE - Money(25_000)).percent_text(), "-150");
    }

    #[test]
    fn draws_stay_in_range() {
        let mut rnd = Rnd::new(7);
        for _ in 0..1000 {
            let n = rnd.between(1, 100);
            assert!((1..=100).contains(&n));
        }
    }
}
