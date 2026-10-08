//! Host bridge v2: what an embedding page may ask through the upstream control protocol
//! (`control.rs`), the state digest the page mirrors, the canvas key policy, and two small
//! host-only commands. Everything here is policy over the existing protocol; nothing new in the
//! engine.
//!
//! The allow-list is applied where host requests enter (`request_allowed`, called by the web
//! shell before a request reaches the control channel). It is deliberately *not* installed as
//! `Services.automation_command`: that gate also sees the commands the app runs on a request's
//! behalf (committing a Free Transform runs `edit.transform`), which the host never names.
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::state::Tool;

/// Control methods the host may call. Anything else is refused before it reaches `control::handle`.
pub const METHODS: &[&str] = &["engine.execute", "ui.set", "ui.key", "ui.dialog.confirm", "ui.dialog.cancel", "ab.layer.thumbs", "ab.toolOptions"];
/// `ui.set` fields the host may change (a subset of `control::UI_SET_FIELDS`).
pub const UI_SET: &[&str] = &["tool", "selectionMode", "brushSize", "maskTarget", "zoom", "center", "fit"];
/// Tools the host's toolbar offers (first batch only).
pub const TOOLS: &[Tool] = &[Tool::Move, Tool::RectMarquee, Tool::EllipseMarquee, Tool::Lasso, Tool::MagicWand, Tool::Brush, Tool::Eraser, Tool::Type];
/// Commands the host may run (engine commands, plus `edit.freeTransform` from `menus::UI_COMMANDS`).
pub const COMMANDS: &[&str] = &[
    "document.inspect",
    "edit.undo",
    "edit.redo",
    "edit.freeTransform",
    "tools.setBrush",
    "tools.setColors",
    "select.deselect",
    "layer.layerMask.revealAll",
    "layer.layerMask.revealSelection",
    "layer.layerMask.delete",
    "image.adjustments.invert",
    "layer.newAdjustmentLayer.brightnessContrast",
    "layer.newAdjustmentLayer.hueSaturation",
    "layer.newAdjustmentLayer.levels",
    "layer.setAdjustment",
    "type.create",
    "type.edit",
    "type.setStyle",
    "type.info",
    "type.updateAllTextLayers",
    "layer.select",
    "layer.setProps",
    "layer.moveTo",
];
const UI_KEYS: &[&str] = &["Enter", "Escape"];
const SET_PROPS: &[&str] = &["layer", "visible", "opacity", "blend", "name"];
/// Blend modes the host offers (it shows its own labels; these are the engine's).
pub const BLENDS: &[&str] =
    &["Normal", "Dissolve", "Darken", "Multiply", "Color Burn", "Lighten", "Screen", "Color Dodge", "Overlay", "Soft Light", "Hard Light", "Difference"];
/// Longest side of `ab.layer.thumbs` images, and how many one call returns.
pub const THUMB_MAX: u64 = 128;
pub const THUMBS_PER_CALL: usize = 16;
/// The state digest is built at most this often (and right after every reply).
pub const STATE_INTERVAL_MS: f64 = 100.0;

/// Engine command gate. Invert is allowed only on a layer mask; `layer.setProps` only for the
/// fields (and blend modes) the host's layer row edits.
pub fn command_allowed(id: &str, params: &Value) -> Result<(), String> {
    if !COMMANDS.contains(&id) {
        return Err(format!("command `{id}` is not available here"));
    }
    if id == "image.adjustments.invert" && params.get("target").and_then(Value::as_str) != Some("mask") {
        return Err("invert is only available on a layer mask here".into());
    }
    if id == "layer.setProps" {
        if let Some(k) = params.as_object().and_then(|o| o.keys().find(|k| !SET_PROPS.contains(&k.as_str()))) {
            return Err(format!("layer.setProps field `{k}` is not available here"));
        }
        if let Some(b) = params.get("blend")
            && !b.as_str().is_some_and(|b| BLENDS.contains(&b))
        {
            return Err(format!("blend {b} is not available here"));
        }
    }
    Ok(())
}

/// Whole-request gate for a host request (method + params).
pub fn request_allowed(method: &str, params: &Value) -> Result<(), String> {
    if !METHODS.contains(&method) {
        return Err(format!("method `{method}` is not available here"));
    }
    match method {
        "engine.execute" => {
            let id = params.get("command").and_then(Value::as_str).ok_or("missing `command`")?;
            command_allowed(id, params.get("params").unwrap_or(&Value::Null))
        }
        "ui.set" => {
            let o = params.as_object().ok_or("`ui.set` takes an object")?;
            if let Some(k) = o.keys().find(|k| !UI_SET.contains(&k.as_str())) {
                return Err(format!("`ui.set` field `{k}` is not available here"));
            }
            match o.get("tool") {
                None => Ok(()),
                // Parsed the way `ui.set` parses it (case and spacing are loose there), then checked.
                Some(t) => match t.as_str().and_then(Tool::from_name) {
                    Some(tool) if TOOLS.contains(&tool) => Ok(()),
                    _ => Err(format!("tool {t} is not available here")),
                },
            }
        }
        "ui.key" => {
            let k = params.get("key").and_then(Value::as_str).unwrap_or("");
            let mods = ["command", "shift", "alt", "ctrl", "modifiers"].iter().any(|m| params.get(*m).is_some());
            if UI_KEYS.contains(&k) && !mods { Ok(()) } else { Err(format!("key `{k}` is not available here")) }
        }
        _ => Ok(()),
    }
}

