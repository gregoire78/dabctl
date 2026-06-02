use chrono::{FixedOffset, Locale, NaiveDate, TimeZone};
use std::env;
use std::str::FromStr;

fn current_date_locale() -> Locale {
    if let Ok(raw) = env::var("LC_TIME") {
        if let Some(locale) = parse_locale_tag(&raw) {
            return locale;
        }
    }

    if let Ok(raw) = env::var("LANG") {
        if let Some(locale) = parse_locale_tag(&raw) {
            return locale;
        }
    }

    if let Ok(raw) = env::var("LANGUAGE") {
        for candidate in raw.split(':') {
            if let Some(locale) = parse_locale_tag(candidate) {
                return locale;
            }
        }
    }

    Locale::POSIX
}

fn parse_locale_tag(raw: &str) -> Option<Locale> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let base = trimmed.split('.').next().unwrap_or(trimmed);
    let normalized = base.replace('-', "_");

    Locale::from_str(&normalized).ok().or_else(|| {
        let lang = normalized.split('@').next().unwrap_or(&normalized);
        let short = lang.split('_').next().unwrap_or(lang);
        if short.len() == 2 {
            let upper = short.to_ascii_uppercase();
            Locale::from_str(&format!("{}_{}", short, upper)).ok()
        } else {
            None
        }
    })
}

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
    format_dab_datetime_with_locale(dt, output_ms, time_only, current_date_locale())
}

fn format_time_component(dt: &DabDateTime, output_ms: bool) -> String {
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
}

fn format_dab_datetime_with_locale(
    dt: &DabDateTime,
    output_ms: bool,
    time_only: bool,
    locale: Locale,
) -> String {
    if time_only {
        return format_time_component(dt, output_ms);
    }

    let pattern = "%Y-%m-%d, %a - ";
    let base = format_dab_datetime_custom_with_locale(dt, None, pattern, locale);

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
        format_time_component(dt, output_ms)
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
    format_dab_datetime_custom_with_locale(dt, offset, pattern, current_date_locale())
}

fn format_dab_datetime_custom_with_locale(
    dt: &DabDateTime,
    offset: Option<&str>,
    pattern: &str,
    locale: Locale,
) -> String {
    let Some(rendered) = render_with_chrono(dt, offset, pattern, locale) else {
        return pattern.to_string();
    };

    rendered
}

fn render_with_chrono(
    dt: &DabDateTime,
    offset: Option<&str>,
    pattern: &str,
    locale: Locale,
) -> Option<String> {
    let year = dt.year + 1900;
    let date = NaiveDate::from_ymd_opt(year, u32::from(dt.month), u32::from(dt.day))?;
    let datetime = date.and_hms_milli_opt(
        u32::from(dt.hour),
        u32::from(dt.minute),
        u32::from(dt.second),
        u32::from(dt.ms.unwrap_or(0)),
    )?;

    let offset_colon = normalize_offset(offset);

    let sec = parse_offset_seconds(&offset_colon)?;
    let fixed = FixedOffset::east_opt(sec)?;
    let dt_fixed = fixed.from_local_datetime(&datetime).single()?;

    Some(dt_fixed.format_localized(pattern, locale).to_string())
}

fn normalize_offset(offset: Option<&str>) -> String {
    let off = offset.unwrap_or("+00:00");
    if off.eq_ignore_ascii_case("z") {
        "+00:00".to_string()
    } else if off.len() == 5 && (off.starts_with('+') || off.starts_with('-')) {
        format!("{}{}:{}", &off[0..1], &off[1..3], &off[3..5])
    } else {
        off.to_string()
    }
}

fn parse_offset_seconds(offset: &str) -> Option<i32> {
    if offset.len() != 6 {
        return None;
    }

    let sign = match &offset[0..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    if &offset[3..4] != ":" {
        return None;
    }

    let hours: i32 = offset[1..3].parse().ok()?;
    let minutes: i32 = offset[4..6].parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
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
            format_dab_datetime_with_locale(&dt, true, false, Locale::en_US),
            "2023-02-25, Sat - 12:34:45.321"
        );
        assert_eq!(
            format_dab_datetime_with_locale(&dt, false, false, Locale::en_US),
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

        assert_eq!(
            format_dab_datetime_with_locale(&dt, true, true, Locale::en_US),
            "12:34:45.321"
        );
        assert_eq!(
            format_dab_datetime_with_locale(&dt, false, true, Locale::en_US),
            "12:34:45"
        );
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
    fn test_format_dab_datetime_custom_chrono() {
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
                "%y %Y %-m %m %b %B %-d %d %w %a %A %-H %H %-I %I %-M %M %-S %S %3f %:z %z"
            ),
            "23 2023 2 02 Feb February 25 25 6 Sat Saturday 12 12 12 12 34 34 45 45 321 +01:00 +0100"
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
            format_dab_datetime_custom(&dt, Some("+00:00"), "YYYYescape %Y-%m-%dT%H:%M:%S%:zZ"),
            "YYYYescape 2023-02-25T12:34:45+00:00Z"
        );
    }

    #[test]
    fn test_render_with_chrono_french_locale_textual_names() {
        let dt = DabDateTime {
            year: 123,
            month: 2,
            day: 25,
            hour: 12,
            minute: 34,
            second: 45,
            ms: Some(321),
        };

        let rendered = render_with_chrono(&dt, Some("+01:00"), "%a %A %b %B", Locale::fr_FR)
            .expect("localized rendering");
        assert_eq!(rendered, "sam. samedi f\u{e9}vr. f\u{e9}vrier");
    }

    #[test]
    fn test_format_dab_datetime_human_french_locale() {
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
            format_dab_datetime_with_locale(&dt, true, false, Locale::fr_FR),
            "2023-02-25, sam. - 12:34:45.321"
        );
    }

    #[test]
    fn test_parse_locale_tag_variants() {
        assert_eq!(parse_locale_tag("fr_FR.UTF-8"), Some(Locale::fr_FR));
        assert_eq!(parse_locale_tag("fr-FR"), Some(Locale::fr_FR));
        assert_eq!(parse_locale_tag("de"), Some(Locale::de_DE));
    }
}
