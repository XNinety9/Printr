# Threat model — Printr

## What this project does

Printr turns an ESC/POS receipt printer (Epson TM-T88V) into a family gadget. It is a single Rust binary
(`printr`) that composes "tickets" from blocks (weather, puzzles, news, photos, AI-written text…),
renders them to ESC/POS and prints them over USB (`/dev/usb/lp0`), TCP or to a file.

`printr serve` runs a web app and an HTTP API, a scheduler and an optional MQTT client. It typically
runs on a Raspberry Pi on a home network, and is often exposed on the Internet behind a reverse proxy
(HTTPS). It is used by a family: a handful of accounts, one administrator.

Since 2.0 it also looks after its machine: a ready-to-flash Raspberry Pi image, a first network
connection through its own Wi-Fi hotspot, settings edited in the app, backup and restore, and signed
automatic updates. On the image it runs as the `printr` user under a hardened systemd unit
(`deploy/printr.service`: `ProtectSystem=strict`, writable `/opt/printr` for updates and its state
directory), and a polkit rule (`image/51-printr-network.rules`) lets that user drive NetworkManager and
start or stop `printr-hotspot.service`.

## Where untrusted input enters

- **HTTP (`src/server.rs`)**, the main surface:
  - unauthenticated: the login and the first-run setup (`/api/setup`, refused from public addresses);
    restoring a backup on a fresh installation with no account (`POST /api/restore`, refused from public
    addresses, like the setup); the Wi-Fi setup (`GET`/`POST /api/wifi`, see below);
  - authenticated web accounts (session cookie, argon2 passwords): compose, preview and print tickets,
    upload photos, saved tickets and schedules, shopping list, history, family birthdays
    (`/api/birthdays`); some routes are admin only: the settings (`/api/settings`, saved in the data
    file and applied by a restart; secrets are never sent back; `/etc/printr.env` wins and is
    read-only), backup and restore (`/api/backup`, `/api/restore`), printer discovery and the test
    ticket of the first-run wizard;
  - scripts with the API token (`Authorization: Bearer`, `PRINTR_TOKEN`): `/print`, `/todo`,
    `/api/*`.
  - Request bodies are capped (`MAX_JSON` 1 MiB, `MAX_IMAGE` 15 MiB, `MAX_BACKUP` 200 MiB for restores), tickets are capped at 60 blocks,
    prints are rate limited per person (`PRINTR_MAX_TICKETS_PER_HOUR`).
- **First network connection (`src/wifi.rs`, `image/`)**, only with `PRINTR_WIFI_SETUP=1` (set by the
  Raspberry Pi image) and only while the machine has no network: Printr opens a WPA2 hotspot
  (`Printr-XXXX`, hostapd and dnsmasq, address 10.42.0.1) whose password is printed on a ticket, so
  joining it means having the printer at hand. Every HTTP `GET` for another host is redirected to the
  setup page (captive portal). `POST /api/wifi` takes an SSID and a password from anyone on the hotspot
  or the local network, without an account, and passes them to `nmcli` (no shell, one argument per
  value). Scanned SSIDs from nearby networks are untrusted text shown in the web app and on tickets.
- **MQTT (`src/mqtt.rs`, `src/hass.rs`)**: anyone allowed to publish on the broker can print (tickets,
  text, saved tickets, shopping list, new paper roll). There is no token on MQTT by design: access is
  the broker's job. Retained Home Assistant discovery messages are read back from the broker
  (and erased when discovery is turned off).
- **Ticket JSON (`src/blocks/mod.rs`)**: from the web app, the API, MQTT and the CLI. Every block type
  parses its own fields (serde); free text, URLs, file paths, numbers and seeds come from the user.
- **Data fetched from the Internet by blocks**: RSS/Atom feeds, recipe web pages (schema.org JSON-LD),
  ICS calendars, images by URL, weather/air/sky APIs, the Lichess daily puzzle, GitHub tags and
  releases (update check).
  From the web app and the API, outgoing requests may only reach public addresses (`src/net.rs`,
  checked in the DNS resolver, redirects included) and local files are never read; from the CLI
  (`Ctx::new`), local files and addresses are allowed on purpose.
- **Uploaded and fetched images (`src/raster.rs`)**: decoded with dimension and memory limits.
- **Backup files (`src/backup.rs`)**: a single JSON file with accounts (argon2 hashes), tickets,
  schedules, history, settings and photos (base64 PNG). Restoring replaces all data; photo names are
  checked before being written next to the data file, and the current state is kept in `backups/`.
- **Updates (`src/update.rs`, `deploy/printr-update-guard.sh`)**: with `PRINTR_AUTO_UPDATE=auto`, the
  latest GitHub release is downloaded; its `SHA256SUMS` must carry a valid minisign signature from a key
  compiled into the binary (or `PRINTR_UPDATE_KEY`), the binary must match its checksum, run and report
  the expected version before it replaces `/opt/printr/printr`. A version that does not answer after
  restarting is rolled back by the guard script (run as `ExecStartPre`) and never retried. The signing
  key stays offline on the maintainer's machine: a compromised GitHub account must not be enough to
  ship code.
- **Printer discovery (`src/detect.rs`)**: the admin's wizard probes port 9100 on the machine's /24 and
  lists USB printers; the printer can then be set to a device under `/dev` or a `host:port`.
