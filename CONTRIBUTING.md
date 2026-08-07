# Contributing

Thanks for taking a look. Issues and pull requests are both welcome — including "this is
confusing" or "this broke on my machine", which are as useful as patches.

## Getting set up

You need [Rust](https://rustup.rs), [Node 22+](https://nodejs.org) (there is an `.nvmrc`)
and the Xcode command line tools. macOS only for now — see
[Platform support](README.md#platform-support).

```sh
npm install
npm run app     # the whole app, hot reloading
```

For UI work you usually do not need the Rust side at all:

```sh
npm run dev     # then open http://localhost:1420/preview.html
```

`preview.html` renders the real components against a stubbed Tauri bridge. `?at=settings`,
`?at=server:api` and `?at=add` open a specific screen so you can iterate on one without
clicking through to it.

## Checks

Run these before opening a pull request:

```sh
npm run build                                   # tsc --noEmit && vite build
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

CI runs exactly these on every pull request, on a macOS runner.

### About the supervisor tests

Most of the suite is ordinary unit tests, but two files drive real processes,
because what they check is about process groups and signals and there is nothing
left to test once those are stubbed out:

- `tests/supervisor.rs` starts real commands and asserts they — and anything they
  spawned — are gone after `stop`, `restart` and `shutdown`.
- `tests/crash_safety.rs` re-executes itself, `SIGKILL`s the copy, and asserts the
  servers died with it. A `SIGKILL`ed process runs no destructors, so only the pipe
  watchdog can pass this. It runs without the libtest harness because the process
  under test has to be the one that gets killed.

Both redirect `HOME` to a scratch directory, so running the suite never touches your
own `~/Library/Application Support/Cucina/servers.json`.

If you change anything in `supervisor.rs`, check the crash test still **fails** when
you break it deliberately — neuter the watchdog in `wrap()` and confirm it goes red.
A green suite that cannot fail is worse than no suite.

## How the code is written

The existing code has a consistent style, and matching it matters more than any rule below.
Read a neighbouring file before adding to it.

- **Comments explain why, not what.** If a line needs a comment to say what it does, rename
  something instead. The comments worth writing are the ones that stop someone "simplifying"
  a thing that is load-bearing — why the watchdog is a pipe rather than a signal handler,
  why `PATH` comes from an interactive shell, why the tray title is set to `Some("")` rather
  than `None`.
- **No dead abstraction.** There is no `utils.ts`. Things live next to what uses them.
- **Battery discipline is a feature.** No idle polling, no timers that run when nothing is
  happening, no looping animations. If you need to know something, ask once when it changes.
- **Design tokens over literals.** Colours, type and spacing come from the `:root` block in
  `src/styles.css`. Radius is 0 and there are no shadows — that is deliberate, not an
  oversight.

## Areas that would help

- **A Linux port.** The process supervision is ordinary Unix and should carry over; what
  needs work is abstracting the hardcoded macOS paths, tray integration and autostart.
- **Intel / universal builds** in the release workflow.
- **Signing and notarization**, if anyone has an Apple Developer ID to donate to the
  project — it is the single biggest friction for people installing this.
- **Tests for the IPC and MCP layers.** The supervisor is covered; the socket
  protocol and the MCP tool surface are not.

## Pull requests

- Branch from `master`, keep the change focused, and say what you changed and why.
- If it changes behaviour, say how you tested it. For anything touching the supervisor,
  please confirm servers still die when the app is `kill -9`'d.
- If it changes the UI, a before/after screenshot saves a lot of back and forth.

By contributing you agree that your work is licensed under the [MIT License](LICENSE).
