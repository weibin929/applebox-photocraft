//! The browser shell: web `Services`, drag-and-drop, and the eframe web runner.

use std::sync::{Arc, Mutex};

use photocraft_codecs::{ChannelLayout, EncodeOptions, Image};
use photocraft_doc::Document;
use photocraft_engine::Session;
use photocraft_ui_egui::control::ControlRequest;
use photocraft_ui_egui::theme::ThemeKind;
use photocraft_ui_egui::{PhotocraftApp, Services, ab_bridge};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::wasm_bindgen;

type Inbox = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

// ── Host bridge (Apple Box fork, AB_FORK.md) ────────────────────────────────────────────────────
// The host page (same origin) talks to the editor through three exported functions instead of
// drag-and-drop and browser downloads: `open_bytes` feeds the inbox, `set_host_writer` receives
// every saved/exported file, `host_command` asks for a save or an export. `set_embedded` hides
// the product name and community links (`photocraft_ui_egui::embedded`).
thread_local! {
    static HOST_WRITER: std::cell::RefCell<Option<js_sys::Function>> = const { std::cell::RefCell::new(None) };
    static PENDING_CMDS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    static HOST_INBOX: std::cell::RefCell<Option<(Inbox, egui::Context)>> = const { std::cell::RefCell::new(None) };
    // Files the host sent before eframe created the app (the JS bindings, and so `lc-pc-ready`,
    // exist a moment before the app does): kept here and moved into the inbox when it appears.
    static EARLY_OPENS: std::cell::RefCell<Vec<(String, Vec<u8>)>> = const { std::cell::RefCell::new(Vec::new()) };
    // Bridge v2 (`photocraft_ui_egui::ab_bridge`): the control channel into the app, replies still
    // owed to the host, `ab.*` calls waiting for the frame, the host's event callback, the digest.
    static CONTROL_TX: std::cell::RefCell<Option<std::sync::mpsc::Sender<ControlRequest>>> = const { std::cell::RefCell::new(None) };
    static PENDING_REPLIES: std::cell::RefCell<Vec<(u32, std::sync::mpsc::Receiver<serde_json::Value>)>> = const { std::cell::RefCell::new(Vec::new()) };
    static AB_CALLS: std::cell::RefCell<Vec<(u32, String, serde_json::Value)>> = const { std::cell::RefCell::new(Vec::new()) };
    static HOST_EVENTS: std::cell::RefCell<Option<js_sys::Function>> = const { std::cell::RefCell::new(None) };
    static DIGEST: std::cell::RefCell<ab_bridge::Digest> = std::cell::RefCell::new(ab_bridge::Digest::default());
}

/// Host → editor: open a file's bytes as if it had been dropped on the canvas.
#[wasm_bindgen]
pub fn open_bytes(name: String, bytes: Vec<u8>) {
    HOST_INBOX.with(|h| {
        if let Some((inbox, ctx)) = &*h.borrow() {
            inbox.lock().unwrap_or_else(|e| e.into_inner()).push((name, bytes));
            ctx.request_repaint();
        } else {
            log::info!("open_bytes before the app started: {name} queued until it does");
            EARLY_OPENS.with(|q| q.borrow_mut().push((name, bytes)));
        }
    });
}

/// Called once the app exists: hand over everything `open_bytes` queued before that.
fn adopt_early_opens(inbox: &Inbox, ctx: &egui::Context) {
    let early: Vec<(String, Vec<u8>)> = EARLY_OPENS.with(|q| std::mem::take(&mut *q.borrow_mut()));
    if !early.is_empty() {
        inbox.lock().unwrap_or_else(|e| e.into_inner()).extend(early);
        ctx.request_repaint();
    }
}

/// Editor → host: `cb(name, Uint8Array)` for every Save / Export instead of a browser download.
#[wasm_bindgen]
pub fn set_host_writer(cb: js_sys::Function) {
    HOST_WRITER.with(|w| *w.borrow_mut() = Some(cb));
}

