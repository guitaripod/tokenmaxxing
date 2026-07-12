//! Compact number formatting shared across the dashboard and share card.

/// A count as a short human string: `1234` → `1.2K`, `3_400_000` → `3.4M`.
pub fn count(n: u64) -> String {
    let value = n as f64;
    if value >= 1e12 {
        format!("{:.1}T", value / 1e12)
    } else if value >= 1e9 {
        format!("{:.1}B", value / 1e9)
    } else if value >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.1}K", value / 1e3)
    } else {
        n.to_string()
    }
}

/// Dollars, compacted for headline tiles: `$12.3K`, `$1.2M`, else two decimals.
pub fn usd(v: f64) -> String {
    let a = v.abs();
    if a >= 1e6 {
        format!("${:.2}M", v / 1e6)
    } else if a >= 10_000.0 {
        format!("${:.1}K", v / 1e3)
    } else if a >= 100.0 {
        format!("${:.0}", v)
    } else {
        format!("${v:.2}")
    }
}

/// Dollars always to the cent — for spend caps and precise figures.
pub fn usd_cents(v: f64) -> String {
    format!("${v:.2}")
}

/// A duration until `seconds` from now, as `3d 4h` / `5h 12m` / `9m`.
pub fn until(seconds: i64) -> String {
    let s = seconds.max(0);
    let (days, hours, minutes) = (s / 86_400, (s % 86_400) / 3_600, (s % 3_600) / 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// A percentage as `73%`.
pub fn percent(fraction: f64) -> String {
    format!("{}%", (fraction * 100.0).round() as i64)
}
