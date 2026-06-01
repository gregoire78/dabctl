#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DabDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub ms: Option<u16>,
}

pub fn format_lto(lto: i8) -> String {
    let total_minutes = i32::from(lto) * 30;
    let sign = if total_minutes < 0 { '-' } else { '+' };
    let abs = total_minutes.abs();
    format!("{}{:02}:{:02}", sign, abs / 60, abs % 60)
}

pub fn mjd_to_ymd(mjd: i32) -> (i32, u8, u8) {
    let mjd_f = mjd as f64;
    let y0 = ((mjd_f - 15078.2) / 365.25).floor() as i32;
    let m0 = ((mjd_f - 14956.1 - ((y0 as f64) * 365.25).floor()) / 30.6001).floor() as i32;
    let d = mjd
        - 14956
        - ((y0 as f64) * 365.25).floor() as i32
        - ((m0 as f64) * 30.6001).floor() as i32;
    let k = if m0 == 14 || m0 == 15 { 1 } else { 0 };
    let y = y0 + k;
    let m = m0 - 1 - k * 12;
    (y, m as u8, d as u8)
}

pub fn apply_lto(utc: &DabDateTime, lto: i8) -> DabDateTime {
    let day_count = days_from_civil(utc.year + 1900, u32::from(utc.month), u32::from(utc.day));
    let sec_of_day =
        i64::from(utc.hour) * 3600 + i64::from(utc.minute) * 60 + i64::from(utc.second);
    let total_seconds = day_count * 86_400 + sec_of_day + i64::from(lto) * 1800;

    let norm_days = total_seconds.div_euclid(86_400);
    let rem = total_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(norm_days);
    let hour = (rem / 3600) as u8;
    let minute = ((rem % 3600) / 60) as u8;
    let second = (rem % 60) as u8;

    DabDateTime {
        year: year - 1900,
        month: month as u8,
        day: day as u8,
        hour,
        minute,
        second,
        ms: utc.ms,
    }
}

pub fn format_dab_datetime(dt: &DabDateTime, output_ms: bool, time_only: bool) -> String {
    if time_only {
        return match dt.ms {
            Some(ms) => {
                if output_ms {
                    format!("{:02}:{:02}:{:02}.{:03}", dt.hour, dt.minute, dt.second, ms)
                } else {
                    format!("{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second)
                }
            }
            None => format!("{:02}:{:02}", dt.hour, dt.minute),
        };
    }

    let year = dt.year + 1900;
    let weekday = weekday_name(year, u32::from(dt.month), u32::from(dt.day));
    let base = format!("{:04}-{:02}-{:02}, {} - ", year, dt.month, dt.day, weekday);

    match dt.ms {
        Some(ms) => {
            if output_ms {
                format!(
                    "{}{:02}:{:02}:{:02}.{:03}",
                    base, dt.hour, dt.minute, dt.second, ms
                )
            } else {
                format!("{}{:02}:{:02}:{:02}", base, dt.hour, dt.minute, dt.second)
            }
        }
        None => format!("{}{:02}:{:02}", base, dt.hour, dt.minute),
    }
}

pub fn format_dab_datetime_iso8601(
    dt: &DabDateTime,
    output_ms: bool,
    suffix: Option<&str>,
    time_only: bool,
) -> String {
    let mut base = if time_only {
        match dt.ms {
            Some(ms) => {
                if output_ms {
                    format!("{:02}:{:02}:{:02}.{:03}", dt.hour, dt.minute, dt.second, ms)
                } else {
                    format!("{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second)
                }
            }
            None => format!("{:02}:{:02}", dt.hour, dt.minute),
        }
    } else {
        let year = dt.year + 1900;
        match dt.ms {
            Some(ms) => {
                if output_ms {
                    format!(
                        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
                        year, dt.month, dt.day, dt.hour, dt.minute, dt.second, ms
                    )
                } else {
                    format!(
                        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                        year, dt.month, dt.day, dt.hour, dt.minute, dt.second
                    )
                }
            }
            None => format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}",
                year, dt.month, dt.day, dt.hour, dt.minute
            ),
        }
    };

    if let Some(s) = suffix {
        base.push_str(s);
    }

    base
}