/// Host → editor: `"save_psd"` (File › Save As, <name>.psd) or `"export_png"` (Quick Export as PNG).
/// Anything else is ignored. The result reaches the host writer.
#[wasm_bindgen]
pub fn host_command(cmd: String) {
    if cmd == "save_psd" || cmd == "export_png" {
        PENDING_CMDS.with(|c| c.borrow_mut().push(cmd));
        HOST_INBOX.with(|h| {
            if let Some((_, ctx)) = &*h.borrow() {
                ctx.request_repaint();
            }
        });
    } else {
        log::warn!("host_command: unknown command {cmd:?}");
    }
}

/// Host → editor: embedded mode on/off (product name, links and brand mark hidden).
#[wasm_bindgen]
pub fn set_embedded(on: bool) {
    photocraft_ui_egui::embedded::set_on(on);
    HOST_INBOX.with(|h| {
        if let Some((_, ctx)) = &*h.borrow() {
            photocraft_ui_egui::embedded::apply_embedded_visuals(ctx);
            ctx.request_repaint();
        }
    });
}

/// Host → editor (bridge v2): run a control method (`ab_bridge::METHODS`, allow-listed). The
/// reply comes back through the events callback as `{type:"reply", id, ok, result|error}`; a
/// refused request replies at once with `ok: false`.
#[wasm_bindgen]
pub fn host_control(id: u32, method: String, params_json: String) {
    let params: serde_json::Value = serde_json::from_str(&params_json).unwrap_or(serde_json::Value::Null);
    if let Err(e) = ab_bridge::request_allowed(&method, &params) {
        emit(&serde_json::json!({"type": "reply", "id": id, "ok": false, "error": e}));
        return;
    }
    if method.starts_with("ab.") {
        // These need `&mut app`: run in the next frame.
        AB_CALLS.with(|c| c.borrow_mut().push((id, method, params)));
    } else {
        let (req, rx) = ControlRequest::new(method, params);
        let sent = CONTROL_TX.with(|t| t.borrow().as_ref().is_some_and(|tx| tx.send(req).is_ok()));
        if !sent {
            emit(&serde_json::json!({"type": "reply", "id": id, "ok": false, "error": "the editor is not running yet"}));
            return;
        }
        PENDING_REPLIES.with(|p| p.borrow_mut().push((id, rx)));
    }
    HOST_INBOX.with(|h| {
        if let Some((_, ctx)) = &*h.borrow() {
            ctx.request_repaint();
        }
    });
}

/// Host → editor: a font (TTF/OTF bytes) for type layers; the web build has no system fonts. A family
/// in the fallback lists ("Noto Sans TC") also fills in glyphs other fonts lack. Text layers
/// already in the document are laid out again (`type.updateAllTextLayers`), so send fonts before
/// opening a document to keep that out of its history.
#[wasm_bindgen]
pub fn ab_add_font(name: String, bytes: Vec<u8>) {
    match ab_bridge::add_runtime_font(bytes) {
        Ok(families) => {
            log::info!("font {name}: {families:?}");
            let (req, _reply) = ControlRequest::new("engine.execute", serde_json::json!({"command": "type.updateAllTextLayers"}));
            CONTROL_TX.with(|t| t.borrow().as_ref().map(|tx| tx.send(req)));
            HOST_INBOX.with(|h| {
                if let Some((_, ctx)) = &*h.borrow() {
                    ctx.request_repaint();
                }
            });
        }
        Err(e) => log::error!("font {name}: {e}"),
    }
}

/// Editor → host: `cb(json)` for replies and `lc-pc-state` digests.
#[wasm_bindgen]
pub fn set_host_events(cb: js_sys::Function) {
    HOST_EVENTS.with(|e| *e.borrow_mut() = Some(cb));
    DIGEST.with(|d| d.borrow_mut().resend()); // a new listener gets the current state (seq keeps counting)
    HOST_INBOX.with(|h| {
        if let Some((_, ctx)) = &*h.borrow() {
            ctx.request_repaint();
        }
    });
}

