# Threat model — Printr

## What this project does

Printr turns an ESC/POS receipt printer (Epson TM-T88V) into a family gadget. It is a single Rust binary
(`printr`) that composes "tickets" from blocks (weather, puzzles, news, photos, AI-written text…),
renders them to ESC/POS and prints them over USB (`/dev/usb/lp0`), TCP or to a file.

`printr serve` runs a web app and an HTTP API, a scheduler and an optional MQTT client. It typically
runs on a Raspberry Pi on a home network, and is often exposed on the Internet behind a reverse proxy
(HTTPS). It is used by a family: a handful of accounts, one administrator.

## Where untrusted input enters

- **HTTP (`src/server.rs`)**, the main surface:
  - unauthenticated: the login and the first-run setup (`/api/setup`, refused from public addresses);
  - authenticated web accounts (session cookie, argon2 passwords): compose, preview and print tickets,
    upload photos, saved tickets and schedules, shopping list, history; some routes are admin only;
  - scripts with the API token (`Authorization: Bearer`, `PRINTR_TOKEN`): `/print`, `/todo`,
    `/api/*`.
  - Request bodies are capped (`MAX_JSON` 1 MiB, `MAX_IMAGE` 15 MiB), tickets are capped at 60 blocks,
    prints are rate limited per person (`PRINTR_MAX_TICKETS_PER_HOUR`).
- **MQTT (`src/mqtt.rs`, `src/hass.rs`)**: anyone allowed to publish on the broker can print (tickets,
  text, saved tickets, shopping list, new paper roll). There is no token on MQTT by design: access is
  the broker's job. Retained Home Assistant discovery messages are read back from the broker.
- **Ticket JSON (`src/blocks/mod.rs`)**: from the web app, the API, MQTT and the CLI. Every block type
  parses its own fields (serde); free text, URLs, file paths, numbers and seeds come from the user.
- **Data fetched from the Internet by blocks**: RSS/Atom feeds, recipe web pages (schema.org JSON-LD),
  ICS calendars, images by URL, weather/air/sky APIs, the Lichess daily puzzle, GitHub releases.
  From the web app and the API, outgoing requests may only reach public addresses (`src/net.rs`,
  checked in the DNS resolver, redirects included) and local files are never read; from the CLI
  (`Ctx::new`), local files and addresses are allowed on purpose.
- **Uploaded and fetched images (`src/raster.rs`)**: decoded with dimension and memory limits.
- **External programs**: Barnum (`PRINTR_BARNUM`, configured by the admin), the `fortune` command
  (from 1.11: fixed locations only, fixed argument, no shell), the Claude Code CLI (`PRINTR_CLAUDE_CLI`). Their
  output is untrusted text.
- **AI output**: text written by Claude (horoscope, recipes, bedtime story, puzzle clues) is untrusted
  text that ends up on tickets and in the web app.
- **The printer link**: text sent to the printer must never carry ESC/POS commands (control characters
  are stripped in `src/cp858.rs`).

## Components that matter most

1. Authentication and authorization in `src/server.rs`, `src/store.rs` and `src/guard.rs`: sessions,
   password hashing, the setup route, admin-only routes, the API token, login rate limiting, the real
   client address behind the reverse proxy, cross-site request rejection.
2. SSRF protection in `src/net.rs` and every block that fetches a URL.
3. Parsing of untrusted documents: RSS, HTML/JSON-LD recipes, ICS calendars (with RRULE), images.
4. Path handling: anything that turns user input into a file path (photos, data files, the cache).
5. The MQTT client and the web app (`web/app.js`, rendered with Preact/htm): XSS in anything that shows
   user, feed or AI text.

Less important: the CLI (`printr print`, `printr text`, `printr image`), which is run by someone who
already controls the machine.

## How to exercise it

- `cargo test` runs the unit tests (about 170), including the parsers, the network filter and the
  printer encoding.
- `printr --preview print examples/complet.json` renders a ticket in the terminal without a printer;
  `printr --dump out.bin print …` writes the ESC/POS bytes to a file.
- `printr --dump /tmp/out.bin serve` (with `PRINTR_DATA_DIR` set to an empty directory) starts the web
  app and the API on port 8080 without a printer; create the first account with `POST /api/setup`.
- `examples/` contains ticket JSON files; `docs/exemples/*.json` has one ticket per block.

## How we rate severity

- **Critical**: remote code execution, or taking over the administrator account, without an account.
- **High**: authentication or authorization bypass (acting as another user, admin routes as a regular
  user, using the API without the token); SSRF reaching a private or local address from the web app or
  the API; reading or writing arbitrary files; stored XSS in the web app; sending raw ESC/POS commands
  to the printer from ticket content.
- **Medium**: denial of service by an authenticated user beyond the per-person quota (memory, CPU,
  disk), crashes reachable from fetched content (feeds, pages, calendars, images), information
  disclosure between accounts (another person's history or photos).
- **Low**: issues that need the API token, a broker account (MQTT) or the admin account; crashes from
  the CLI or from the admin's own configuration (`/etc/printr.env`).

## Out of scope / please do not report

- Anything requiring control of `/etc/printr.env`, the data directory or the machine.
- MQTT accepting prints from any client allowed on the broker: this is the documented design.
- The CLI reading local files or local addresses: intended.
- Missing rate limits on the CLI, or the cost of AI calls configured by the admin.
- Dependency advisories already reported by `cargo deny` (checked in CI).

## Reports and patches

Reports in English or French. Patches as a minimal diff against `master`, in the style of the
surrounding code, with a unit test when possible.
