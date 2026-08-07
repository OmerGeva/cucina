# Security

## Reporting a vulnerability

Please report privately rather than opening a public issue, using
[GitHub's private advisory form](https://github.com/omergeva/cucina/security/advisories/new).
That reaches the maintainer directly and keeps the report confidential until
there is a fix to publish.

This is a personal project maintained in spare time. I will acknowledge reports as quickly
as I can, but cannot promise a fixed response window.

## What Cucina does by design

Some of this looks alarming out of context, so it is worth stating plainly. None of the
following are bugs — they are what the app is for — but they are worth understanding before
you install it.

**It runs arbitrary shell commands.** That is the entire feature. Each server is a command
string you supply, executed in your login shell with your environment and your permissions.
Anything that can edit Cucina's config can run code as you.

**Its config is a plain file.** Servers, commands and environment variables live in
`~/Library/Application Support/Cucina/servers.json`, readable and writable by your user. Environment
variables you set on a server are stored in plaintext — do not put production secrets there.

**Its control socket is not authenticated.** The app listens on a Unix domain socket at
`~/Library/Application Support/Cucina/cucina.sock`. Any process running as your user can connect and start,
stop or read the logs of any configured server. This is the same trust boundary as your
shell — file permissions are the control — but it does mean the CLI and MCP surface are
available to anything running as you, not only to the agent you intended.

**The MCP server gives coding agents that same reach.** An agent you connect to Cucina can
start and stop any configured server and read its output. It cannot create new servers or
change commands — `cucina_*` tools operate on servers you have already defined — but log
output can contain whatever your server prints, including tokens it logs.

**Releases are not notarized.** Builds are ad-hoc signed, so macOS cannot verify the
publisher and you are asked to clear the quarantine flag by hand. This means a downloaded
DMG carries no cryptographic assurance that it came from this repository. If that matters to
you, build from source — see [Installing](README.md#installing).

## What it does not do

- No telemetry, analytics, crash reporting or update checks. The app makes no network
  requests of its own.
- No data leaves your machine. Logs are held in a bounded in-memory ring buffer and are not
  written anywhere.
- Fonts are bundled, so no request goes out to a font CDN at runtime.
