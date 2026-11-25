// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Dates, lengths and axis labels for Gantt charts, in milliseconds since 1970.

use crate::Result;

pub(super) const SECOND: i64 = 1_000;
pub(super) const MINUTE: i64 = 60 * SECOND;
pub(super) const HOUR: i64 = 60 * MINUTE;
pub(super) const DAY: i64 = 24 * HOUR;
/// The longest task: 100 years.
pub(super) const MAX_SPAN: i64 = 36_525 * DAY;

pub(super) const WEEKDAYS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];
pub(super) const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Days since 1970-01-01 of a proleptic Gregorian date.
pub(super) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The year, month and day of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    days_from_civil(year + i64::from(month == 12), month % 12 + 1, 1)
        - days_from_civil(year, month, 1)
}

/// Read `text` as a date in a `dateFormat` of `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and
/// literal characters, as milliseconds since 1970.
pub(super) fn parse_date(format: &str, text: &str) -> Option<i64> {
    let (mut f, mut t) = (format, text);
    let (mut year, mut month, mut day, mut hour, mut minute, mut second) =
        (None, None, None, 0, 0, 0);
    while !f.is_empty() {
        let token = ["YYYY", "MM", "DD", "HH", "mm", "ss"]
            .into_iter()
            .find(|token| f.starts_with(token));
        let Some(token) = token else {
            t = t.strip_prefix(f.chars().next()?)?;
            f = &f[1..];
            continue;
        };
        let digits = t.get(..token.len())?;
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let value: i64 = digits.parse().ok()?;
        match token {
            "YYYY" => year = Some(value),
            "MM" => month = Some(value),
            "DD" => day = Some(value),
            "HH" => hour = value,
            "mm" => minute = value,
            _ => second = value,
        }
        f = &f[token.len()..];
        t = &t[token.len()..];
    }
    let (year, month, day) = (year?, month?, day?);
    if !t.is_empty()
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * DAY + hour * HOUR + minute * MINUTE + second * SECOND)
}

/// A task's length: milliseconds, or calendar months.
#[derive(Clone, Copy)]
pub(super) enum Length {
    Fixed(i64),
    Months(i64),
}

/// A length as Mermaid writes one: a number (`3`, `1.5`) and `ms`, `s`, `m`, `h`, `d`, `w`, `M`
/// (calendar months) or `y` (12 months). Months and years are whole numbers. A length over 100
/// years is kept just over, so the task is refused for its length.
pub(super) fn parse_length(text: &str) -> Option<Length> {
    let split = text.find(|c: char| !c.is_ascii_digit() && c != '.')?;
    let (number, unit) = text.split_at(split);
    let (whole, fraction) = number.split_once('.').unwrap_or((number, "0"));
    if whole.is_empty() || fraction.is_empty() || fraction.contains('.') {
        return None;
    }
    let unit = match unit {
        "ms" => 1,
        "s" => SECOND,
        "m" => MINUTE,
        "h" => HOUR,
        "d" => DAY,
        "w" => 7 * DAY,
        "M" | "y" if number.contains('.') => return None,
        "M" | "y" => {
            let per = if unit == "y" { 12 } else { 1 };
            let count = whole.parse::<i64>().unwrap_or(i64::MAX).min(1201);
            return Some(Length::Months((count * per).min(1201)));
        }
        _ => return None,
    };
    let number: f64 = number.parse().ok()?;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let length = (number * unit as f64).round().min((MAX_SPAN + DAY) as f64) as i64;
    Some(Length::Fixed(length))
}

/// `at` plus `months` calendar months, the day held to the new month's last day as Mermaid's
/// date library does.
pub(super) fn add_months(at: i64, months: i64) -> i64 {
    let (year, month, day) = civil_from_days(at.div_euclid(DAY));
    let total = year * 12 + month - 1 + months;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    days_from_civil(year, month, day.min(days_in_month(year, month))) * DAY + at.rem_euclid(DAY)
}

/// `at` written in a d3-style `axisFormat`.
pub(super) fn format_axis(line: usize, format: &str, at: i64) -> Result<String> {
    let days = at.div_euclid(DAY);
    let (year, month, day) = civil_from_days(days);
    let time = at.rem_euclid(DAY);
    let weekday = WEEKDAYS[(days + 4).rem_euclid(7) as usize];
    let (month_name, weekday_name) = (MONTHS[month as usize - 1], weekday);
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let spec = chars.next().unwrap_or(' ');
        match spec {
            'Y' => out.push_str(&format!("{year:04}")),
            'y' => out.push_str(&format!("{:02}", year.rem_euclid(100))),
            'm' => out.push_str(&format!("{month:02}")),
            'd' => out.push_str(&format!("{day:02}")),
            'e' => out.push_str(&format!("{day:>2}")),
            'H' => out.push_str(&format!("{:02}", time / HOUR)),
            'M' => out.push_str(&format!("{:02}", time % HOUR / MINUTE)),
            'S' => out.push_str(&format!("{:02}", time % MINUTE / SECOND)),
            'b' => out.push_str(&month_name[..3]),
            'B' => out.push_str(month_name),
            'a' => {
                out.push(weekday_name.as_bytes()[0].to_ascii_uppercase() as char);
                out.push_str(&weekday_name[1..3]);
            }
            'A' => {
                out.push(weekday_name.as_bytes()[0].to_ascii_uppercase() as char);
                out.push_str(&weekday_name[1..]);
            }
            '%' => out.push('%'),
            _ => {
                return Err(format!(
                    "Line {line}: axisFormat uses %Y, %y, %m, %d, %e, %H, %M, %S, %b, %B, %a, %A and %%, not %{spec}"
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{DAY, add_months, days_from_civil};

    #[test]
    fn adds_calendar_months_holding_the_day_to_the_month_end() {
        let at = |y, m, d| days_from_civil(y, m, d) * DAY + 5;
        assert_eq!(add_months(at(2024, 2, 29), 12), at(2025, 2, 28));
        assert_eq!(add_months(at(2026, 1, 31), 1), at(2026, 2, 28));
        assert_eq!(add_months(at(2026, 11, 15), 3), at(2027, 2, 15));
        assert_eq!(add_months(at(1969, 12, 31), 2), at(1970, 2, 28));
    }
}
