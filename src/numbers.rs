pub type Int = i64;
pub const TRUE: i64 = -1;
pub const FALSE: i64 = 0;

/// Is the value true-ish.
#[inline]
pub fn is_true(value: Int) -> bool {
    value != FALSE
}

/// Transform `bool` to `Int`.
#[inline]
pub fn from_bool(value: bool) -> Int {
    if value {
        TRUE
    } else {
        FALSE
    }
}

/// Return character for the numerical code, if not possible show the replacement character.
#[inline]
pub fn to_char(value: Int) -> char {
    if let Ok(u) = value.try_into() {
        if let Some(c) = char::from_u32(u) {
            return c;
        }
    }
    '�'
}

/// Transform i128 to i64 clamping to the limits of i64.
#[inline]
pub fn saturating_i128_to_i64(value: i128) -> i64 {
    value.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}