pub fn format_dab_datetime_custom(dt: &DabDateTime, offset: Option<&str>, pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut i = 0usize;

    while i < pattern.len() {
        let rest = &pattern[i..];

        if let Some(after_open) = rest.strip_prefix('[') {
            if let Some(close_idx) = after_open.find(']') {
                out.push_str(&after_open[..close_idx]);
                i += close_idx + 2;
                continue;
            }
        }

        if let Some((token, token_len)) = match_custom_token(rest) {
            out.push_str(&render_custom_token(token, dt, offset));
            i += token_len;
            continue;
        }

        if let Some(ch) = rest.chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    out
}

fn match_custom_token(input: &str) -> Option<(&'static str, usize)> {
    const TOKENS: [&str; 23] = [
        "YYYY", "MMMM", "dddd", "SSS", "MMM", "ddd", "hh", "HH", "mm", "ss", "DD", "YY", "MM",
        "dd", "ZZ", "Z", "M", "D", "d", "H", "h", "m", "s",
    ];

    TOKENS
        .iter()
        .find(|token| input.starts_with(**token))
        .map(|token| (*token, token.len()))
}

fn render_custom_token(token: &str, dt: &DabDateTime, offset: Option<&str>) -> String {
    const MONTHS_SHORT: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const MONTHS_FULL: [&str; 12] = [
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
    const WEEKDAY_MIN: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
    const WEEKDAY_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const WEEKDAY_FULL: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];

    let year = dt.year + 1900;
    let month_idx = usize::from(dt.month.saturating_sub(1).min(11));
    let weekday_idx = weekday_index(year, u32::from(dt.month), u32::from(dt.day));
    let h12 = ((dt.hour + 11) % 12) + 1;

    match token {
        "YY" => format!("{:02}", year.rem_euclid(100)),
        "YYYY" => format!("{:04}", year),
        "M" => dt.month.to_string(),
        "MM" => format!("{:02}", dt.month),
        "MMM" => MONTHS_SHORT[month_idx].to_string(),
        "MMMM" => MONTHS_FULL[month_idx].to_string(),
        "D" => dt.day.to_string(),
        "DD" => format!("{:02}", dt.day),
        "d" => weekday_idx.to_string(),
        "dd" => WEEKDAY_MIN[weekday_idx].to_string(),
        "ddd" => WEEKDAY_SHORT[weekday_idx].to_string(),
        "dddd" => WEEKDAY_FULL[weekday_idx].to_string(),
        "H" => dt.hour.to_string(),
        "HH" => format!("{:02}", dt.hour),
        "h" => h12.to_string(),
        "hh" => format!("{:02}", h12),
        "m" => dt.minute.to_string(),
        "mm" => format!("{:02}", dt.minute),
        "s" => dt.second.to_string(),
        "ss" => format!("{:02}", dt.second),
        "SSS" => format!("{:03}", dt.ms.unwrap_or(0)),
        "Z" => normalize_offset(offset, false),
        "ZZ" => normalize_offset(offset, true),
        _ => token.to_string(),
    }
}

fn normalize_offset(offset: Option<&str>, compact: bool) -> String {
    let off = offset.unwrap_or("+00:00");
    let expanded = if off.eq_ignore_ascii_case("z") {
        "+00:00".to_string()
    } else if off.len() == 5 && (off.starts_with('+') || off.starts_with('-')) {
        format!("{}{}:{}", &off[0..1], &off[1..3], &off[3..5])
    } else {
        off.to_string()
    };

    if compact {
        expanded.replace(':', "")
    } else {
        expanded
    }
}

fn weekday_index(year: i32, month: u32, day: u32) -> usize {
    let days = days_from_civil(year, month, day);
    (days + 4).rem_euclid(7) as usize
}

fn weekday_name(year: i32, month: u32, day: u32) -> &'static str {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let idx = weekday_index(year, month, day);
    WEEKDAYS[idx]
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let mut y = i64::from(year);
    let m = i64::from(month);
    let d = i64::from(day);
    y -= if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_lto_positive_and_negative() {
        assert_eq!(format_lto(2), "+01:00");
        assert_eq!(format_lto(-3), "-01:30");
    }

    #[test]
    fn test_mjd_to_ymd_known_value() {
        assert_eq!(mjd_to_ymd(60000), (123, 2, 25));
    }

    #[test]
    fn test_apply_lto_offsets_across_midnight() {
        let utc = DabDateTime {
            year: 123,
            month: 2,
            day: 25,
            hour: 23,
            minute: 45,
            second: 0,
            ms: Some(321),
        };

        let local = apply_lto(&utc, 2);
        assert_eq!(local.year, 123);
        assert_eq!(local.month, 2);
        assert_eq!(local.day, 26);
        assert_eq!(local.hour, 0);
        assert_eq!(local.minute, 45);
        assert_eq!(local.second, 0);
        assert_eq!(local.ms, Some(321));
    }

    #[test]
    fn test_format_dab_datetime_human_and_iso8601() {
        let dt = DabDateTime {
            year: 123,
            month: 2,
            day: 25,
            hour: 12,
            minute: 34,
            second: 45,
            ms: Some(321),
        };

        assert_eq!(
            format_dab_datetime(&dt, true, false),
            "2023-02-25, Sat - 12:34:45.321"
        );
        assert_eq!(
            format_dab_datetime(&dt, false, false),
            "2023-02-25, Sat - 12:34:45"
        );
        assert_eq!(
            format_dab_datetime_iso8601(&dt, true, Some("Z"), false),
            "2023-02-25T12:34:45.321Z"
        );
        assert_eq!(
            format_dab_datetime_iso8601(&dt, false, Some("+01:00"), false),
            "2023-02-25T12:34:45+01:00"
        );
    }

    #[test]
    fn test_format_dab_datetime_time_only() {
        let dt = DabDateTime {
            year: 123,
            month: 2,
            day: 25,
            hour: 12,
            minute: 34,
            second: 45,
            ms: Some(321),
        };

        assert_eq!(format_dab_datetime(&dt, true, true), "12:34:45.321");
        assert_eq!(format_dab_datetime(&dt, false, true), "12:34:45");
        assert_eq!(
            format_dab_datetime_iso8601(&dt, true, Some("Z"), true),
            "12:34:45.321Z"
        );
        assert_eq!(
            format_dab_datetime_iso8601(&dt, false, Some("+01:00"), true),
            "12:34:45+01:00"
        );
    }

    #[test]
    fn test_format_dab_datetime_custom_tokens() {
        let dt = DabDateTime {
            year: 123,
            month: 2,
            day: 25,
            hour: 12,
            minute: 34,
            second: 45,
            ms: Some(321),
        };

        assert_eq!(
            format_dab_datetime_custom(
                &dt,
                Some("+01:00"),
                "YY YYYY M MM MMM MMMM D DD d dd ddd dddd H HH h hh m mm s ss SSS Z ZZ"
            ),
            "23 2023 2 02 Feb February 25 25 6 Sa Sat Saturday 12 12 12 12 34 34 45 45 321 +01:00 +0100"
        );
    }

    #[test]
    fn test_format_dab_datetime_custom_escape() {
        let dt = DabDateTime {
            year: 123,
            month: 2,
            day: 25,
            hour: 12,
            minute: 34,
            second: 45,
            ms: Some(321),
        };

        assert_eq!(
            format_dab_datetime_custom(&dt, Some("+00:00"), "[YYYYescape] YYYY-MM-DDTHH:mm:ssZ[Z]"),
            "YYYYescape 2023-02-25T12:34:45+00:00Z"
        );
    }
}