/// Keys the canvas keeps while embedded; everything else is dropped before the app sees it.
/// Tool letters are not passed through (upstream's W is Quick Selection, and ⇧ cycles to tools
/// outside the first batch): [`tool_for_key`] maps them instead.
pub fn key_allowed(key: egui::Key, m: egui::Modifiers) -> bool {
    use egui::Key::*;
    let cmd = m.command || m.ctrl;
    match key {
        Z => cmd && !m.alt,                            // undo / redo (⇧)
        Y => cmd && !m.shift && !m.alt,                // redo
        D => cmd && !m.shift && !m.alt,                // deselect
        T => cmd && !m.shift && !m.alt,                // free transform
        Equals | Plus | Minus | Num0 => cmd && !m.alt, // zoom in / out / fit
        OpenBracket | CloseBracket => !cmd && !m.alt,  // brush size
        Enter | Escape | Space => !cmd,                // commit / cancel / hold for the hand
        _ => false,
    }
}

/// First-batch tool for a bare tool letter (`current` decides M's rectangle/ellipse toggle).
pub fn tool_for_key(key: egui::Key, m: egui::Modifiers, current: Tool) -> Option<Tool> {
    use egui::Key::*;
    if m.command || m.ctrl || m.alt || m.shift {
        return None;
    }
    Some(match key {
        V => Tool::Move,
        B => Tool::Brush,
        E => Tool::Eraser,
        M if current == Tool::RectMarquee => Tool::EllipseMarquee,
        M => Tool::RectMarquee,
        L => Tool::Lasso,
        W => Tool::MagicWand,
        T => Tool::Type,
        _ => return None,
    })
}

fn hex(c: &Value) -> Value {
    let Some(a) = c.as_array() else { return Value::Null };
    let q = |i: usize| (a.get(i).and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0) * 255.0).round() as u8;
    json!(format!("#{:02x}{:02x}{:02x}", q(0), q(1), q(2)))
}

fn flat_layers(v: &Value, depth: u32, out: &mut Vec<Value>) {
    for l in v.as_array().into_iter().flatten() {
        out.push(json!({
            "id": l["id"], "name": l["name"], "kind": l["kind"], "visible": l["visible"], "opacity": l["opacity"],
            "blend": l["blend"], "hasMask": l["hasMask"], "selected": l["selected"], "depth": depth,
            "adjustment": l.get("adjustment").cloned().unwrap_or(Value::Null),
            "text": l.get("text").cloned().unwrap_or(Value::Null),
        }));
        if let Some(c) = l.get("children") {
            flat_layers(c, depth + 1, out);
        }
    }
}

/// What `control::inspect` doesn't carry.
#[derive(Clone, Copy, Debug, Default)]
pub struct StateExtra {
    pub selection_mode: u8,
    pub transforming: bool,
}

impl StateExtra {
    pub fn of(app: &PhotocraftApp) -> Self {
        Self { selection_mode: app.ui.selection_mode, transforming: app.ui.transform.is_some() }
    }
}

/// The digest the host mirrors, from `control::inspect` plus [`StateExtra`]. Pure: tests feed a
/// sample. `seq` is added by [`Digest`].
pub fn state_from(inspect: &Value, extra: StateExtra) -> Value {
    let d = &inspect["document"];
    let mut layers = Vec::new();
    flat_layers(&d["layers"], 0, &mut layers);
    let dialog = inspect["dialogs"].as_array().and_then(|a| a.last()).cloned().unwrap_or(Value::Null);
    json!({
        "type": "lc-pc-state",
        "rev": d["revision"],
        "tool": inspect["tool"],
        "selectionMode": extra.selection_mode,
        "wand": {"tolerance": inspect["toolOptions"]["tolerance"], "contiguous": inspect["toolOptions"]["contiguous"]},
        "brush": inspect["brush"],
        "fg": hex(&inspect["session"]["foreground"]),
        "doc": if d.is_null() { Value::Null } else { json!({
            "w": d["width"], "h": d["height"], "dirty": d["dirty"], "canUndo": d["canUndo"], "canRedo": d["canRedo"],
            "hasSelection": d["hasSelection"], "selectionBounds": d["selectionBounds"],
        }) },
        "layers": layers,
        "active": d["activeLayer"],
        "typing": !inspect["textEdit"].is_null(),
        "transforming": extra.transforming,
        "dialog": dialog,
    })
}

