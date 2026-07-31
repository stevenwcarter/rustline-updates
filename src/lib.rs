//! `updates` — a rustline widget counting pending package updates.
//!
//! Runs a configured checker (default `checkupdates`, from Arch's
//! pacman-contrib) through the host's capability-gated exec, at most once per
//! `refresh_secs`: the result is persisted via the sandboxed state capability
//! and reused until stale. Deliberately NOT `rl_exec_cached` — the host's
//! exec cache stores only zero-exit runs, and `checkupdates` exits 2 in the
//! everyday "no updates" case, which would re-spawn a network-hitting checker
//! on every render tick.
//!
//! Pure logic lives here and is unit-tested on the host target; the Extism
//! guest glue below only compiles for wasm32.

use serde::Deserialize;

/// This plugin's `[plugins.updates.options]` table.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Options {
    /// The checker program (argv[0]; no shell is involved).
    pub program: String,
    /// Its arguments, passed through verbatim.
    pub args: Vec<String>,
    /// How often to actually run the checker, in seconds.
    pub refresh_secs: i64,
    /// Exit statuses meaning "the checker ran; stdout is the update list".
    /// `checkupdates` exits 2 for "no updates" (default `[0, 2]`); apt wants
    /// `[0]`, dnf `[0, 100]`.
    pub ok_statuses: Vec<i32>,
    /// Render format. `{count}` is the number of pending updates.
    pub format: String,
    /// Click-toggle alternate view.
    pub alt_format: String,
    /// Shown when no usable count exists at all. Empty renders nothing.
    pub down_format: String,
    /// Shown when the count is 0. Empty (the default) renders nothing —
    /// the i3status/polybar convention. Set it to your `format` to show 0.
    pub zero_format: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            program: "checkupdates".to_string(),
            args: Vec::new(),
            refresh_secs: 3600,
            ok_statuses: vec![0, 2],
            format: "\u{f06b0} {count}".to_string(), // nf-md-update + space
            alt_format: String::new(),
            down_format: String::new(),
            zero_format: String::new(),
        }
    }
}

/// Number of pending updates: non-empty stdout lines.
pub fn count_updates(stdout: &str) -> usize {
    stdout.lines().filter(|l| !l.trim().is_empty()).count()
}

/// State file contents: `"<unix_ts> <count>"` on one line.
pub fn parse_state(contents: &str) -> Option<(i64, usize)> {
    let mut it = contents.split_whitespace();
    let ts = it.next()?.parse().ok()?;
    let count = it.next()?.parse().ok()?;
    Some((ts, count))
}

pub fn serialize_state(ts: i64, count: usize) -> String {
    format!("{ts} {count}\n")
}

/// A sample is fresh while `now` is within `ttl` after `ts`; a clock that
/// moved backwards counts as stale (mirrors the host caches' convention).
pub fn is_fresh(now: i64, ts: i64, ttl: i64) -> bool {
    now >= ts && now - ts < ttl
}

/// Which format renders a given count, honoring the hide-zero convention.
/// `None` means render nothing.
pub fn render_count(active_format: &str, zero_format: &str, count: usize) -> Option<String> {
    let f = if count == 0 {
        zero_format
    } else {
        active_format
    };
    (!f.is_empty()).then(|| f.replace("{count}", &count.to_string()))
}