fn emit(v: &serde_json::Value) {
    HOST_EVENTS.with(|e| {
        if let Some(cb) = &*e.borrow()
            && let Err(err) = cb.call1(&wasm_bindgen::JsValue::NULL, &v.to_string().into())
        {
            log::error!("host events callback failed: {err:?}");
        }
    });
}

/// Once per frame after `app.logic`: deliver control replies, run `ab.*` calls, and send the state
/// digest when it changed (built right after a reply, else at most every 100 ms).
fn pump_bridge(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if HOST_EVENTS.with(|e| e.borrow().is_none()) {
        return;
    }
    let mut replied = false;
    let mut done = Vec::new();
    PENDING_REPLIES.with(|p| {
        p.borrow_mut().retain(|(id, rx)| match rx.try_recv() {
            Ok(v) => {
                done.push((*id, v));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                done.push((*id, serde_json::json!({"ok": false, "error": "dropped"})));
                false
            }
        })
    });
    for (id, mut v) in done {
        v["type"] = "reply".into();
        v["id"] = id.into();
        emit(&v);
        replied = true;
    }
    let calls: Vec<_> = AB_CALLS.with(|c| std::mem::take(&mut *c.borrow_mut()));
    for (id, method, params) in calls {
        emit(&match ab_bridge::run_ab(app, &method, &params) {
            Ok(v) => serde_json::json!({"type": "reply", "id": id, "ok": true, "result": v}),
            Err(e) => serde_json::json!({"type": "reply", "id": id, "ok": false, "error": e}),
        });
        replied = true;
    }
    let now = js_sys::Date::now();
    if DIGEST.with(|d| d.borrow().due(now, replied)) {
        let state = ab_bridge::state_from(&photocraft_ui_egui::control::inspect(app, ctx), &ab_bridge::StateExtra::of(app));
        if let Some(s) = DIGEST.with(|d| d.borrow_mut().next(state, now)) {
            emit(&s);
        }
    } else {
        // Something ran this frame; look again once the interval is over.
        ctx.request_repaint_after(std::time::Duration::from_millis(ab_bridge::STATE_INTERVAL_MS as u64));
    }
}

/// Hand `bytes` to the host writer; `false` when none is installed (the caller then downloads).
fn host_write(path: &str, bytes: &[u8]) -> bool {
    HOST_WRITER.with(|w| match &*w.borrow() {
        Some(cb) => {
            let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
            let data = js_sys::Uint8Array::from(bytes);
            if let Err(e) = cb.call2(&wasm_bindgen::JsValue::NULL, &name.into(), &data.into()) {
                log::error!("host writer failed: {e:?}");
            }
            true
        }
        None => false,
    })
}

/// Run the host's pending commands (one frame, in order).
fn run_host_commands(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let cmds: Vec<String> = PENDING_CMDS.with(|c| std::mem::take(&mut *c.borrow_mut()));
    for cmd in cmds {
        let r = match cmd.as_str() {
            "save_psd" => {
                let name = app.session.active().map(|d| d.doc.name.clone()).unwrap_or_else(|| "document".into());
                let stem = name.rsplit_once('.').map_or(name.as_str(), |(a, _)| a).to_string();
                photocraft_ui_egui::menus::invoke(app, ctx, "file.saveAs", serde_json::json!({ "path": format!("{stem}.psd") }))
            }
            "export_png" => photocraft_ui_egui::export_dialog::quick_export_png(app),
            _ => Ok(serde_json::Value::Null),
        };
        if let Err(e) = r {
            log::error!("host_command {cmd}: {e}");
        }
    }
}

