# Apple Box fork of PhotoCraft

This repository is a thin fork of [storytold/photocraft](https://github.com/storytold/photocraft) (MIT / Apache-2.0,
see `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`). The `applebox` branch carries the fork; `main` tracks upstream unchanged.
The fork exists so the web (wasm) build can be embedded in another product's page: files go in and come out through a
JavaScript bridge instead of drag-and-drop and browser downloads, and the product name / community links are hidden
inside the embedding page. The engine, the document model, the PSD codec and the editing tools are untouched.

## What changed (and only this)

| Where | Change |
|---|---|
| `apps/photocraft-web/src/web.rs` | Four exported functions: `open_bytes(name, bytes)` (pushes the existing drop/open inbox), `set_host_writer(cb)` (`Services.write` calls `cb(name, Uint8Array)` instead of triggering a download; without a writer the download path is unchanged), `host_command(cmd)` (`"save_psd"` → File › Save As `<name>.psd`, `"export_png"` → Quick Export as PNG; both end in the host writer), `set_embedded(on)`. Pending commands run once per frame in `WebShell::logic`. |
| `apps/photocraft-web/ab_init.js`, `index.html` | Trunk `data-initializer`: download progress and the postMessage bridge (`lc-pc-*` messages, same-origin parent only). Page title "Editor". |
| `crates/ui-egui/src/applebox_theme.rs` (new) | The host's dark palette as egui `Visuals` + `Spacing` (`cis` colour table in one place, `applebox_visuals`, `applebox_spacing`, `apply_if`); applied at the end of `theme::apply` and from `set_embedded` when embedded mode is on. Appearance only. Tests: CIS values land in the visuals, every text pair ≥ 4.5:1, the switch applies/leaves the context. |
| `crates/ui-egui/src/embedded.rs` (new) | The `embedded` flag and pure helpers (`app_name_for`, `brand_for`, `hidden_for`, `filter_menu_for`) with tests, plus `apply_embedded_visuals(ctx)`: the hook for the host's colour scheme (separate from the brand hiding; today only the selection accent). |
| `crates/ui-egui/src/{canvas,dialogs,file_ui,gpu_status,notices,panels,prefs_ui,rasterize_prompt,menus}.rs` | Each user-visible "PhotoCraft" string goes through `embedded::brand`/`app_name`; the brand mark, the Discord button and the link rows are skipped in embedded mode; `help.*` links, About and Print leave the menus and are refused by `menus::invoke`. |
| `.github/workflows/ab-release.yml` | Builds the web bundle on every push to `applebox` (artifact) and on `ab-v*` tags (GitHub release with brotli/gzip precompressed files, `VERSION.json`, `SHA256SUMS`). |

Nothing in this fork talks to any server, and the repository holds no sample documents; tests draw their own images.

## Host page protocol

| Direction | Message | Meaning |
|---|---|---|
| host → editor | `{type:'lc-pc-open', name, bytes}` | Open `bytes` (ArrayBuffer) as `name` (format by extension, same as drag-and-drop). |
| host → editor | `{type:'lc-pc-command', cmd}` | `cmd` is `save_psd` or `export_png`. |
| editor → host | `{type:'lc-pc-progress', loaded, total}` | wasm download progress (`total` may be 0 when unknown). |
| editor → host | `{type:'lc-pc-ready', backend}` | The app runs; `backend` is `webgpu` or `webgl2` (what the browser offers). |
| editor → host | `{type:'lc-pc-file', name, kind, bytes}` | A saved (`psd`) or exported (`png`) file, bytes transferred. |
| editor → host | `{type:'lc-pc-error', message}` | Start-up failed. |

Messages are accepted only from `window.parent` on the same origin.

## Rebasing on upstream

```sh
git fetch upstream                      # upstream = https://github.com/storytold/photocraft
git switch main && git merge --ff-only upstream/main && git push origin main
git switch applebox && git rebase main  # conflicts, if any, are in the files listed above
cargo test -p photocraft-ui-egui embedded
git push --force-with-lease origin applebox
git tag ab-v<upstream version>-<n> && git push origin ab-v<upstream version>-<n>
```

Tags are `ab-v<upstream version>-<n>` (for example `ab-v0.3.0-1`), always cut from `applebox`.
