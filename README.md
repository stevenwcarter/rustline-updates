# rustline-updates

A count of pending package updates in your tmux status line, as a
[rustline](https://github.com/stevenwcarter/rustline) WASM plugin.

Runs a configured checker (default `checkupdates`, from Arch's
pacman-contrib) through rustline's capability-gated exec host function, and
persists the result via the sandboxed state capability so the checker runs at
most once per `refresh_secs`. This is deliberately plain `exec` plus a
state-backed TTL, not the host's `exec_cached`: that cache stores only
zero-exit runs, and `checkupdates` exits 2 in the everyday "no updates" case,
which would defeat the cache and re-spawn a network-hitting checker on every
render tick.

## Install

    rustline plugin install stevenwcarter/rustline-updates --name updates
    rustline plugin approve updates      # grants: checkupdates

(`--name updates` is only needed on rustline v0.1.0, whose default install
name came from the repo; newer builds derive it from the release asset.) The
`allowed_commands` warning `approve` prints — that this grants a real program
run with your own environment and permissions — is expected; the manifest
only ever requests `checkupdates`.

Place it:

    rustline widget enable updates

## Configure

    [plugins.updates.options]
    # program = "checkupdates"   # Arch (pacman-contrib)
    # args = []
    # refresh_secs = 3600
    # ok_statuses = [0, 2]       # checkupdates: 2 = "no updates"
    # format = "󰚰 {count}"
    # zero_format = ""           # "" hides the widget when up to date
    # alt_format = ""
    # down_format = ""

`checkupdates` is Arch-specific; other distros need a different `program`/
`args`/`ok_statuses`, each requiring its own `allowed_commands` grant (the
default manifest only requests `checkupdates` — anything else needs `rustline
plugin cmd add updates "<pattern>"`, matched against the whole argv, not just
the program name):

| Distro | `program` / `args` | `ok_statuses` | grant (`plugin cmd add updates …`) |
|---|---|---|---|
| Arch | `checkupdates` | `[0, 2]` | `checkupdates` (via `approve`) |
| Debian/Ubuntu | `apt` / `["list", "--upgradable"]` | `[0]` | `apt list --upgradable` |
| Fedora | `dnf` / `["check-update", "-q"]` | `[0, 100]` | `dnf check-update -q` |
| Homebrew | `brew` / `["outdated"]` | `[0]` | `brew outdated` |

Note: apt's output includes a `Listing...` header line, so the count is one
higher than the real number of upgradable packages; a `WARNING` line from apt
lands on stderr, not stdout, so it doesn't affect the count.

## Building from source

    cargo build --release --target wasm32-unknown-unknown
    cp target/wasm32-unknown-unknown/release/updates.wasm ~/.local/share/rustline/plugins/

The capability manifest (`manifest.toml`) is embedded into the `.wasm` as the
`rustline-manifest` custom section at build time, so `plugin approve` works
on the distributed binary alone. This repo doubles as a worked example of a
standalone (out-of-tree) rustline plugin: scaffolded with `rustline plugin
new`, SDK via a git dependency pinned to a rustline release tag, CI + tag
releases publishing the installable `.wasm`.

## License

MIT