/// Everything File › Open reads: PhotoCraft and Photoshop documents, flat images, and Photoshop
/// brushes (.abr) and gradients (.grd), which go to the preset libraries.
const OPEN_EXTS: &[&str] = &[
    "pcraft", "psd", "psb", "psdt", "png", "jpg", "jpeg", "tif", "tiff", "webp", "gif", "bmp", "tga", "ico", "qoi", "exr", "hdr", "pbm", "pgm", "ppm", "pam",
    "pfm", "heic", "heif", "hif", "dng", "cr2", "cr3", "nef", "nrw", "arw", "pef", "orf", "rw2", "raf", "abr", "grd",
];
const CANVAS_ID: &str = "photocraft_canvas";

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("no document");
            return;
        };
        let Some(canvas) = document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            log::error!("missing <canvas id=\"{CANVAS_ID}\">");
            return;
        };
        let q = query();
        let force_cpu = q.contains("cpu");
        let mut options = eframe::WebOptions::default();
        photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut options.wgpu_options.wgpu_setup);
        if q.contains("webgl")
            && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
        {
            create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
        let pen_target = canvas.clone();
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(move |cc| {
                    PhotocraftApp::setup_context(&cc.egui_ctx, ThemeKind::Pro);
                    let inbox: Inbox = Arc::default();
                    HOST_INBOX.with(|h| *h.borrow_mut() = Some((inbox.clone(), cc.egui_ctx.clone())));
                    adopt_early_opens(&inbox, &cc.egui_ctx);
                    photocraft_ui_egui::embedded::apply_embedded_visuals(&cc.egui_ctx);
                    let (control_tx, control_rx) = std::sync::mpsc::channel();
                    CONTROL_TX.with(|t| *t.borrow_mut() = Some(control_tx));
                    let mut app = PhotocraftApp::new(Session::new(), services(inbox.clone(), cc.egui_ctx.clone())).with_control(control_rx);
                    listen_pen(&pen_target, app.stylus.feed.clone());
                    app.set_theme(&cc.egui_ctx, ThemeKind::Pro);
                    if let Some(rs) = cc.wgpu_render_state.clone()
                        && !force_cpu
                    {
                        log::info!("photocraft-web: wgpu backend {:?}", rs.adapter.get_info().backend);
                        app.set_wgpu(rs);
                    }
                    Ok(Box::new(WebShell { app, inbox }))
                }),
            )
            .await;
        if let Some(el) = document.get_element_by_id("photocraft_loading") {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!("<p>PhotoCraft failed to start: {e:?}</p><p>A browser with WebGPU or WebGL2 is required.</p>")),
            }
        }
    });
}

/// Pen pressure, tilt, twist and the eraser button from Pointer Events (eframe forwards none of them for pens) into
/// the app's stylus feed. The sample is kept through `pointerup` so the stroke's last points keep
/// their pressure; hovering, a mouse, or leaving the canvas clears it.
fn listen_pen(target: &web_sys::HtmlCanvasElement, feed: photocraft_ui_egui::stylus::StylusFeed) {
    use photocraft_ui_egui::stylus::PenSample;
    use wasm_bindgen::closure::Closure;
    for kind in ["pointerdown", "pointermove", "pointerup", "pointercancel", "pointerleave"] {
        let feed = feed.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
            let ty = e.type_();
            if ty == "pointerup" && e.pointer_type() == "pen" {
                return;
            }
            let pen = e.pointer_type() == "pen" && e.buttons() != 0 && ty != "pointercancel" && ty != "pointerleave";
            // W3C Pointer Events: `buttons` bit 5 (32) is the pen's eraser.
            let eraser = e.buttons() & 32 != 0;
            feed.set(pen.then(|| PenSample {
                pressure: e.pressure(),
                tilt_x: e.tilt_x() as f32,
                tilt_y: e.tilt_y() as f32,
                rotation: e.twist() as f32,
                eraser,
            }));
        });
        if target.add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref()).is_ok() {
            cb.forget();
        }
    }
}

fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}

/// Wraps the app to read dropped files asynchronously (browsers can't read them synchronously,
/// so the app's own drop path can't handle them) and feed them through the inbox.
struct WebShell {
    app: PhotocraftApp,
    inbox: Inbox,
}