/// When to build the digest and what to send. Every state that goes out carries `seq`, counted
/// here from 1 and only ever increasing: a document's `revision` restarts with each document, so
/// the host orders states by `seq` alone.
#[derive(Default)]
pub struct Digest {
    seq: u64,
    last: String,
    built_at: Option<f64>,
}

impl Digest {
    /// Build a digest this frame? Right after a reply, else at most every `STATE_INTERVAL_MS`.
    pub fn due(&self, now_ms: f64, replied: bool) -> bool {
        replied || self.built_at.is_none_or(|t| now_ms - t >= STATE_INTERVAL_MS)
    }

    /// Send the next state even if unchanged (a new listener); `seq` keeps counting.
    pub fn resend(&mut self) {
        self.last.clear();
        self.built_at = None;
    }

    /// The state to send, with its `seq`, or `None` when nothing changed since the last one.
    pub fn next(&mut self, mut state: Value, now_ms: f64) -> Option<Value> {
        self.built_at = Some(now_ms);
        let sig = state.to_string();
        if sig == self.last {
            return None;
        }
        self.last = sig;
        self.seq += 1;
        state["seq"] = self.seq.into();
        Some(state)
    }
}

/// `ab.toolOptions {wandTolerance?, wandContiguous?}` (not in `ui.set`). Unknown fields or a bad
/// value refuse the whole call.
pub fn set_tool_options(app: &mut PhotocraftApp, p: &Value) -> Result<Value, String> {
    let o = p.as_object().ok_or("`ab.toolOptions` takes an object")?;
    if let Some(k) = o.keys().find(|k| !["wandTolerance", "wandContiguous"].contains(&k.as_str())) {
        return Err(format!("field `{k}` is not available here"));
    }
    let tolerance = match o.get("wandTolerance") {
        Some(t) => Some(t.as_f64().filter(|t| (0.0..=255.0).contains(t)).ok_or("wandTolerance must be 0..255")?),
        None => None,
    };
    let contiguous = match o.get("wandContiguous") {
        Some(c) => Some(c.as_bool().ok_or("wandContiguous must be a boolean")?),
        None => None,
    };
    if let Some(t) = tolerance {
        app.ui.tool_options.tolerance = t as f32;
    }
    if let Some(c) = contiguous {
        app.ui.tool_options.contiguous = c;
    }
    Ok(json!({"tolerance": app.ui.tool_options.tolerance, "contiguous": app.ui.tool_options.contiguous}))
}

/// `ab.layer.thumbs {ids, size?}`: the Layers panel's square, letterboxed thumbnails as straight
/// (unpremultiplied) RGBA8 in base64, `size`² pixels (default 64, 16..=128; at most 16 ids).
/// Layers without pixels (adjustments, groups) return `rgba: null`; unknown ids are skipped.
pub fn layer_thumbs(app: &PhotocraftApp, p: &Value) -> Result<Value, String> {
    use base64::Engine as _;
    let size = p.get("size").and_then(Value::as_u64).unwrap_or(64).clamp(16, THUMB_MAX) as usize;
    let ids: Vec<u64> = p.get("ids").and_then(Value::as_array).ok_or("missing `ids`")?.iter().filter_map(Value::as_u64).take(THUMBS_PER_CALL).collect();
    let doc = &app.session.active().ok_or("no document")?.doc;
    let mut out = Vec::new();
    for id in ids {
        let Some(layer) = doc.layer(photocraft_doc::LayerId(id)) else { continue };
        let Some(s) = layer.surface() else {
            out.push(json!({"id": id, "rgba": null}));
            continue;
        };
        let mut px = [0.0f32; 8];
        let img = crate::thumb_image(doc, size, |x, y| {
            let n = s.channels();
            s.read_pixel(x, y, &mut px[..n]);
            photocraft_raster::to_rgba(&s.format(), &px[..n])
        });
        let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
        out.push(json!({
            "id": id, "w": img.size[0], "h": img.size[1], "rev": crate::surface_fingerprint(s).to_string(),
            "rgba": base64::engine::general_purpose::STANDARD.encode(bytes),
        }));
    }
    Ok(json!({"thumbs": out}))
}

/// Run an `ab.*` method (they need `&mut app`, so the web shell calls this from its frame).
pub fn run_ab(app: &mut PhotocraftApp, method: &str, params: &Value) -> Result<Value, String> {
    match method {
        "ab.layer.thumbs" => layer_thumbs(app, params),
        "ab.toolOptions" => set_tool_options(app, params),
        _ => Err(format!("method `{method}` is not available here")),
    }
}

#[cfg(test)]
#[path = "ab_bridge_tests.rs"]
mod tests;
