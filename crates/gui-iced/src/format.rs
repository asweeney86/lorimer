use std::time::Duration;

pub fn format_bytes(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut unit_index = 0;

    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{size} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

/// Formats a count with thousands separators, for example `1,234,567`.
pub fn format_count(value: u64) -> String {
    let digits = value.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(digit);
    }

    formatted
}

pub fn format_items(count: u64) -> String {
    if count == 1 {
        "1 item".to_string()
    } else {
        format!("{} items", format_count(count))
    }
}

pub fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f32();

    if seconds < 1.0 {
        format!("{} ms", duration.as_millis())
    } else if seconds < 60.0 {
        format!("{seconds:.1} s")
    } else {
        let whole = duration.as_secs();
        format!("{} min {} s", whole / 60, whole % 60)
    }
}

/// Share of `part` in `whole` as a fraction clamped to `0.0..=1.0`.
pub fn fraction(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        return 0.0;
    }

    (part as f64 / whole as f64).clamp(0.0, 1.0) as f32
}

pub fn format_percent(fraction: f32) -> String {
    let percent = fraction.clamp(0.0, 1.0) * 100.0;

    if percent > 0.0 && percent < 0.1 {
        "<0.1%".to_string()
    } else if percent >= 99.95 {
        "100%".to_string()
    } else {
        format!("{percent:.1}%")
    }
}

/// Shortens `value` to at most `max_chars` characters by replacing the middle with an ellipsis,
/// which keeps both the start of a name and its extension readable.
pub fn truncate_middle(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars || max_chars < 5 {
        return value.to_string();
    }

    let keep = max_chars - 1;
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let start: String = value.chars().take(head).collect();
    let end: String = value.chars().skip(count - tail).collect();

    format!("{}…{}", start.trim_end(), end.trim_start())
}

/// Turns an RFC 3339 timestamp such as `2026-09-26T22:22:30.84Z` into `26 Sep 2026 at 22:22`.
/// Unrecognized input is returned unchanged.
pub fn format_timestamp(value: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    let parsed = (|| {
        let year = value.get(0..4)?;
        let month: usize = value.get(5..7)?.parse().ok()?;
        let day: u32 = value.get(8..10)?.parse().ok()?;
        let time = value.get(11..16)?;
        let month = MONTHS.get(month.checked_sub(1)?)?;

        if value.as_bytes().get(10) != Some(&b'T') || !year.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }

        let zone = if value.ends_with('Z') { " UTC" } else { "" };
        Some(format!("{day} {month} {year} at {time}{zone}"))
    })();

    parsed.unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_binary_units_with_one_decimal() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn counts_get_thousands_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(3_889_163), "3,889,163");
    }

    #[test]
    fn items_are_pluralized() {
        assert_eq!(format_items(1), "1 item");
        assert_eq!(format_items(2400), "2,400 items");
    }

    #[test]
    fn durations_pick_a_readable_unit() {
        assert_eq!(format_duration(Duration::from_millis(420)), "420 ms");
        assert_eq!(format_duration(Duration::from_millis(2500)), "2.5 s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2 min 5 s");
    }

    #[test]
    fn fraction_handles_zero_and_overflow() {
        assert_eq!(fraction(5, 0), 0.0);
        assert_eq!(fraction(5, 10), 0.5);
        assert_eq!(fraction(20, 10), 1.0);
    }

    #[test]
    fn percent_handles_tiny_and_full_shares() {
        assert_eq!(format_percent(0.0), "0.0%");
        assert_eq!(format_percent(0.0004), "<0.1%");
        assert_eq!(format_percent(0.3214), "32.1%");
        assert_eq!(format_percent(1.0), "100%");
    }

    #[test]
    fn truncate_middle_keeps_both_ends() {
        assert_eq!(truncate_middle("short.txt", 20), "short.txt");

        let truncated = truncate_middle("a-very-long-file-name-for-testing.tar.gz", 16);
        assert_eq!(truncated.chars().count(), 16);
        assert!(truncated.starts_with("a-very-l"));
        assert!(truncated.ends_with(".tar.gz"));
        assert!(truncated.contains('…'));
    }

    #[test]
    fn truncate_middle_respects_multibyte_characters() {
        let truncated = truncate_middle("résumé-définitif-très-long.pdf", 12);
        assert!(truncated.chars().count() <= 12);
    }

    #[test]
    fn timestamps_become_readable() {
        assert_eq!(
            format_timestamp("2026-09-26T22:22:30.840608481Z"),
            "26 Sep 2026 at 22:22 UTC"
        );
        assert_eq!(format_timestamp("not a date"), "not a date");
        assert_eq!(
            format_timestamp("2026-13-01T00:00:00Z"),
            "2026-13-01T00:00:00Z"
        );
    }
}
