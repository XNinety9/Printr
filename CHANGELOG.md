# Changelog

All notable changes to Printr are listed here. Versions follow [semantic versioning](https://semver.org/);
each one is a `vX.Y.Z` tag on GitHub, which the server checks to tell the admin a new version is out.

Each minor version is named after a pioneer of printing, in alphabetical order (patch releases keep the
name): Aldus Manutius, Giambattista Bodoni, William Caslon, the Didot family, Robert Estienne,
Pierre-Simon Fournier, Johannes Gutenberg, Richard Hoe, Frederic Ives, Nicolas Jenson, Friedrich
Koenig, Tolbert Lanston…

## [Unreleased]

### Security
- Settings in a restored backup are now checked like those typed in the app: a crafted backup could
  set the printer device to any path the service can write to (such as the update guard script in
  `/opt/printr`). Invalid settings now make the restore fail, and are ignored at startup.

## [2.0.0] “Mergenthaler” - 2026-10-26

Printr 2.0 looks after itself: flash a card, plug in the printer, set up the Wi-Fi from your phone, and
everything else happens in the app, up to signed updates at night. No terminal needed any more.

### Upgrading from 1.x
- Run the full installation once (`scripts/deploy.sh HOST --install`, the archive's `install.sh`, or the
  new one-command install below): the service now runs the update guard before each start and may write
  to `/opt/printr`. A plain binary swap keeps the old service file, and automatic updates would not work.
- Data, accounts and `/etc/printr.env` are kept (and backed up first). What `/etc/printr.env` sets stays in
  charge and shows as read-only in the new settings screen; remove a line from it to manage that setting
  from the app instead.

### Added
**Install and set up without a terminal**
- **Ready-to-flash image for Raspberry Pi** (Pi 3, 4, 5 and Zero 2 W, 64-bit): Raspberry Pi OS Lite with
  Printr installed, the printer's udev rule, `http://printr.local` on port 80 and Barnum. Write it with
  Raspberry Pi Imager, plug in the printer, power on. Built with pi-gen (`image/build.sh`), attached to
  each release with `printr-imager.json`, which lists Printr in Imager from a custom content repository.
- **First network connection without a screen**: on the image, when no network shows up within a minute
  of boot, Printr opens a `Printr-XXXX` Wi-Fi hotspot and prints a ticket with QR codes to join it and open
  the setup page (phones open it by themselves, like a hotel Wi-Fi). Pick the home Wi-Fi, type its
  password: Printr joins it and prints the app's address. On a wrong password it reopens the hotspot and
  prints why; a known Wi-Fi missing after a power cut is retried every five minutes. The hotspot runs on
  hostapd (plain WPA2: NetworkManager's own hotspot is refused by phones on Raspberry Pi Wi-Fi chips);
  enabled by `PRINTR_WIFI_SETUP=1` (the image sets it).
- **Install in one command** on a machine already running:
  `curl -fsSL https://xninety9.github.io/Printr/install.sh | bash` downloads the archive for the machine,
  checks the release signature (with `openssl`) and the archive's checksum, and runs `install.sh`.
- **First-run wizard**: after the first account (where the language can now be picked), the admin is
  guided through finding the printer (USB, or network printers answering on port 9100), printing a test
  ticket and choosing the code page from how the accents came out, then the city, an optional Claude key
  and automatic updates.
- **Settings in the app** (My account → Settings, admin only): printer, language, city, weather units,
  Claude key, model and timeout, MQTT, API token, limits, update check and automatic updates. They are
  saved with the data and applied by a quick automatic restart (under systemd or Docker). Whatever
  `/etc/printr.env` sets still wins and is shown read-only; secrets are never sent back to the app.

**Look after itself**
- **Signed automatic updates** (opt-in, offered by the wizard): at night, Printr installs a new release only
  if its checksum list is signed with the maintainer's key (minisign, with an offline spare key) and the
  program matches, runs and reports the right version; if it does not answer after restarting, the
  service's guard puts the previous one back and never retries it. Releases now also carry the bare
  binaries (`printr-aarch64`, `printr-arm`, `printr-x86_64`); `scripts/signer-release.sh` signs a release,
  `scripts/labo-maj.sh` tests the whole path locally.
- **Backup and restore** from My account (admin): accounts, tickets, schedules, history, shopping list,
  family birthdays, settings and photos in one JSON file. Restoring keeps the current state in `backups/`
  first; a fresh installation can restore from the first account's screen (`GET /api/backup`,
  `POST /api/restore`).

**Blocks**
- **Date** (`date`): another day than today, as a set `date` or an `offset` in days, to print tickets ahead
  of a trip, one per day (#32, suggested by Fabwiss).
- **Family birthdays**: a shared list that everyone fills in from the Birthdays block, added to each
  block's own list (`family`, on by default; `GET`/`PUT /api/birthdays`) (#27, suggested by Fabwiss).
- **Claude blocks on any compatible endpoint**: `ANTHROPIC_BASE_URL` points the API path at another
  Messages API endpoint (a local inference server, for instance), and `PRINTR_CLAUDE_TIMEOUT_SECS` sets the
  call timeout (120 s by default) for slower local models (#26, by salazr).

### Changed
- **Weather** (`weather`): `wind` and `sun` hide the wind and UV line, and sunrise and sunset, to save paper
  (#29, suggested by Fabwiss); °F and mph with the "weather units" setting (`PRINTR_UNITS=imperial`),
  forecasts being still read in °C so the what-to-wear advice keeps working (discussion #34, suggested by
  mi3816).
- **Air quality** (`air_quality`): `pollutants` and `pollen` hide their lines (#30, suggested by Fabwiss).
- **The default city is used by every block with a place**: weather, air quality, sun and tonight's sky
  take the city from the settings when their own `location` is empty, which is now how the app adds them
  (it no longer fills in "Lyon"); the field shows the default city. With neither, the block says where to
  set one.
- With no `PRINTR_DEVICE`, Printr uses `/dev/tm88` (the link made by the udev rule) when it exists, then
  `/dev/usb/lp0`. `printr.env.example` no longer sets `PRINTR_DEVICE`, so new installs can pick the printer
  in the app; existing `/etc/printr.env` files are left as they are.
- The accent check of the test ticket leaves out `œ`, which the printer's code page prints as `oe`.

## [1.11.1] “Lanston” - 2026-10-25

### Added
- **`PRINTR_CODEPAGE=850`** for printers that lack the PC858 code page (`ESC t 19`), such as some MUNBYN clones, where
  accented letters were missing from tickets: Printr then selects PC850 (`ESC t 2`) and prints `€` as `EUR`
  (#31, by Fabwiss).

## [1.11.0] “Lanston” - 2026-10-23

### Added
- **Printr in a container**: `deploy/Dockerfile` and `deploy/compose.yaml` run the server next to Home
  Assistant & co (`docker compose up -d --build`), with Barnum, an unprivileged user and the safeguards of
  the systemd service. The container sees the host's `/dev` (opening only the printer), so a printer
  turned off and on again, or plugged in after startup, is still found. Both files ship in the release
  archives, next to the binary (#20, by Matteo Penaud).
- **Show the ticket configuration**, in the composer (admin only): the ticket as YAML for Home Assistant, ready to paste
  into an automation or a script — the `mqtt.publish` action on `printr/print` when Printr is connected to
  MQTT, or the `printr.print_ticket` action of the Printr integration — or as JSON for the HTTP API, MQTT
  and `printr print`. Copying also works over plain HTTP on the local
  network, where browsers refuse the clipboard API. `/api/status` gives the topic prefix (`mqtt_prefix`)
  (#21, by Matteo Penaud).
- **Fortune** (`fortune`): a fortune-cookie line (saying, proverb, silly prediction), offline, in French
  or English; `cow` makes the cowsay cow say it, in the printer's font. `source: system` uses the real
  `fortune` command of the server instead (`fortune-mod` package), run from its standard locations only,
  without a shell, with a time and length limit.

### Changed
- MQTT: with Home Assistant discovery turned off (`PRINTR_MQTT_DISCOVERY=off`), Printr now removes the
  device it had announced before, instead of leaving a ghost device next to another integration.
- Guide: a **Home Assistant** section of its own, with the two ways in (the
  [Printr integration](https://github.com/Matteo-Penaud/printr-ha-integration) by Matteo Penaud, or MQTT
  discovery) and how to pick one, plus ready-to-paste automations (doorbell, washing machine, the
  morning's sensors, paper almost out).
- Blocks page: each block's example JSON is always shown, with the values each choice field accepts
  (read from the code, in the page's language); every block now has an English example.
- Guide: the same photo printed with each dithering algorithm; a **Paper** section (prefer phenol-free
  rolls: BPA is banned in thermal paper in the EU, but often replaced by BPS); questions and printer
  feedback now go to GitHub Discussions. README: star history chart.
- `SECURITY.md`: how to report a vulnerability privately. Printr is enrolled for Anthropic's OSS Scanner
  (`.oss-scanner/`: build image and threat model).

### Security
- `ctutils` 0.4.5, used by `argon2` for the account passwords: constant-time comparisons could compile
  to branches (RUSTSEC-2026-0385).
- tokio 1.54.1 (1.54.0 was yanked).

## [1.10.0] “Koenig” - 2026-10-21

### Added
- Image block: choose the **dithering algorithm** — Floyd–Steinberg (default, smooth gradients for photos),
  Atkinson (more contrast, cleaner flat areas) or Bayer (a regular pattern without grain). In the web app,
  under the Dithering switch; in JSON, `dither_algorithm`; on the command line, `printr image --dither`.
  Existing tickets keep Floyd–Steinberg (#12).
- **Cover open and out of paper**, read from the printer itself (USB port status, a control request that
  cannot block the link, unlike ESC/POS status queries). The web app says which one it is (banner,
  sidebar, message after printing), tickets sent meanwhile wait in the queue right away instead of
  stalling for 30 s, and come out once when the printer is ready again. Also published as
  `printer_status` (`ready`, `cover_open`, `paper_out`, `missing`) in `/api/status`, on `printr/status`
  and in MQTT results. Tested on the TM-T88V.
- **Paper gauge**: Printr counts the paper used since the roll was put in (70 m by default) and warns
  with a banner when less than 10% is left. The gauge is under *My account*, with a **New roll** button
  (`POST /api/paper/roll`); it also resets by itself when the printer goes from “out of paper” to
  “ready”, and a whole roll used up sets the real roll length. Published as `paper` in `/api/status` and
  on `printr/status`.
- **Home Assistant discovery** over MQTT: Printr shows up by itself as a device, with sensors (printer
  status, problem, paper left in % and meters, paper almost out, queued tickets, last ticket), a New
  roll button, a Print a text field and one button per shared ticket, kept in sync as tickets change.
  `PRINTR_MQTT_DISCOVERY` sets the discovery prefix (`homeassistant` by default) or turns it `off`.
  New MQTT topic `printr/paper/roll`.

- **What's for dinner?** (`recipe_from_ingredients`): list the ingredients at hand, Claude writes a
  recipe that uses them, with pantry basics only, and an optional wish ("quick", "vegetarian"…).
- **Chores** (`chores`): who does what, chores rotating between people every week (or day), worked
  out from the date.
- **Chess** (`chess`): the Lichess daily puzzle, the board drawn from the side to play with the
  last move framed, and the answer in algebraic notation, upside down. Pieces from Noto Sans Symbols 2
  (SIL Open Font License).
- **Birthdays** (`birthdays`): "Happy birthday Leo!" with the age on the day, then a countdown to the
  next ones; one person per line, the year optional.
- **Bedtime story** (`story`): Claude makes up a gentle tale where the children are the heroes, on an
  optional theme, short or long; a new story every day, never the same title twice.
- Three new puzzles, generated on the Pi with a unique solution and the same number system as the
  sudoku: **Kakuro** (`kakuro`), **Takuzu** (`takuzu`, binairo) and **Hashi** (`hashi`, bridges).

### Changed
- **Weather with pictograms**: sun, clouds, fog, drizzle, rain, snow and storm, drawn on the Pi (no image
  to download). A large one for today, and the next days in columns: day, pictogram, high, low and
  chance of rain (#7).
- Weather: **what to wear** option (`outfit`), worked out from today's forecast without AI: layers for
  the morning and the afternoon, umbrella or raincoat when it's windy, boots in the snow, sun protection.
- The README is now a short overview; the full documentation moved to the guide (`docs/guide.md` and
  `docs/guide.fr.md`, rendered on the site), with a new **Supported printers** section.

### Fixed
- The live preview of the composer stopped as soon as a block was added with an empty required field
  (Claude recipe, recipe, calendar…): the whole ticket was refused. Now only that block shows an error.

### Security
- MQTT: switched from `rumqttc` to its maintained fork `rumqttc-v4-next` (still MQTT 3.1.1), which uses
  `rustls-webpki` 0.103. This fixes RUSTSEC-2026-0049, -0098, -0099 and -0104 (certificate revocation
  lists and name constraints, only relevant with `mqtts://`) and drops the unmaintained `rustls-pemfile`
  (RUSTSEC-2025-0134). The temporary exceptions in `deny.toml` are gone. Nothing changes in the
  configuration (#10).
- `rustls-webpki` 0.103.16 and `webpki-roots` 1.0.10: distrusted emSign certificates are no longer
  accepted (RUSTSEC-2026-0381, RUSTSEC-2026-0382).

## [1.9.0] “Jenson” - 2026-10-19

### Added
- Block catalog: **filters** at the top — for kids, offline and free, together, Claude. Search knows them too
  (“kids”, “free”, “together”…).
- The administrator can **hide blocks** from the whole family's catalog (My account → Offered blocks): the
  ones nobody uses. Tickets that already use them keep working.
- When Claude is not available (no API key, no Claude CLI), the blocks that cannot work without it — Claude
  horoscope, word of the day, Claude recipe — leave the catalog on their own. Crosswords and arrow words stay:
  their clues come from the cache.
- Catalog **favorites and recently added blocks follow your account**: you find them on every phone and
  computer. Those kept by the browser until now are moved into the account the first time.
- **Ticket templates**: a new ticket starts from a morning brief, a game break, a kids' ticket or a weekend
  ticket, to change afterwards — or from a first block, as before. Blocks hidden from the catalog are left out.
- **Node-RED**: the [`node-red-contrib-printr`](https://www.npmjs.com/package/node-red-contrib-printr)
  package prints tickets, to-do lists and saved tickets, adds to the shopping list and reads the printer's
  state from Node-RED flows, with only the Pi's address and token to set. See the README.
- **MQTT** (`PRINTR_MQTT_URL`, `mqtts://` for TLS): home automation systems print by publishing a ticket, a
  text, the name of a saved ticket, a to-do list or shopping items to `printr/…` topics, and get the outcome
  on `printr/result`. Printr publishes its state on `printr/status` and its presence on
  `printr/availability` (`offline` as last will). The HTTP rules apply: block limit, hourly quota, queue.
  `mqtts://` checks the broker certificate; `PRINTR_MQTT_CA` trusts a self-signed or private one.
- **Cut** block (`{"type": "cut"}`, alias `coupe`): cuts the paper in the middle of a ticket, so one print can
  come out as several pieces (a coupon to hand over, a list to share). The ticket's `spacing` sets the margin
  above the cut, the next piece starts at the top, and a ticket that ends with it is not cut twice. Shown as
  scissors in the terminal and app previews.

### Changed
- Block catalog: games are split into three groups — grids and numbers, words and letters, challenges and
  creativity.

### Fixed
- Block catalog: blocks no longer show above the search field while scrolling.

## [1.8.0] “Ives” - 2026-10-18

### Added
- **Printer missing? Tickets wait.** The server checks every 10 seconds that the printer is there (USB device
  present, network printer accepting connections). When it is not, tickets — already laid out — go into a
  queue that survives restarts, and print on their own when the printer comes back, schedules included.
  After 12 hours (`PRINTR_QUEUE_HOURS`), a waiting ticket is dropped and the history says why.
- The app shows the printer state (sidebar), a banner with the number of waiting tickets when it is missing,
  a “printer missing, the ticket will come out when it is back” message, and “waiting” / “came out at 9:12,
  late” in the history.
- **Printer stuck? No double ticket.** When the printer stops taking data (cover open, out of paper) for more
  than 30 seconds, the app says so (“cover, paper?”) instead of hanging. The ticket comes out once the printer
  is ready, only once, and the history then shows it as printed. Tickets sent in the meantime wait their turn
  and come out right after it, never mixed with it.
- The history refreshes on its own (every 10 seconds, when you come back to the tab, and when the printer
  state changes): a waiting ticket turns to “printed” without reloading the app.
- **Tonight's sky** block: visible planets with their direction and height, visible International Space Station
  passes until dawn, moon phase and shooting star showers, for your city. Computed locally (JPL orbital
  elements for the planets, SGP4 for the ISS, whose orbit comes from CelesTrak once a day).
- **Recipe** block: paste the address of a recipe, Printr reads its schema.org data (found on most cooking
  sites) and prints the title, servings and times, ingredients to tick and numbered steps, without the ads.
- **Claude recipe** block: name a dish, Claude (Haiku by default) writes the recipe at print time, with the
  same layout; cached per dish and number of servings.
- **Claude without an API key.** When `ANTHROPIC_API_KEY` is not set and the Claude Code CLI is installed,
  Claude blocks go through `claude -p` and its own login (a Claude subscription). `PRINTR_CLAUDE_BACKEND`
  forces `api` or `cli`, `PRINTR_CLAUDE_CLI` gives the program's path; the startup banner and `/api/status`
  (`claude_backend`) say which way is used.

### Changed
- The systemd service may now use up to 768 MB of memory (400 MB before), for the Claude CLI.
- `--preview` on the command line no longer calls Claude, like the app's preview: Claude blocks show a
  placeholder. `--claude` allows the call when you want the real text.
- `scripts/illustrations.sh`: remakes every illustration of the site and README in both languages (block
  examples, app screenshots, showcase tickets, link preview), then builds the site; `--sans-claude` makes no
  Claude call.

### Fixed
- The app recognizes French block names used in JSON tickets (`mots_meles`, `enigme`…) instead of showing them raw.

## [1.7.0] “Hoe” - 2026-10-15

### Added
- Mental math ([#4](https://github.com/XNinety9/Printr/issues/4)): a **beginner** level (sums up to 10,
  subtractions without negative results), a choice of **operations** (+ − × ÷) and of **times tables** to
  practice. Without options, sheets are exactly the same as before.
- **Crossword** block, generated by CrossWords like the arrow words: numbered cells and Across / Down clues in
  English, Roman-numeral rows and “Horizontalement / Verticalement” in French; clues from the CrossWords cache,
  missing ones written by Claude at print time.
- Block catalog: each block's **example ticket**, shown full size with the eye button before adding it,
  **favorites** (★) and **recently added** blocks at the top, and a search that ignores accents and knows
  words in both languages (“meteo” and “weather” both find the weather block).
- History: the administrator can **see each printed ticket** as it came out, for 30 days. Other accounts and
  the scripts token cannot.

- `scripts/appli-demo.sh`: the web app with fake accounts and tickets, no printer and no Claude, to try it
  out in one command.

### Fixed
- Ticket preview: full-width lines (such as the mental math score) are no longer clipped at the end.

## [1.6.1] “Gutenberg” - 2026-10-14

### Security
- rustls updated to 0.23.46, fixing four TLS client advisories (RUSTSEC-2026-0346 to 0349) affecting
  Printr's HTTPS requests (weather, news, Claude API, update check).

### Added
- Version codenames, shown by `printr --version` and in release titles.
- Apache License 2.0 (`LICENSE`), and a NOTICE crediting Printr's author.
- Continuous integration on every pull request: formatting, clippy, tests, web app syntax, shellcheck,
  dependency audit (cargo-deny) and workflow audit (zizmor); Dependabot for weekly dependency updates;
  issue and pull request templates.
- GitHub releases with prebuilt binaries for Raspberry Pi (64-bit, ARMv7, ARMv6) and x86_64 PCs, each
  archive ready to install with `sudo bash install.sh`, plus a `SHA256SUMS` file.

### Fixed
- English website: the example tickets on the home page and in the guide are now the English ones.
- The link preview image and text (Open Graph) are in English.

## [1.6.0] “Gutenberg” - 2026-10-13

### Added
- **English version**: `PRINTR_LANG=en` switches the web app, the command line, server messages and printed
  tickets to English. French remains the default.
- English content for the blocks that need it: riddles and charades, quotes, word search themes,
  workouts, gift coupons, Categories, secret messages, glitch messages.
- Claude writes the horoscope and the word of the day in English.
- Barnum horoscope in English, with Barnum 0.3 or later (`--lang en`). An older Barnum is asked again in
  French, and the whole ticket stays in French.
- Arrow words in English, with CrossWords 1.1.0: English dictionary, `definitions-en.txt` clues and a
  separate grid cache.
- English README and contributing guide (French versions in `README.fr.md` and `CONTRIBUTING.fr.md`);
  bilingual website, with English tickets and screenshots.

### Changed
- CrossWords dependency upgraded to 1.1.0 (JSON format version 2). French grids already cached remain valid.
- The news block also skips English “live”, “watch” and “opinion” items, in both languages.

## [1.5.0] “Fournier” - 2026-10-13

### Added
- Composer: reorder blocks by drag and drop with a handle, by finger or mouse, with auto-scroll near the
  screen edges ([#3](https://github.com/XNinety9/Printr/issues/3)).
- Composer: a “+” between two blocks inserts a new block right there.
- Keyboard: arrow keys on the handle move a block up or down.

### Fixed
- Date fields no longer overflow their block on iPhone.

## [1.4.0] “Estienne” - 2026-10-13

### Added
- Update notice: the server checks GitHub tags (daily by default, `PRINTR_UPDATE_INTERVAL` minutes
  otherwise) and tells the admin when a newer version is out. Nothing is downloaded or installed
  automatically; `PRINTR_UPDATE_CHECK=off` turns the check off.
- Website: a page showing every block with the ticket it prints and its JSON example.

### Fixed
- Website: block tickets shown at their real size, readable on desktop and phone.

## [1.3.0] “Didot” - 2026-10-13

### Added
- **Arrow words** block, generated by the [CrossWords](https://github.com/XNinety9/CrossWords) library:
  numbered grid, clues below, missing clues written by Claude at print time only.
- **Train tracks** puzzle block: link A to B with a single track, from the numbers on the edges.

### Note
- The `v1.2.0` tag was published before the train tracks block was merged and points to an incomplete
  version; 1.3.0 replaces it.

## [1.2.0] “Caslon” - 2026-10-13

Published before the train tracks block was merged: an incomplete version, replaced by 1.3.0.

### Added
- **Arrow words** block (see 1.3.0).

## [1.1.1] “Bodoni” - 2026-10-09

### Fixed
- The page scrolls again over the photo: cropping now happens in a dedicated window.
- The crop frame keeps its exact format on small screens.

## [1.1.0] “Bodoni” - 2026-10-09

### Added
- Crop the photo before printing (portrait, square or landscape; drag, pinch or zoom): only the framed
  area is printed.
- Version number shown in the app, and the logo next to the title on phones.

## [1.0.0] “Aldus” - 2026-10-08

First release.

- Single Rust binary for the Epson TM-T88V (native ESC/POS): command line, web server, scheduler and web app.
- Nearly forty blocks: weather, horoscope, sudoku, riddles, word searches, news from RSS feeds, photos,
  QR codes, shopping list, calendar…
- Family web app: accounts, saved and shared tickets, 24-hour scheduling, little notes with a photo.
- Hardened service, login rate limiting, admin account and per-person print quota.
- One-command installation on a Raspberry Pi, no sudo needed afterwards.
- Windows development with a dev container and a printing relay.

[Unreleased]: https://github.com/XNinety9/Printr/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/XNinety9/Printr/compare/v1.11.1...v2.0.0
[1.11.1]: https://github.com/XNinety9/Printr/compare/v1.11.0...v1.11.1
[1.11.0]: https://github.com/XNinety9/Printr/compare/v1.10.0...v1.11.0
[1.10.0]: https://github.com/XNinety9/Printr/compare/v1.9.0...v1.10.0
[1.9.0]: https://github.com/XNinety9/Printr/compare/v1.8.0...v1.9.0
[1.8.0]: https://github.com/XNinety9/Printr/compare/v1.7.0...v1.8.0
[1.7.0]: https://github.com/XNinety9/Printr/compare/v1.6.1...v1.7.0
[1.6.1]: https://github.com/XNinety9/Printr/compare/v1.6.0...v1.6.1
[1.6.0]: https://github.com/XNinety9/Printr/compare/v1.5.0...v1.6.0
[1.5.0]: https://github.com/XNinety9/Printr/compare/v1.4.0...v1.5.0
[1.4.0]: https://github.com/XNinety9/Printr/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/XNinety9/Printr/compare/v1.1.1...v1.3.0
[1.2.0]: https://github.com/XNinety9/Printr/releases/tag/v1.2.0
[1.1.1]: https://github.com/XNinety9/Printr/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/XNinety9/Printr/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/XNinety9/Printr/releases/tag/v1.0.0
