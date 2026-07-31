//! `updates` rustline plugin.
//!
//! TODO: describe what this widget renders.
//!
//! Pure logic lives here and is unit-tested on the host target (`cargo
//! test`); the Extism guest glue below only compiles for wasm32. See
//! `plugins/weather` in the rustline repo for a complete worked example
//! (an HTTP fetch through a host capability, TTL caching, click-toggle
//! support).

/// Substitute the `{name}` placeholder in `format` with this plugin's own
/// name. TODO: replace with your widget's real formatting logic.
pub fn render_format(format: &str) -> String {
    format.replace("{name}", "updates")
}

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::*;
    // One crate for everything a guest needs: the wire types, the typed
    // host-capability wrappers, the click-toggle helper, and `export_plugin!`.
    use rustline_plugin_sdk::{GuestRender, Segment, export_plugin};

    // The host capabilities are plain functions on the SDK — no `extern`
    // block, no JSON walking. Each is gated by this plugin's own grants in
    // config.toml — `allowed_urls` for the http_* calls, `allowed_paths`
    // (read-only) for file_read, `allowed_write_paths` (write-only, empty by
    // default) for file_write, and `max_state_bytes` for the state_* calls —
    // plus `resolve_symlinks` (default off) if either allowlist should follow
    // symlinks — and returns a typed result (see the SDK docs for the full set):
    //
    //   rustline_plugin_sdk::http_get(url)                  -> Result<HttpResult, _>
    //   rustline_plugin_sdk::http_get_cached(url, ttl, now) -> Result<CachedHttpResult, _>
    //   rustline_plugin_sdk::state_read(relpath)            -> Result<ReadResult, _>
    //   rustline_plugin_sdk::state_write(relpath, contents) -> Result<WriteResult, _>
    //   rustline_plugin_sdk::file_read(path)                -> Result<ReadResult, _>   // allowed_paths
    //   rustline_plugin_sdk::file_write(path, contents)     -> Result<WriteResult, _>  // allowed_write_paths
    //   rustline_plugin_sdk::log(level, msg)                -> ()   // capability-free
    //
    // `export_plugin!` wires the `name()`, `render()`, and `abi_version()`
    // Extism exports; `render` just takes the typed input and returns segments.
    fn render(input: &GuestRender) -> Vec<Segment> {
        // TODO: read your plugin's own options out of `input.config` (an
        // opaque JSON object forwarded verbatim from
        // `[plugins.updates.options]` in config.toml), and/or fields off
        // `input.context` (a `WireContext` — loadavg, battery, cpu, memory,
        // toggled widgets, theme colors, and more; see the SDK's re-exported
        // `rustline_abi` types).
        let format = input
            .config
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("updates: hello!");

        vec![Segment::new(render_format(format))]
    }

    export_plugin!(name: "updates", render: render);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_format_substitutes_name() {
        assert_eq!(render_format("hello {name}"), "hello updates");
    }
}
