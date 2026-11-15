---
name: watch-app
description: Make a new CMF Watch Pro 2 app in apps/<name>, or change one, from choosing its surfaces (menu app, widget card, page next to the watch face, phone link) to the store files and the PR. Use for any "make an app / add a widget / show it next to the watch face / change app X" request in this repository.
---

# A watch app, from idea to PR

AGENTS.md has the rules; this is the order to work in. Every doc path is under `.sdk/wp2-apps/` after `make sdk`.

## 1. Is the SDK new enough?

```bash
make sdk
git -C .sdk/wp2-apps fetch --quiet origin && git -C .sdk/wp2-apps log --oneline HEAD..origin/main
```

Read the subjects. If the app needs something only a newer commit has (a new `Api` call, a feature flag, a stock
widget), stop and do the `sdk-update` skill first, as a PR of its own. Never answer "the watch can't" from the pinned
docs alone.

## 2. Pick the surfaces with the owner

Ask, don't guess. A package can mix them (`app-guide.md` section 3, "Widget" and "Page"):

- **Menu app**: always. `BUTTON` ends it within 1000 ms.
- **Widget card** (`WIDGET`): two lines of text on a stock card left of the face. Good for "next thing / count /
  last value". `api.widget(title, value)`: title ≤ 23 B, value ≤ 39 B with one `\n`. Writes NAND: call it when the
  value changes and at start, never per frame. The card shows the new text the next time the face is shown.
- **Page** (`PAGE`): a full screen next to the face ("full-screen widget"). Branch on `api.on_page()` for a compact
  view. No `BUTTON` there, so the loop never returns on its own; `PAUSE` on swipe away, `RESUME` on return: draw
  nothing and run no motor in between, redraw time-dependent parts on `RESUME`.
- **Phone/Mac link**: AGENTS.md, "Apps that talk to a phone". Only non-personal data on the channel. Name the
  companion under `## Requirements`.

```rust
cmf_wp2_app::entry!(main);
cmf_wp2_app::app_features!(cmf_wp2_app::WIDGET | cmf_wp2_app::PAGE);
```

Worked examples: `sdk/examples/hello` (both, smallest), `sdk/examples/herdr-agents` (both, LVGL, a host).

## 3. Pick the drawing model

`app-guide.md`, "LVGL or `cmf-wp2-ui`?". One or the other per app.

- **LVGL** (`cmf_wp2_app::lv` + `stock`): lists, buttons, text in stock fonts, crown scrolling. Not in the
  simulator. Verify with host tests that build every batch and with the emulator.
- **Canvas** (`cmf-wp2-ui`): custom drawing, animation. Runs in `make sim`.

The look is always the stock one: `stock::font`, `stock::theme`, the `stock` colours, the system font. Text has to fit
the round 466×466 panel (`app-guide.md` section 2, Screen table).

## 4. Scaffold and build it

```bash
make new NAME=<name>      # 1–23 of [a-z0-9_-]; it's the store key forever
```

Then `src/`, `icon.svg` (96×96, `docs/app-icons.md`), `store.toml` (name, summary, version, author), `store.md`
(what it does, `## Requirements`). Keep pure logic (dates, layout, parsing, protocol) in modules without `Api` so
`make test APP=<name>` covers it; the `device` module stays thin, as in `vibration-lab`.

Limits that bite: 128 KiB image, 8 KiB stack (buffers over ~1 KiB in a `static mut`), no heap, no floats, 4096 B
storage blob with a magic/version in front, ≤ 64 B per `text`, LVGL budgets (192 objects, 2048 B per batch).

## 5. Verify

```bash
make test APP=<name>
make sim APP=<name>                     # canvas apps; headless: SIM_ARGS='--headless --frames runs/sim/x --do "..."'
make build APP=<name> && make check
```

LVGL, widget and page need the emulator (`make emu`, `make run APP=<name> HCI_PORT=…`; README.md). A physical
watch only with the owner's explicit yes. Go through `app-guide.md` section 7, the checklist, and say in the PR what
ran where, with a screenshot.

## 6. PR

One app per PR, on a branch. A change to an existing app raises `version` in its `store.toml`. Add the app to the
table in README.md. Nothing built, nothing personal in the diff.
