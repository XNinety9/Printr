# Contributing to Printr

[🇫🇷 Version française](CONTRIBUTING.fr.md)

Thanks for your interest! This guide explains how to get the project running, how it is organized,
and how to propose a change. You don't need a printer: an emulator and a preview in the terminal
are enough for almost everything.

## Getting started

You need a stable Rust toolchain (`rustup`). For the emulator and the screenshots you also need
[`uv`](https://docs.astral.sh/uv/), and Chromium for the web interface screenshots.

```sh
git clone https://github.com/XNinety9/Printr && cd Printr
cargo test                                               # all tests, no network or printer needed
cargo run -- --preview print examples/complet.json       # preview of the full ticket in the terminal
scripts/appli-demo.sh                                    # the web app with fake data, no printer and no Claude
```

A few useful environment variables:

| Variable | What it does |
| --- | --- |
| `ANTHROPIC_API_KEY` | Claude blocks (`horoscope`, `word_of_the_day`); without it, the Claude Code CLI is used if installed (`PRINTR_CLAUDE_BACKEND`, `PRINTR_CLAUDE_CLI`). Optional: without it, these blocks show an error and the rest of the ticket prints normally. `--preview` never calls Claude (Claude blocks show a placeholder); add `--claude` to get the real text. |
| `PRINTR_BARNUM` | Command for [Barnum](https://github.com/XNinety9/Barnum) used by the `barnum` block, for example `python3 ../Barnum/main.py`. |
| `PRINTR_DATA_DIR` | Accounts, presets and history of the web interface. Use a throwaway folder for your tests. |
| `PRINTR_GLITCH` | How often glitches happen (one print in N, 20 by default, 0: never). |
| `PRINTR_CACHE_DIR` | Cache of the day's content. |
| `PRINTR_WEB_DIR=web` | Serves the interface from disk: edit `web/` without recompiling. |
| `PRINTR_DEBUG=1` | Shows Claude's raw responses and the news ranking. |

## Testing without a printer

- **Terminal preview**: `--preview` shows the ticket inside a frame, with bold, reverse and underlined text.
- **Emulator**: `scripts/demo.sh [ticket.json]` prints to the
  [emupos](https://pypi.org/project/emupos/) emulator (TM-T88V profile in `emulator/`) and gives
  you the PNG of the ticket, pixel for pixel.
- **Raw bytes**: `--dump sortie.bin` writes exactly what would be sent to the printer.
- **Block examples**: `docs/exemples/generer.sh [bloc…]` re-prints the examples in
  `docs/exemples/` through the emulator, with dummy data. Don't forget to add your block there.
- **Web interface**:

  ```sh
  export PRINTR_DATA_DIR=/tmp/printr-dev PRINTR_WEB_DIR=web
  echo motdepasse | cargo run -- user add Test
  cargo run -- --dump /tmp/sortie.bin serve --listen 127.0.0.1:8080
  ```

Claude blocks cost money on every generation. To iterate, prefer the web interface preview, which
never calls Claude, or the day's cache: without `--refresh`, a horoscope already generated today
is reused as is.

## Code layout

| File | Role |
| --- | --- |
| `src/main.rs` | Command line (clap), help text in French |
| `src/blocks/mod.rs` | The ticket JSON format, the `Block` enum, parallel construction |
| `src/blocks/*.rs` | One file per block with some substance (weather, sudoku, Barnum…) |
| `src/doc.rs` | Intermediate representation of a ticket: styled lines, images, QR codes, previews |
| `src/cp858.rs` | Text encoding for the printer (PC858 code page) |
| `src/raster.rs`, `src/draw.rs` | Images: dithering, drawings (sudoku, maze, side-by-side QR codes) |
| `src/output.rs` | Sending to the USB printer, over TCP, or to a file |
| `src/server.rs`, `src/store.rs`, `src/scheduler.rs` | Web server, data (accounts, presets, history), scheduler |
| `src/claude.rs`, `src/cache.rs` | Claude API client, cache of the day's content |
| `src/ui.rs` | Console output: progress, summary, errors, server log |
| `web/` | Web interface (Preact + htm, no build step), embedded in the binary |
| `docs/` | Showcase site (GitHub Pages) and its illustrations |

## Developing a new block

A block is an entry in a ticket's JSON (`{ "type": "…", … }`) that turns into a piece of ticket.
To develop one, we always follow the same path, illustrated here with an example block, "Dice",
which rolls a few dice and shows their total:

```json
{ "type": "dice", "count": 3 }
```

### 1. Declare the block and its parameters

In `src/blocks/mod.rs`, add a variant to the `Block` enum. Its name, in `snake_case`, gives the
JSON `type`; its fields are the parameters.

```rust
    /// Dice roll.
    Dice {
        /// Number of dice, from 1 to 6.
        #[serde(default = "two")]
        count: u8,
    },
```

- Give a default value (`#[serde(default)]`, or a function such as `two`) to anything that isn't
  essential: a minimal ticket must work.
- An unknown parameter is refused (`deny_unknown_fields`), which catches typos.
- For a list of choices, declare an `enum` with `#[serde(rename_all = "snake_case")]` and French
  aliases (`#[serde(alias = "facile")]`), as `sudoku::Difficulty` does.

### 2. Write the rendering

Create `src/blocks/dice.rs`. Its `build` function receives the parameters and returns a `Doc`,
the representation of the ticket before printing:

```rust
//! Dice roll: one or more six-sided dice, and their total.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::doc::{Doc, Style};

/// Rolls `count` dice (from 1 to 6 dice).
fn roll(count: u8, rng: &mut StdRng) -> Vec<u8> {
    (0..count.clamp(1, 6)).map(|_| rng.random_range(1..=6)).collect()
}

pub fn build(count: u8) -> Doc {
    let rolls = roll(count, &mut StdRng::from_rng(&mut rand::rng()));
    let faces: Vec<String> = rolls.iter().map(u8::to_string).collect();
    let total: u32 = rolls.iter().map(|&r| u32::from(r)).sum();

    let mut doc = Doc::new();
    doc.header("Dice roll");
    doc.feed(1);
    doc.text(&faces.join("  "), Style::default().bold().center().size(2));
    doc.text(&format!("Total: {total}"), Style::default().center());
    doc
}
```

Toolbox of `Doc` (`src/doc.rs`):

| Method | Effect |
| --- | --- |
| `header("Title")` | Reversed banner (white on black) across the full width |
| `text(text, style)` | Word-wrapped text; an empty line skips a line |
| `hanging("Prefix: ", text, style)` | Text whose following lines align after the prefix |
| `line(text, style)` | One line as is, spaces kept: ideal for columns |
| `rule('─')`, `feed(n)` | Separator line, blank lines |
| `image(img)`, `qr(data, size)` | Image 512 dots wide, QR code |

And for `Style`: `bold()`, `underline()`, `reverse()`, `small()` (font B), `size(1..=8)`,
`center()`, `align(…)`, `upside_down()`. The paper is 42 columns wide in normal font, 56 in small
font, and 42 divided by the size when enlarged (21 columns at size 2).

For an image (drawing, grid, photo), see `draw.rs` and `raster.rs`: `draw::canvas(height)` gives a
white canvas 512 dots wide, `draw::fill_rect` draws on it, and `raster::prepare` dithers a photo.

### 3. Wire up the block

Still in `src/blocks/mod.rs`, three additions:

```rust
mod dice;                                          // at the top of the file

Block::Dice { .. } => "dice",                      // in Block::name()

Block::Dice { count } => doc = dice::build(*count), // in Block::build()
```

If a parameter usefully identifies the block (a city, a sign…), also add it to `Block::label()`:
it then appears in the console ("weather · Lyon"), in the history and in errors.

### 4. Try it

```sh
echo '{"blocks":[{"type":"dice","count":3}]}' | cargo run -- --preview print
```

```
╭──────────────────────────────────────────────────────────╮
│                        DICE ROLL                         │
│                                                          │
│                        1   4   2                         │
│                         Total: 7                         │
╰╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ ✂ ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╯
```

Then the real rendering, pixel for pixel, in the emulator:

```sh
echo '{"blocks":[{"type":"dice","count":3}]}' > /tmp/des.json && scripts/demo.sh /tmp/des.json
```

### 5. Test

Add tests at the bottom of the file. Isolate whatever is random or networked so that it can be
tested deterministically, here with a fixed seed:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_are_valid_dice() {
        let mut rng = StdRng::seed_from_u64(42);
        for count in [0, 1, 3, 6, 9] {
            let rolls = roll(count, &mut rng);
            assert_eq!(rolls.len(), count.clamp(1, 6) as usize);
            assert!(rolls.iter().all(|r| (1..=6).contains(r)));
        }
    }
}
```

### 6. Add it to the web interface

In `web/app.js`, add an entry to the `BLOCKS` catalog. The form fields are generated from `fields`:

```js
  {
    type: 'dice', label: 'Dice', emoji: '🎲', group: G.fun, desc: 'A dice roll and its total',
    defaults: { count: 2 },
    fields: [{ key: 'count', label: 'Number of dice', kind: 'number', min: 1, max: 6 }],
    summary: (b) => `${b.count} die${b.count > 1 ? 's' : ''}`,
  },
```

Available field types (`kind`): `text`, `textarea`, `number`, `select`, `segmented` (side-by-side
buttons), `toggle`, `date`, `list` (a list of texts), `icons` (choice by emoji) and `image` (a
photo sent from the phone). Add `ai: true` if the block calls Claude: it then gets the "Claude"
badge in the catalog. A parameter with no field in the form is kept as is when saved.

With `PRINTR_WEB_DIR=web`, simply reload the page to see your block in the catalog.

### 7. Document it

- an example in `docs/exemples/` (`mon_bloc.json`), from which `generer.sh mon_bloc` produces the PNG;
- its entry in `docs/exemples/catalogue.json` (group, emoji, name and description, in French and
  in English): `docs/build.py` builds the site's blocks page from it, and refuses to build if the
  example or its PNG is missing;
- a line in the blocks table of the guide, `docs/guide.md` (and `docs/guide.fr.md`) (parameters and default values);
- a chip in the catalog of `docs/index.html`, in French and in English, pointing to its entry:
  `<a class="chip" href="blocs.html#mon_bloc">`.

### Special cases

**A block that queries an API** receives the context (`ctx: &Ctx`) and uses its HTTP client, which
already has a timeout:

```rust
let data: Reponse = ctx.http.get("https://…").query("ville", ville).call()?.body_mut().read_json()?;
```

Prefer free APIs that need no key, and handle their outages with a clear error (`anyhow`). To test
without the network, separate the decoding and the rendering from the call, and test them on a
sample response (see `barnum.rs`).

**A block that calls Claude** (see `horoscope.rs` and `word.rs`):

- asks for a structured JSON answer with `ctx.claude.generate(system, prompt, schema, effort)`;
- in preview (`ctx.preview`), never calls Claude: it returns `ai_placeholder(title, description)`;
- keeps its result cached for the day (`ctx.cache`, with `get` and `put`, under a key that
  contains the date and `PROMPT_VERSION`), and ignores the cache if `ctx.refresh` is true;
- checks that the answer is complete, and retries once if a field comes back empty.

**The rules that apply to every block**:

- a failing block never brings down the ticket: return an error, and the ticket prints a message
  in its place;
- no emoji or exotic characters to print, since the PC858 code page doesn't have them: for a
  drawing, go through an image (see `picto.rs`);
- blocks are built in parallel: a slow block does not hold up the others, but the ticket waits for
  the slowest one before printing.

## Conventions

- **Language**: the code is in English; comments, messages, printed text and documentation are in
  French, with French typography (a space before « : ; ! ? », guillemets « »).
- **Style**: `cargo fmt` and `cargo clippy` without warnings. The build must produce no warnings.
- **Console output**: goes through `ui.rs` and `anstream` (colors are stripped automatically
  outside a terminal, or with `NO_COLOR`).
- **Commits**: messages in French, in the imperative or the noun form ("Blocs énigme et défi
  sportif"), with an explanatory paragraph when the change is not obvious.

## Web interface and site

- The interface lives in `web/`. With `PRINTR_WEB_DIR=web`, a simple page reload is enough. Check
  it on a phone (390 px wide) as well as on a computer, in light and dark mode.
- **Before each version**, `scripts/illustrations.sh` remakes every illustration in both languages
  (block examples, app screenshots, showcase tickets, link preview image), then builds the site;
  `--sans-claude` avoids any Claude call (Claude blocks keep their current image).
- The screenshots for the site and the docs are regenerated with `docs/demo/photo-session.sh`,
  always with fictional data. Never put real personal data in them. Those of the first start (Wi-Fi
  hotspot, wizard, settings) come from `docs/demo/setup-session.sh`, which runs Printr in an
  isolated network namespace (`unshare`, no sudo) with a fake network, fake `nmcli` and the emulator.
- The logo variants are regenerated with `uv run --with pillow python docs/brand/make-assets.py`.
- The site is built with `docs/build.py`: the home page (`docs/index.html`) and the documentation,
  rendered from `docs/guide.md` and `CONTRIBUTING.md`. A change to these files therefore also updates
  the site. Local preview:
  `uvx --with markdown --with pymdown-extensions python docs/build.py && python3 -m http.server -d _site`.

## Versions

The version number (`Cargo.toml`, shown by `printr --version` and in the app) follows
[semantic versioning](https://semver.org/lang/fr/):

- **patch** (1.0.**1**): a fix, with nothing changed about how it is used;
- **minor** (1.**1**.0): one more block, option or screen; existing tickets keep working;
- **major** (**2**.0.0): a change that requires editing your tickets or your configuration.

Each minor version is named after a pioneer of printing, in alphabetical order (see the
[CHANGELOG](CHANGELOG.md)): update `CODENAME` in `src/main.rs` and the CHANGELOG heading together (a
test checks they match).

Every published version has its git tag (`v1.0.0`), created on `master` once the version has been
pushed: `git tag -a v1.1.0 -m "printr 1.1.0" && git push origin v1.1.0`. The Release workflow then
builds the binaries and publishes the GitHub release, with the CHANGELOG section as notes. Installations see it
within the day ("My account" of the administrator): only push a tag for a version that is ready
to be installed.

## Proposing a change

1. Create a branch from `master`.
2. Check that `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`
   pass. The CI runs them on every pull request, along with `cargo deny check` (known
   vulnerabilities, licenses), `shellcheck` on the scripts and `zizmor` on the workflows.
3. If you touch the rendering, attach a screenshot of the preview or of the emulator output to
   your pull request.
4. Open a pull request describing the why as much as the how.

Got a question or an idea for a block? Open an issue.
