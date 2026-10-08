# Apple Box fork of PhotoCraft

This repository is a thin fork of [storytold/photocraft](https://github.com/storytold/photocraft) (MIT / Apache-2.0,
see `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`). The `applebox` branch carries the fork; `main` tracks upstream unchanged.
The fork exists so the web (wasm) build can be embedded in another product's page: files go in and come out through a
JavaScript bridge instead of drag-and-drop and browser downloads, and the product name / community links are hidden
inside the embedding page. The engine, the document model, the PSD codec and the editing tools are untouched.

## What changed (and only this)

| Where | Change |
|---|---|
| `apps/photocraft-web/src/web.rs` | Four exported functions: `open_bytes(name, bytes)` (pushes the existing drop/open inbox), `set_host_writer(cb)` (`Services.write` calls `cb(name, Uint8Array)` instead of triggering a download; without a writer the download path is unchanged), `host_command(cmd)` (`"save_psd"` → File › Save As `<name>.psd`, `"export_png"` → Quick Export as PNG; both end in the host writer), `set_embedded(on)`. Pending commands run once per frame in `WebShell::logic`. Files opened before eframe has created the app (the bindings, and `lc-pc-ready`, exist slightly earlier) are queued and handed to the inbox when the app starts. |
| `crates/ui-egui/src/ab_bridge.rs` (new; tests in `ab_bridge_tests.rs`) | Host bridge v2: policy over the upstream control protocol (`control.rs`, unchanged). `request_allowed` lets through a short list of methods (`engine.execute`, `ui.set`, `ui.key`, `ui.dialog.*`, two `ab.*`), engine commands (layer select/props/order, layer mask add/invert/delete, Brightness/Contrast, Hue/Saturation and Levels adjustment layers, type create/edit/style, undo/redo, deselect, brush and colour settings, Free Transform), the `ui.set` fields and tools of the host's toolbar, and only Enter/Escape for `ui.key`. It is checked where host requests enter, not installed as `Services.automation_command` (that gate also sees the commands the app runs on a request's behalf, e.g. `edit.transform` when a Free Transform is committed). `state_from` + `Digest`: the state digest the host mirrors, with a `seq` that only increases. `key_allowed` / `tool_for_key`: the canvas key policy for embedded mode. Two host-only methods: `ab.layer.thumbs` (the Layers panel thumbnails as RGBA) and `ab.toolOptions` (Magic Wand tolerance / contiguous). `lib.rs`: `pub mod ab_bridge`, `thumb_image` is `pub(crate)`. Contract test: every allowed command id, `ui.set` field and blend label still exists upstream. |
| `apps/photocraft-web/src/web.rs` (bridge v2) | Two more exports: `host_control(id, method, params_json)` (allow-listed, then into the app's control channel, `with_control`) and `set_host_events(cb)` (replies and state digests as JSON strings). Each frame after `app.logic`: replies, `ab.*` calls, and the digest (built right after a reply, else at most every 100 ms; sent only when it changed). |
| Embedded draws only the canvas: `embedded::canvas_only()`; `lib.rs` `ui()` (no title/options/status bars, toolbar or dock; no floating panels, dialogs, notices or GPU fallback card; Esc doesn't leave a screen mode; backdrop = the host's night colour), `canvas.rs` (no document tabs or Home Screen, even when the Home Screen was up over a document; nothing drawn before the first document; no canvas context menus), `paint_mouse.rs` (a right-click never opens the Brush Preset picker, and a picker already open is not drawn), `web.rs` (`WebShell::raw_input_hook` forwards to the app's hook, then `ab_bridge::filter_canvas_input`: only the first batch's keys reach the canvas: undo/redo, zoom, `[`/`]`, Space, Enter/Escape, bare Delete/Backspace to clear the selected pixels; tool letters pick first-batch tools, no text/IME/clipboard input). Off (`set_embedded(false)`), everything is upstream's; whole-app frame tests check both, right-clicks, the Home Screen over a document, Delete on a selection, and the rasterize prompt answered through the state digest. |
| `crates/text/src/fonts.rs` (+ `runtime_font_tests.rs`), `web.rs`, `ab_init.js` | Run-time font registration for the host: `fonts::add_runtime_font(bytes)` registers with the shared text engine (`register_font_data`, unchanged); a family in the fallback lists (e.g. "Noto Sans TC") fills in glyphs for every family. `web.rs` exports `ab_add_font(name, bytes)` (registers, then `type.updateAllTextLayers`). The repository ships no font: the host sends one. The test builds a tiny TrueType font in code. |
| `apps/photocraft-web/ab_init.js`, `index.html` | Trunk `data-initializer`: download progress and the postMessage bridge (`lc-pc-*` messages, same-origin parent only). Page title "Editor". |
| `crates/ui-egui/src/applebox_theme.rs` (new) | The host's dark palette as egui `Visuals` + `Spacing` and as the app's own colour `Tokens` (most panels paint from those) (`cis` colour table in one place, `applebox_visuals`, `applebox_tokens`, `applebox_spacing`, `apply_if`); applied at the end of `theme::apply` and from `set_embedded` when embedded mode is on. Appearance only. Tests: CIS values land in the visuals, every text pair ≥ 4.5:1, the switch applies/leaves the context. |
| `crates/ui-egui/src/embedded.rs` (new) | The `embedded` flag and pure helpers (`app_name_for`, `brand_for`, `hidden_for`, `filter_menu_for`) with tests, plus `apply_embedded_visuals(ctx)`: the hook for the host's colour scheme (separate from the brand hiding; today only the selection accent). |
| `crates/ui-egui/src/{canvas,dialogs,file_ui,gpu_status,notices,panels,prefs_ui,rasterize_prompt,menus}.rs` | Each user-visible "PhotoCraft" string goes through `embedded::brand`/`app_name`; the brand mark, the Discord button and the link rows are skipped in embedded mode; `help.*` links, About and Print leave the menus and are refused by `menus::invoke`. |
| `.github/workflows/ab-release.yml` | Builds the web bundle on every push to `applebox` (artifact) and on `ab-v*` tags (GitHub release with brotli/gzip precompressed files, `VERSION.json`, `SHA256SUMS`). |

Nothing in this fork talks to any server, and the repository holds no sample documents; tests draw their own images.

## Host page protocol

| Direction | Message | Meaning |
|---|---|---|
| host → editor | `{type:'lc-pc-open', name, bytes}` | Open `bytes` (ArrayBuffer) as `name` (format by extension, same as drag-and-drop). |
| host → editor | `{type:'lc-pc-command', cmd}` | `cmd` is `save_psd` or `export_png`. |
| host → editor | `{type:'lc-pc-font', name, bytes}` | A font (TTF/OTF, ArrayBuffer) for type layers; send it before `lc-pc-open` (text layers already open are laid out again, one history step). |
| editor → host | `{type:'lc-pc-progress', loaded, total}` | wasm download progress (`total` may be 0 when unknown). |
| editor → host | `{type:'lc-pc-ready', backend, bridge}` | The app runs; `backend` is `webgpu` or `webgl2` (what the browser offers); `bridge` is `2` when `lc-pc-call` is available. |
| host → editor | `{type:'lc-pc-call', id, method, params}` | Bridge v2: one control request (`ab_bridge::METHODS`; anything else is refused with `ok: false`). `id` is the host's, echoed in the reply. |
| editor → host | `{type:'lc-pc-reply', id, ok, result \| error}` | The answer to `lc-pc-call` `id` (after the command, or a background job it started, has finished). |
| editor → host | `{type:'lc-pc-state', seq, rev, tool, selectionMode, wand, brush, fg, doc, layers, active, typing, transforming, dialog}` | The editor's state, sent when it changed. `tool` and `layers[].kind` are camelCase (`magicWand`, `pixel`, `type`, `adjustment`, `group`; the names `ui.set` takes). `doc.maskTarget`: painting goes to the active layer's mask. `layers[]` is flat, top to bottom, with `depth`, and `thumbRev`: the layer's pixel fingerprint (same as `ab.layer.thumbs` `rev`; absent for layers without pixels), so the host refetches only thumbnails whose `thumbRev` changed. `seq` starts at 1 and only increases for the life of the page; order states by `seq` (`rev` is the document's own revision and restarts with each document). `dialog` is the newest open dialog or `null`: `{id, kind, title, fields}`. In embedded mode no dialog is drawn, so the host answers it with `ui.dialog.confirm {dialog: id}` / `ui.dialog.cancel {dialog: id}`. The one a canvas gesture can open is the rasterize prompt (painting a pixel tool on a type layer): `kind: "Command"`, recognised by `fields.__rasterize` (`"type"`, `"shape"`, `"smartObject"` or `"fill"`), with `fields.message` (upstream's English question), `fields.layer` (id) and `fields.__command` (`layer.rasterize.*`); confirm rasterizes, then paints where the click was. |
| editor → host | `{type:'lc-pc-file', name, kind, bytes}` | A saved (`psd`) or exported (`png`) file, bytes transferred. |
| editor → host | `{type:'lc-pc-error', message}` | Start-up failed. |

Messages are accepted only from `window.parent` on the same origin.

## Rebasing on upstream

```sh
git fetch upstream                      # upstream = https://github.com/storytold/photocraft
git switch main && git merge --ff-only upstream/main && git push origin main
git switch applebox && git rebase main  # conflicts, if any, are in the files listed above
cargo test -p photocraft-ui-egui --lib -- embedded ab_bridge applebox_theme && cargo test -p photocraft-text --lib runtime_font
git push --force-with-lease origin applebox
git tag ab-v<upstream version>-<n> && git push origin ab-v<upstream version>-<n>
```

Tags are `ab-v<upstream version>-<n>` (for example `ab-v0.3.0-1`), always cut from `applebox`.
