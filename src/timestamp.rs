use chrono::{
    DateTime, Duration, FixedOffset, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc,
};
use chrono_tz::Tz;

use crate::models::{TimestampConfig, TimestampFormatType, TimezoneModeType};

pub fn parse_timestamp_to_utc(
    raw_value: &str,
    config: &TimestampConfig,
) -> Result<DateTime<Utc>, String> {
    let value = raw_value.trim();
    if value.is_empty() {
        return Err("Timestamp value is empty.".to_string());
    }

    match config.format {
        TimestampFormatType::Iso8601 => parse_iso8601_to_utc(value),
        TimestampFormatType::Naive | TimestampFormatType::Custom => {
            let naive = parse_naive_timestamp(value, config)?;
            localize_naive_to_utc(naive, config)
        }
    }
}

pub fn validate_fixed_offset(value: &str) -> Result<FixedOffset, String> {
    let clean = value.trim();
    let bytes = clean.as_bytes();

    let (hours_str, minutes_str) = match bytes.len() {
        5 if bytes[3] != b':' => (&clean[1..3], &clean[3..5]),
        6 if bytes[3] == b':' => (&clean[1..3], &clean[4..6]),
        _ => return Err(invalid_offset_message(clean)),
    };

    let sign = match bytes[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return Err(invalid_offset_message(clean)),
    };

    let hours = hours_str
        .parse::<i32>()
        .map_err(|_| invalid_offset_message(clean))?;
    let minutes = minutes_str
        .parse::<i32>()
        .map_err(|_| invalid_offset_message(clean))?;

    if hours > 14 || minutes >= 60 || (hours == 14 && minutes != 0) {
        return Err(invalid_offset_message(clean));
    }

    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
        .ok_or_else(|| invalid_offset_message(clean))
}

fn parse_iso8601_to_utc(value: &str) -> Result<DateTime<Utc>, String> {
    let normalized = normalize_iso8601(value);
    if let Ok(parsed) = DateTime::parse_from_rfc3339(&normalized) {
        return Ok(parsed.with_timezone(&Utc));
    }

    let naive = parse_flexible_naive(value)
        .ok_or_else(|| format!("Couldn't parse timestamp '{value}' as ISO8601."))?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn parse_naive_timestamp(value: &str, config: &TimestampConfig) -> Result<NaiveDateTime, String> {
    match config.format {
        TimestampFormatType::Custom => {
            let format = config.custom_format.as_deref().ok_or_else(|| {
                "Custom timestamp formats require a customFormat value.".to_string()
            })?;
            parse_naive_with_format(value, format).ok_or_else(|| {
                format!("Couldn't parse timestamp '{value}' with format '{format}'.")
            })
        }
        TimestampFormatType::Naive => parse_flexible_naive(value)
            .ok_or_else(|| format!("Couldn't parse timestamp '{value}' as a naive timestamp.")),
        TimestampFormatType::Iso8601 => unreachable!(),
    }
}

fn parse_naive_with_format(value: &str, format: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, format)
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(value, format)
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
        })
}

fn parse_flexible_naive(value: &str) -> Option<NaiveDateTime> {
    for format in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %H:%M",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
        "%m/%d/%Y",
        "%Y/%m/%d",
        "%Y-%m-%d",
    ] {
        if let Some(parsed) = parse_naive_with_format(value, format) {
            return Some(parsed);
        }
    }

    None
}

fn localize_naive_to_utc(
    naive: NaiveDateTime,
    config: &TimestampConfig,
) -> Result<DateTime<Utc>, String> {
    match config.timezone_mode {
        TimezoneModeType::Utc | TimezoneModeType::EmbeddedOffset => {
            Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
        }
        TimezoneModeType::FixedOffset => {
            let offset = validate_fixed_offset(config.timezone.as_deref().ok_or_else(|| {
                "Timezone is required when using fixedOffset or daylightSavings timestamp modes."
                    .to_string()
            })?)?;
            let localized = offset
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| format!("Couldn't localize timestamp '{naive}' with offset."))?;
            Ok(localized.with_timezone(&Utc))
        }
        TimezoneModeType::DaylightSavings => {
            let timezone_name = config.timezone.as_deref().ok_or_else(|| {
                "Timezone is required when using fixedOffset or daylightSavings timestamp modes."
                    .to_string()
            })?;
            let timezone = timezone_name
                .parse::<Tz>()
                .map_err(|_| format!("Invalid IANA timezone '{timezone_name}'."))?;
            localize_with_timezone_shift_forward(timezone, naive)
                .map(|value| value.with_timezone(&Utc))
        }
    }
}

fn localize_with_timezone_shift_forward(
    timezone: Tz,
    naive: NaiveDateTime,
) -> Result<DateTime<Tz>, String> {
    let mut candidate = naive;
    for _ in 0..180 {
        match timezone.from_local_datetime(&candidate) {
            LocalResult::Single(value) => return Ok(value),
            LocalResult::Ambiguous(_, latest) => return Ok(latest),
            LocalResult::None => {
                candidate += Duration::minutes(1);
            }
        }
    }

    Err(format!(
        "Couldn't localize timestamp '{naive}' in timezone '{}'.",
        timezone
    ))
}

fn normalize_iso8601(value: &str) -> String {
    let mut normalized = value.trim().to_string();
    if normalized.contains(' ') && !normalized.contains('T') {
        normalized = normalized.replacen(' ', "T", 1);
    }

    if let Some(stripped) = normalized.strip_suffix('Z') {
        return format!("{stripped}Z");
    }

    if normalized.len() >= 5 {
        let suffix = &normalized[normalized.len() - 5..];
        if (suffix.starts_with('+') || suffix.starts_with('-'))
            && suffix[1..].chars().all(|char| char.is_ascii_digit())
        {
            return format!(
                "{}{}:{}",
                &normalized[..normalized.len() - 5],
                &suffix[..3],
                &suffix[3..]
            );
        }
    }

    normalized
}

fn invalid_offset_message(value: &str) -> String {
    format!(
        "Invalid timestamp UTC offset '{value}'. UTC offsets must be specified in ±HHMM or ±HH:MM format (e.g: '-0700' or '-07:00') with hours between 00 and 14 and minutes between 00 and 59."
    )
}