/// RFC3339 (`context.now`) -> unix seconds, no chrono: fixed-position
/// date/time fields plus an optional fractional-seconds run and a
/// `Z`/`±HH:MM` offset, over Howard Hinnant's days-from-civil algorithm.
pub fn rfc3339_to_unix(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    if b[4] != b'-'
        || b[7] != b'-'
        || !(b[10] == b'T' || b[10] == b't' || b[10] == b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<i64> { s.get(r)?.parse().ok() };
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, min, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let rest = &s[19..];
    let rest = if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        &frac[digits..]
    } else {
        rest
    };
    let offset_secs: i64 = if rest.is_empty() || rest == "Z" || rest == "z" {
        0
    } else {
        let sign = match rest.as_bytes()[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let os = rest.get(1..)?;
        let oh: i64 = os.get(0..2)?.parse().ok()?;
        if os.as_bytes().get(2) != Some(&b':') {
            return None;
        }
        let om: i64 = os.get(3..5)?.parse().ok()?;
        sign * (oh * 3600 + om * 60)
    };

    // Days-from-civil (Hinnant): valid over the whole proleptic Gregorian
    // calendar with integer math only.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hour * 3600 + min * 60 + sec - offset_secs)
}

/// The capability manifest, embedded as the `rustline-manifest` custom
/// section so `rustline plugin approve updates` can read the requests straight
/// out of the distributed `.wasm` — `plugin install` downloads only this
/// binary, so a sidecar file would never reach the user's plugin dir.
#[cfg(target_arch = "wasm32")]
#[used]
#[unsafe(link_section = "rustline-manifest")]
pub static MANIFEST: [u8; include_bytes!("../manifest.toml").len()] =
    *include_bytes!("../manifest.toml");

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::*;
    use rustline_plugin_sdk::{
        GuestRender, LogLevel, Segment, active_format, exec, export_plugin, log, state_read,
        state_write,
    };

    /// State-file name under this plugin's sandboxed state dir.
    const STATE_FILE: &str = "last";

    fn render(input: &GuestRender) -> Vec<Segment> {
        let opts: Options = serde_json::from_value(input.config.clone()).unwrap_or_default();
        let Some(now) = rfc3339_to_unix(&input.context.now) else {
            log(LogLevel::Warn, "updates: could not parse context.now");
            return down_segment(&opts.down_format);
        };

        let persisted = state_read(STATE_FILE)
            .ok()
            .filter(|r| r.ok && r.exists)
            .and_then(|r| parse_state(&r.contents));

        let count = match persisted {
            Some((ts, c)) if is_fresh(now, ts, opts.refresh_secs) => Some(c),
            _ => match run_checker(&opts) {
                Some(c) => {
                    match state_write(STATE_FILE, &serialize_state(now, c)) {
                        Ok(w) if w.ok => {}
                        Ok(w) => log(
                            LogLevel::Warn,
                            &format!("updates: state write failed: {}", w.error),
                        ),
                        Err(e) => log(LogLevel::Warn, &format!("updates: state write failed: {e}")),
                    }
                    Some(c)
                }
                // The checker couldn't run: serve the stale count if any
                // (the host caches' serve-stale convention), else go down.
                None => persisted.map(|(_, c)| c),
            },
        };

        match count {
            Some(c) => {
                let format =
                    active_format(&input.context, "updates", &opts.format, &opts.alt_format);
                match render_count(format, &opts.zero_format, c) {
                    Some(text) => vec![Segment::new(text)],
                    None => Vec::new(),
                }
            }
            None => down_segment(&opts.down_format),
        }
    }

    /// Run the checker once. `Some(count)` iff it was allowed, spawned, and
    /// exited with a status in `ok_statuses`.
    fn run_checker(opts: &Options) -> Option<usize> {
        let args: Vec<&str> = opts.args.iter().map(String::as_str).collect();
        match exec(&opts.program, &args) {
            Ok(r) if r.ok && opts.ok_statuses.contains(&r.status) => {
                if r.truncated {
                    log(
                        LogLevel::Info,
                        &format!(
                            "updates: {} output truncated; count is a floor",
                            opts.program
                        ),
                    );
                }
                Some(count_updates(&r.stdout))
            }
            Ok(r) if r.ok => {
                log(
                    LogLevel::Warn,
                    &format!(
                        "updates: {} exited with unexpected status {}",
                        opts.program, r.status
                    ),
                );
                None
            }
            Ok(r) => {
                log(
                    LogLevel::Warn,
                    &format!(
                        "updates: {} denied or failed to run: {}",
                        opts.program, r.error
                    ),
                );
                None
            }
            Err(e) => {
                log(LogLevel::Warn, &format!("updates: host call failed: {e}"));
                None
            }
        }
    }

    fn down_segment(down_format: &str) -> Vec<Segment> {
        if down_format.is_empty() {
            Vec::new()
        } else {
            vec![Segment::new(down_format.to_string())]
        }
    }

    export_plugin!(name: "updates", render: render);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_non_empty_lines() {
        assert_eq!(count_updates(""), 0);
        assert_eq!(count_updates("linux 6.9 -> 6.10\n"), 1);
        assert_eq!(count_updates("a 1 -> 2\nb 3 -> 4\n\n"), 2);
    }

    #[test]
    fn state_round_trips() {
        let s = serialize_state(1785427200, 42);
        assert_eq!(parse_state(&s), Some((1785427200, 42)));
        assert_eq!(parse_state("garbage"), None);
        assert_eq!(parse_state(""), None);
    }

    #[test]
    fn freshness_window_and_backward_clock() {
        assert!(is_fresh(1000, 900, 3600));
        assert!(!is_fresh(5000, 900, 3600));
        assert!(!is_fresh(800, 900, 3600)); // clock went backwards -> stale
    }

    #[test]
    fn render_count_hides_zero_by_default() {
        assert_eq!(render_count("U {count}", "", 3), Some("U 3".to_string()));
        assert_eq!(render_count("U {count}", "", 0), None);
        assert_eq!(
            render_count("U {count}", "up to date", 0),
            Some("up to date".to_string())
        );
        assert_eq!(render_count("", "", 3), None);
    }

    #[test]
    fn rfc3339_epoch_and_offsets() {
        assert_eq!(rfc3339_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_to_unix("2026-07-30T16:00:00Z"), Some(1785427200));
        // Same instant expressed with a -04:00 offset.
        assert_eq!(
            rfc3339_to_unix("2026-07-30T12:00:00-04:00"),
            Some(1785427200)
        );
        // Fractional seconds are tolerated and ignored.
        assert_eq!(
            rfc3339_to_unix("2026-07-30T16:00:00.123Z"),
            Some(1785427200)
        );
    }

    #[test]
    fn rfc3339_rejects_garbage() {
        assert_eq!(rfc3339_to_unix(""), None);
        assert_eq!(rfc3339_to_unix("not a timestamp"), None);
        assert_eq!(rfc3339_to_unix("2026-13-01T00:00:00Z"), None);
    }
}