- **External programs**: Barnum (`PRINTR_BARNUM`, configured by the admin), the `fortune` command
  (from 1.11: fixed locations only, fixed argument, no shell), the Claude Code CLI (`PRINTR_CLAUDE_CLI`),
  `nmcli` and `systemctl` for the Wi-Fi setup. Their output is untrusted text.
- **AI output**: text written by Claude (horoscope, recipes, bedtime story, puzzle clues) is untrusted
  text that ends up on tickets and in the web app. The Messages API endpoint can be moved with
  `ANTHROPIC_BASE_URL` (a local inference server, for instance); it is deliberately not editable from
  the app, since the API key is sent there.
- **The printer link**: text sent to the printer must never carry ESC/POS commands (control characters
  are stripped in `src/cp858.rs`).

## Components that matter most

1. Authentication and authorization in `src/server.rs`, `src/store.rs` and `src/guard.rs`: sessions,
   password hashing, the setup route, admin-only routes, the API token, login rate limiting, the real
   client address behind the reverse proxy, cross-site request rejection, and the routes that work
   without an account on a fresh installation or during the Wi-Fi setup (`/api/setup`, `/api/restore`,
   `/api/wifi`).
2. The update path in `src/update.rs` and `deploy/printr-update-guard.sh`: signature and checksum
   checks, version check, rollback; anything that could make Printr install or run a binary that was not
   signed with the maintainer's key.
3. Settings and restored data in `src/settings.rs` and `src/backup.rs`: values that end up in the
   process environment (printer device, addresses, MQTT URL, Claude key) and in files.
4. SSRF protection in `src/net.rs` and every block that fetches a URL.
5. Parsing of untrusted documents: RSS, HTML/JSON-LD recipes, ICS calendars (with RRULE), images,
   backups.
6. Path handling: anything that turns user input into a file path (photos, data files, the cache, the
   printer device).
7. The Wi-Fi setup in `src/wifi.rs`: arguments passed to `nmcli`, the captive-portal redirect, the
   hotspot configuration written for hostapd.
8. The MQTT client and the web app (`web/app.js`, rendered with Preact/htm): XSS in anything that shows
   user, feed or AI text.

Less important: the CLI (`printr print`, `printr text`, `printr image`), which is run by someone who
already controls the machine.

## How to exercise it

- `cargo test` runs the unit tests (about 200), including the parsers, the network filter, the
  printer encoding, settings checks, backup parsing, update signature checks and `nmcli` output
  parsing. They pass offline in the scanner image.
- `printr --preview print examples/complet.json` renders a ticket in the terminal without a printer;
  `printr --dump out.bin print …` writes the ESC/POS bytes to a file.
- `printr --dump /tmp/out.bin serve` (with `PRINTR_DATA_DIR` set to an empty directory) starts the web
  app and the API on port 8080 without a printer; create the first account with `POST /api/setup`.
  The Wi-Fi setup needs NetworkManager and hostapd and is not exercised in the image;
  `scripts/labo-maj.sh` tests the whole update path with a local release server.
- `examples/` contains ticket JSON files; `docs/exemples/*.json` has one ticket per block.

## How we rate severity

- **Critical**: remote code execution, or taking over the administrator account, without an account;
  getting an unsigned binary installed by the automatic updates.
- **High**: authentication or authorization bypass (acting as another user, admin routes as a regular
  user, using the API without the token); SSRF reaching a private or local address from the web app or
  the API; reading or writing arbitrary files (including through settings or a restored backup);
  joining or reconfiguring the Wi-Fi without the hotspot password or local network access; command or
  argument injection into `nmcli`, `systemctl` or hostapd's configuration; stored XSS in the web app; sending raw ESC/POS commands
  to the printer from ticket content.
- **Medium**: denial of service by an authenticated user beyond the per-person quota (memory, CPU,
  disk), crashes reachable from fetched content (feeds, pages, calendars, images), information
  disclosure between accounts (another person's history or photos); secrets (API token, Claude key,
  MQTT URL, hotspot password) sent back to the app or to a non-admin account; breaking the update
  rollback so a bad release stays installed.
- **Low**: issues that need the API token, a broker account (MQTT) or the admin account; crashes from
  the CLI or from the admin's own configuration (`/etc/printr.env`).

## Out of scope / please do not report

- Anything requiring control of `/etc/printr.env`, the data directory or the machine, including
  `PRINTR_UPDATE_KEY` and `ANTHROPIC_BASE_URL`, which only the system can set.
- Restoring a backup on a fresh installation with no account from the local network, or using the Wi-Fi
  setup from the hotspot (password printed on the ticket) or the local network: intended, like the
  first-run setup. What a restored backup or the Wi-Fi setup can then do beyond that is in scope.
- The wizard's printer discovery probing the local /24 on port 9100: admin only, intended.
- MQTT accepting prints from any client allowed on the broker: this is the documented design.
- The CLI reading local files or local addresses: intended.
- Missing rate limits on the CLI, or the cost of AI calls configured by the admin.
- Dependency advisories already reported by `cargo deny` (checked in CI).

## Reports and patches

Reports in English or French. Patches as a minimal diff against `master`, in the style of the
surrounding code, with a unit test when possible.