impl eframe::App for WebShell {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        for f in dropped {
            let inbox = self.inbox.clone();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let name = f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dropped".into());
                match f.bytes_async().await {
                    Ok(bytes) => {
                        inbox.lock().unwrap_or_else(|e| e.into_inner()).push((name, bytes));
                        ctx.request_repaint();
                    }
                    Err(e) => log::error!("couldn't read dropped file {name}: {e}"),
                }
            });
        }
        run_host_commands(&mut self.app, ctx);
        self.app.logic(ctx, frame);
        pump_bridge(&mut self.app, ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
    }

    // The app's hook feeds queued synthetic input (`ui.key` from the host bridge) and turns
    // clipboard events back into key presses; without forwarding it, `ui.key` never arrives.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        eframe::App::raw_input_hook(&mut self.app, ctx, raw_input);
        if photocraft_ui_egui::embedded::canvas_only() {
            ab_bridge::filter_canvas_input(&mut self.app.ui.tool, &mut raw_input.events);
        }
    }
}

fn services(inbox: Inbox, ctx: egui::Context) -> Services {
    let open_inbox = inbox.clone();
    Services {
        import: Some(Box::new(|name: &str, bytes: &[u8]| photocraft_io::import(name, bytes).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string()))),
        export: Some(Box::new(|doc: &Document, path: &str, settings: &photocraft_ui_egui::ExportSettings| {
            let mut opts = photocraft_io::ExportOptions::default();
            if let Some(q) = settings.jpeg_quality {
                opts.encode.jpeg_quality = q;
            }
            opts.encode.webp_lossless = settings.webp_lossless;
            if let Some(q) = settings.webp_quality {
                opts.encode.webp_quality = q;
            }
            opts.tiff_layers = settings.tiff_layers;
            opts.xmp = if settings.xmp_all { photocraft_io::XmpEmbed::All } else { photocraft_io::XmpEmbed::None };
            photocraft_io::export(doc, path, &opts).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
        })),
        pick_open: Some(Box::new(move || {
            let inbox = open_inbox.clone();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let Some(file) = rfd::AsyncFileDialog::new().add_filter("All Formats", OPEN_EXTS).pick_file().await else {
                    return;
                };
                let bytes = file.read().await;
                inbox.lock().unwrap_or_else(|e| e.into_inner()).push((file.file_name(), bytes));
                ctx.request_repaint();
            });
            None
        })),
        // No save dialog on the web: the suggested name becomes the download name.
        pick_save: Some(Box::new(|suggested: &str| {
            let name = std::path::Path::new(suggested).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| suggested.to_string());
            Some(name)
        })),
        write: Some(Box::new(|path: &str, bytes: &[u8]| if host_write(path, bytes) { Ok(()) } else { download(path, bytes) })),
        encode_png: Some(Box::new(|w, h, rgba| {
            let img = Image::from_u8(w, h, ChannelLayout::Rgba, rgba.to_vec()).map_err(|e| e.to_string())?;
            photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &EncodeOptions::default()).map_err(|e| e.to_string())
        })),
        inbox: Some(inbox),
        // Preferences live in the browser's localStorage.
        load_prefs: Some(Box::new(|| local_storage()?.get_item(PREFS_KEY).ok().flatten())),
        save_prefs: Some(Box::new(|text: &str| local_storage().ok_or("no localStorage")?.set_item(PREFS_KEY, text).map_err(|e| format!("{e:?}")))),
        ..Default::default()
    }
}

const PREFS_KEY: &str = "photocraft.preferences";

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Trigger a browser download of `bytes` named after the last component of `path`.
fn download(path: &str, bytes: &[u8]) -> Result<(), String> {
    let js = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "photocraft".into());
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime_for(&name));
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(js)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(js)?.dyn_into().map_err(|_| "not an anchor")?;
    a.set_href(&url);
    a.set_download(&name);
    a.style().set_property("display", "none").map_err(js)?;
    let body = document.body().ok_or("no body")?;
    body.append_child(&a).map_err(js)?;
    a.click();
    a.remove();
    // Revoke after the click has been dispatched; the download keeps its own reference.
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        web_sys::Url::revoke_object_url(&url).ok();
    });
    window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000).map_err(js)?;
    Ok(())
}

fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("tif" | "tiff") => "image/tiff",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("psd" | "psb") => "image/vnd.adobe.photoshop",
        _ => "application/octet-stream",
    }
}
