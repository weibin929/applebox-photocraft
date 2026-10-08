use super::*;
use crate::control::{ControlRequest, Outcome, handle, inspect};
use egui::{Key, Modifiers};
use photocraft_doc::LayerId;

// ── Policy ──────────────────────────────────────────────────────────────────────────────────────

#[test]
fn every_allowed_command_exists_upstream() {
    // Contract: an upstream rebase that renames or drops a command turns this red.
    for id in COMMANDS {
        let engine = photocraft_engine::commands::find(id).is_some();
        let ui = crate::menus::UI_COMMANDS.iter().any(|c| c.0 == *id);
        assert!(engine || ui, "{id} is gone upstream");
    }
    for f in UI_SET {
        assert!(crate::control::UI_SET_FIELDS.contains(f), "ui.set field {f} is gone upstream");
    }
    for b in BLENDS {
        assert!(photocraft_color::BlendMode::LAYER_MODES.iter().any(|m| m.label() == *b), "blend {b} is gone upstream");
    }
}

#[test]
fn refuses_everything_outside_the_first_batch() {
    for (m, p) in [
        ("app.save", json!({})),
        ("ui.menu.invoke", json!({"id": "file.saveAs"})),
        ("ui.screenshot", json!({})),
        ("ui.pointer", json!({"events": []})),
        ("ui.type", json!({"text": "x"})),
        ("engine.execute", json!({"command": "file.open"})),
        ("engine.execute", json!({"command": "prefs.set"})),
        ("engine.execute", json!({"command": "layer.delete"})),
        ("engine.execute", json!({"command": "edit.transform"})),
        ("engine.execute", json!({"command": "edit.transform.flipHorizontal"})),
        ("engine.execute", json!({"command": "select.inverse"})),
        ("engine.execute", json!({"command": "image.adjustments.invert"})),
        ("engine.execute", json!({"command": "image.adjustments.invert", "params": {"target": "pixels"}})),
        ("engine.execute", json!({"command": "layer.setProps", "params": {"fill": 0.5}})),
        ("engine.execute", json!({"command": "layer.setProps", "params": {"blend": "Luminosity"}})),
        ("engine.execute", json!({})),
        ("ui.set", json!({"tool": "gradient"})),
        ("ui.set", json!({"tool": "quickSelection"})),
        ("ui.set", json!({"tool": "polygonLasso"})),
        ("ui.set", json!({"tool": 3})),
        ("ui.set", json!({"theme": "classic"})),
        ("ui.set", json!({"panels": {"layers": true}})),
        ("ui.key", json!({"key": "S", "command": true})),
        ("ui.key", json!({"key": "Enter", "modifiers": {"shift": true}})),
        ("ui.key", json!({"key": "Tab"})),
    ] {
        assert!(request_allowed(m, &p).is_err(), "{m} {p} should be refused");
    }
}

#[test]
fn allows_the_first_batch() {
    for (m, p) in [
        ("engine.execute", json!({"command": "edit.undo"})),
        ("engine.execute", json!({"command": "edit.freeTransform"})),
        ("engine.execute", json!({"command": "layer.moveTo", "params": {"layer": 3, "target": 2}})),
        ("engine.execute", json!({"command": "image.adjustments.invert", "params": {"target": "mask"}})),
        ("engine.execute", json!({"command": "layer.setProps", "params": {"layer": 3, "opacity": 0.5, "blend": "Multiply"}})),
        ("ui.set", json!({"tool": "magicWand", "selectionMode": 1})),
        // `ui.set` reads tool names loosely; the gate parses them the same way.
        ("ui.set", json!({"tool": "MagicWand"})),
        ("ui.set", json!({"tool": "rect marquee"})),
        ("ui.key", json!({"key": "Enter"})),
        ("ui.key", json!({"key": "Escape"})),
        ("ab.layer.thumbs", json!({"ids": [1]})),
        ("ab.toolOptions", json!({"wandTolerance": 32})),
    ] {
        assert!(request_allowed(m, &p).is_ok(), "{m} {p} should pass");
    }
}

#[test]
fn keys() {
    assert!(key_allowed(Key::Z, Modifiers::COMMAND));
    assert!(key_allowed(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT));
    assert!(key_allowed(Key::D, Modifiers::COMMAND));
    assert!(key_allowed(Key::OpenBracket, Modifiers::NONE));
    assert!(key_allowed(Key::Enter, Modifiers::NONE));
    for (k, m) in [
        (Key::S, Modifiers::COMMAND),
        (Key::O, Modifiers::COMMAND),
        (Key::N, Modifiers::COMMAND),
        (Key::J, Modifiers::COMMAND),
        (Key::I, Modifiers::COMMAND),
        (Key::Tab, Modifiers::NONE),
        (Key::F, Modifiers::NONE),
        (Key::F1, Modifiers::NONE),
        (Key::Delete, Modifiers::NONE),
        (Key::Backspace, Modifiers::NONE),
        (Key::G, Modifiers::NONE),
        (Key::B, Modifiers::NONE), // letters go through tool_for_key, never to upstream
        (Key::W, Modifiers::NONE),
    ] {
        assert!(!key_allowed(k, m), "{k:?} {m:?} should be dropped");
    }
}

#[test]
fn tool_letters_map_to_the_first_batch_only() {
    let n = Modifiers::NONE;
    assert_eq!(tool_for_key(Key::W, n, Tool::Brush), Some(Tool::MagicWand));
    assert_eq!(tool_for_key(Key::V, n, Tool::Brush), Some(Tool::Move));
    assert_eq!(tool_for_key(Key::M, n, Tool::Brush), Some(Tool::RectMarquee));
    assert_eq!(tool_for_key(Key::M, n, Tool::RectMarquee), Some(Tool::EllipseMarquee));
    assert_eq!(tool_for_key(Key::L, Modifiers::SHIFT, Tool::Lasso), None);
    assert_eq!(tool_for_key(Key::T, Modifiers::COMMAND, Tool::Brush), None);
    assert_eq!(tool_for_key(Key::G, n, Tool::Brush), None);
    for k in [Key::V, Key::B, Key::E, Key::M, Key::L, Key::W, Key::T] {
        assert!(TOOLS.contains(&tool_for_key(k, n, Tool::Brush).unwrap()));
    }
}

#[test]
fn canvas_input_filter() {
    let key = |key, pressed, modifiers| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
    let mut tool = Tool::Brush;
    let mut ev = vec![
        key(Key::W, true, Modifiers::NONE),
        key(Key::W, false, Modifiers::NONE),
        key(Key::Z, true, Modifiers::COMMAND),
        key(Key::S, true, Modifiers::COMMAND),
        key(Key::C, true, Modifiers::COMMAND), // ⌘C after upstream's clipboard_keys
        key(Key::Tab, true, Modifiers::NONE),
        key(Key::Enter, true, Modifiers::NONE),
        egui::Event::Text("w".into()),
        egui::Event::Paste("x".into()),
        egui::Event::PointerMoved(egui::pos2(1.0, 2.0)),
    ];
    filter_canvas_input(&mut tool, &mut ev);
    assert_eq!(tool, Tool::MagicWand, "W picks the Magic Wand, not upstream's Quick Selection");
    assert_eq!(ev, vec![key(Key::Z, true, Modifiers::COMMAND), key(Key::Enter, true, Modifiers::NONE), egui::Event::PointerMoved(egui::pos2(1.0, 2.0))]);
    // M toggles rectangle/ellipse once per press, not per auto-repeat.
    let mut tool = Tool::RectMarquee;
    let mut ev =
        vec![key(Key::M, true, Modifiers::NONE), egui::Event::Key { key: Key::M, physical_key: None, pressed: true, repeat: true, modifiers: Modifiers::NONE }];
    filter_canvas_input(&mut tool, &mut ev);
    assert_eq!((tool, ev.len()), (Tool::EllipseMarquee, 0));
}

#[test]
fn state_digest_shape() {
    let inspect = json!({
        "tool": "Brush", "toolOptions": {"tolerance": 32.0, "contiguous": true},
        "brush": {"size": 24.0, "hardness": 0.8, "opacity": 1.0}, "session": {"foreground": [1.0, 0.239, 0.498, 1.0]},
        "textEdit": null, "dialogs": [],
        "document": {"revision": 9, "width": 64, "height": 64, "dirty": true, "canUndo": true, "canRedo": false,
            "hasSelection": false, "selectionBounds": null, "activeLayer": 2,
            "layers": [{"id": 2, "name": "a", "kind": "Pixel", "visible": true, "opacity": 1.0, "blend": "Normal", "hasMask": false, "selected": true},
                       {"id": 1, "name": "g", "kind": "Group", "visible": true, "opacity": 1.0, "blend": "Normal", "hasMask": false, "selected": false,
                        "children": [{"id": 4, "name": "c", "kind": "Pixel", "visible": false, "opacity": 0.5, "blend": "Multiply", "hasMask": true, "selected": false}]}]}
    });
    let extra = StateExtra { selection_mode: 2, transforming: true, mask_target: true, thumb_revs: [(4, "77".to_string())].into() };
    let s = state_from(&inspect, &extra);
    assert_eq!(s["type"], "lc-pc-state");
    assert_eq!(s["rev"], 9);
    assert_eq!(s["tool"], "brush", "camelCase, the names ui.set takes");
    assert_eq!(s["fg"], "#ff3d7f");
    assert_eq!(s["selectionMode"], 2);
    assert_eq!(s["transforming"], true);
    assert_eq!(s["wand"]["tolerance"], 32.0);
    assert_eq!(s["doc"]["canUndo"], true);
    assert_eq!(s["doc"]["maskTarget"], true);
    assert_eq!(s["active"], 2);
    let ls = s["layers"].as_array().unwrap();
    assert_eq!(ls.len(), 3);
    assert_eq!((ls[2]["id"].as_u64(), ls[2]["depth"].as_u64(), ls[2]["blend"].as_str()), (Some(4), Some(1), Some("Multiply")));
    assert_eq!([ls[0]["kind"].as_str(), ls[1]["kind"].as_str()], [Some("pixel"), Some("group")]);
    assert_eq!((ls[2]["thumbRev"].as_str(), ls[0]["thumbRev"].is_null()), (Some("77"), true));
    let wand = state_from(&json!({"tool": "MagicWand", "document": null}), &StateExtra::default());
    assert_eq!(wand["tool"], "magicWand");
    assert_eq!(Tool::from_name("magicWand"), Some(Tool::MagicWand), "round-trips through ui.set's parser");
    assert!(s.get("seq").is_none(), "seq belongs to Digest");
    let empty = state_from(&json!({"tool": "Move", "document": null}), &StateExtra::default());
    assert!(empty["doc"].is_null());
    assert_eq!(empty["layers"], json!([]));
}

#[test]
fn state_seq_only_increases_even_when_revision_restarts() {
    let mut d = Digest::default();
    let st = |rev: u64, tool: &str| json!({"type": "lc-pc-state", "rev": rev, "tool": tool});
    assert!(d.due(0.0, false), "first state is due at once");
    let a = d.next(st(5, "Brush"), 0.0).unwrap();
    assert_eq!(a["seq"], 1);
    assert!(d.next(st(5, "Brush"), 10.0).is_none(), "unchanged state is not resent");
    // Undo adds to the revision; a newly opened document starts its own from 0.
    let b = d.next(st(6, "Brush"), 20.0).unwrap();
    let c = d.next(st(0, "Brush"), 30.0).unwrap();
    let e = d.next(st(0, "Move"), 40.0).unwrap();
    let seqs: Vec<u64> = [&a, &b, &c, &e].iter().map(|v| v["seq"].as_u64().unwrap()).collect();
    assert_eq!(seqs, [1, 2, 3, 4]);
    // Throttle: not due within the interval unless a reply just went out.
    assert!(!d.due(40.0 + STATE_INTERVAL_MS - 1.0, false));
    assert!(d.due(40.0 + STATE_INTERVAL_MS - 1.0, true));
    assert!(d.due(40.0 + STATE_INTERVAL_MS, false));
    // A new listener gets the same state again, and the count goes on.
    d.resend();
    assert!(d.due(41.0, false));
    assert_eq!(d.next(st(0, "Move"), 41.0).unwrap()["seq"], 5);
}

// ── Each bridge command, through the control protocol (headless app) ───────────────────────────

fn app64() -> (PhotocraftApp, egui::Context, u64) {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    app.run("file.new", json!({"width": 64, "height": 64})).unwrap();
    let layer = app.run("layer.new.layer", json!({"name": "p"})).unwrap()["layer"].as_u64().unwrap();
    app.run("select.rect", json!({"x": 8, "y": 8, "width": 16, "height": 16})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    (app, ctx, layer)
}

/// A host request: through the gate, then `control::handle`, exactly as the web shell does.
fn host(app: &mut PhotocraftApp, ctx: &egui::Context, method: &str, params: Value) -> Value {
    request_allowed(method, &params).unwrap_or_else(|e| panic!("{method} {params} refused: {e}"));
    if method.starts_with("ab.") {
        return run_ab(app, method, &params).unwrap_or_else(|e| panic!("{method}: {e}"));
    }
    let (req, _rx) = ControlRequest::new(method, params.clone());
    match handle(app, ctx, &req) {
        Outcome::Done(v) => {
            assert_eq!(v["ok"], true, "{method} {params}: {v}");
            v["result"].clone()
        }
        _ => panic!("{method}: expected an immediate reply"),
    }
}

fn exec(app: &mut PhotocraftApp, ctx: &egui::Context, command: &str, params: Value) -> Value {
    host(app, ctx, "engine.execute", json!({"command": command, "params": params}))
}

fn layer<'a>(app: &'a PhotocraftApp, id: u64) -> &'a photocraft_doc::Layer {
    app.session.active().unwrap().doc.layer(LayerId(id)).unwrap()
}

fn pixel(app: &PhotocraftApp, id: u64, x: i32, y: i32) -> [f32; 4] {
    let s = layer(app, id).surface().unwrap();
    let mut px = [0.0f32; 8];
    let n = s.channels();
    s.read_pixel(x, y, &mut px[..n]);
    photocraft_raster::to_rgba(&s.format(), &px[..n])
}

fn mask_at(app: &PhotocraftApp, id: u64, x: i32, y: i32) -> f32 {
    let mut v = [0.0f32; 1];
    layer(app, id).mask.as_ref().unwrap().surface.read_pixel(x, y, &mut v);
    v[0]
}

fn doc_json(app: &PhotocraftApp) -> Value {
    photocraft_engine::inspect::document(app.session.active().unwrap())
}

fn order(app: &PhotocraftApp) -> Vec<u64> {
    doc_json(app)["layers"].as_array().unwrap().iter().filter_map(|l| l["id"].as_u64()).collect()
}

#[test]
fn layer_select_set_props_move_to() {
    let (mut app, ctx, p) = app64();
    let bg = *order(&app).last().unwrap();
    assert_ne!(bg, p);
    assert_eq!(exec(&mut app, &ctx, "layer.select", json!({"layer": bg}))["selected"], json!([bg]));
    assert_eq!(app.session.active().unwrap().active_layer, Some(LayerId(bg)));
    exec(&mut app, &ctx, "layer.setProps", json!({"layer": p, "visible": false, "opacity": 0.5, "blend": "Multiply", "name": "改名"}));
    let l = layer(&app, p);
    assert_eq!((l.visible, l.opacity, l.blend.label(), l.name.as_str()), (false, 0.5, "Multiply", "改名"));
    let before = order(&app);
    exec(&mut app, &ctx, "layer.moveTo", json!({"layer": p, "target": bg, "position": "below"}));
    let after = order(&app);
    assert_ne!(before, after);
    assert_eq!(after.last(), Some(&p), "{after:?}");
}

#[test]
fn mask_add_invert_delete() {
    let (mut app, ctx, p) = app64();
    app.run("select.rect", json!({"x": 0, "y": 0, "width": 16, "height": 64})).unwrap(); // the user drags this on the canvas
    exec(&mut app, &ctx, "layer.layerMask.revealSelection", json!({"layer": p}));
    assert!(layer(&app, p).mask.is_some());
    let (inside, outside) = (mask_at(&app, p, 4, 4), mask_at(&app, p, 40, 4));
    assert!(inside > 0.9 && outside < 0.1, "{inside} {outside}");
    exec(&mut app, &ctx, "select.deselect", json!({}));
    let red = pixel(&app, p, 12, 12);
    host(&mut app, &ctx, "ui.set", json!({"maskTarget": true}));
    exec(&mut app, &ctx, "image.adjustments.invert", json!({"target": "mask"}));
    assert!(mask_at(&app, p, 4, 4) < 0.1 && mask_at(&app, p, 40, 4) > 0.9, "the mask flips");
    assert_eq!(pixel(&app, p, 12, 12), red, "the pixels don't");
    host(&mut app, &ctx, "ui.set", json!({"maskTarget": false}));
    exec(&mut app, &ctx, "layer.layerMask.delete", json!({"layer": p}));
    assert!(layer(&app, p).mask.is_none());
    exec(&mut app, &ctx, "layer.layerMask.revealAll", json!({"layer": p}));
    assert!(mask_at(&app, p, 40, 40) > 0.9);
}

#[test]
fn adjustment_layers_and_set_adjustment() {
    let (mut app, ctx, _) = app64();
    for (kind, params, field, value) in [
        ("brightnessContrast", json!({"brightness": 10, "contrast": 20}), "brightness", 40.0),
        ("hueSaturation", json!({"hue": 30, "saturation": 10}), "hue", -45.0),
        ("levels", json!({}), "gamma", 1.5),
    ] {
        let id = exec(&mut app, &ctx, &format!("layer.newAdjustmentLayer.{kind}"), params)["layer"].as_u64().unwrap();
        let before = doc_json(&app)["layers"].as_array().unwrap().iter().find(|l| l["id"] == id).unwrap()["adjustment"].clone();
        assert!(!before.is_null(), "{kind}");
        exec(&mut app, &ctx, "layer.setAdjustment", json!({"layer": id, field: value}));
        let after = doc_json(&app)["layers"].as_array().unwrap().iter().find(|l| l["id"] == id).unwrap()["adjustment"].clone();
        assert_ne!(before, after, "{kind}: setAdjustment {field} changed nothing");
    }
}

#[test]
fn type_create_edit_set_style_info() {
    let (mut app, ctx, _) = app64();
    let r = exec(&mut app, &ctx, "type.create", json!({"x": 4, "y": 30, "text": "AB", "size": 20}));
    let id = r["layer"].as_u64().unwrap();
    let b = r["bounds"].as_array().unwrap();
    assert!(b[2].as_i64().unwrap() > b[0].as_i64().unwrap(), "laid-out glyphs have width: {r}");
    exec(&mut app, &ctx, "type.edit", json!({"layer": id, "text": "XYZ"}));
    exec(&mut app, &ctx, "type.setStyle", json!({"layer": id, "size": 30, "color": "#ff3d7f"}));
    let info = exec(&mut app, &ctx, "type.info", json!({"layer": id}));
    assert_eq!(info["text"], "XYZ", "{info}");
    let style = &info["runs"][0]["style"];
    assert!(style.to_string().contains("30"), "{style}");
    exec(&mut app, &ctx, "type.updateAllTextLayers", json!({}));
}

#[test]
fn undo_redo_deselect_brush_colors() {
    let (mut app, ctx, p) = app64();
    exec(&mut app, &ctx, "layer.setProps", json!({"layer": p, "opacity": 0.25}));
    exec(&mut app, &ctx, "edit.undo", json!({}));
    assert_eq!(layer(&app, p).opacity, 1.0);
    exec(&mut app, &ctx, "edit.redo", json!({}));
    assert_eq!(layer(&app, p).opacity, 0.25);
    app.run("select.rect", json!({"x": 1, "y": 1, "width": 4, "height": 4})).unwrap();
    exec(&mut app, &ctx, "select.deselect", json!({}));
    assert!(app.session.active().unwrap().doc.selection.is_none());
    exec(&mut app, &ctx, "tools.setBrush", json!({"size": 24, "hardness": 0.5, "opacity": 0.8, "flow": 0.7}));
    assert_eq!(app.session.tools.brush.hardness, 0.5);
    exec(&mut app, &ctx, "tools.setColors", json!({"foreground": "#ff3d7f"}));
    let s = state_from(&inspect(&app, &ctx), &StateExtra::of(&app));
    assert_eq!(s["fg"], "#ff3d7f");
    host(&mut app, &ctx, "ui.set", json!({"tool": "magicWand", "selectionMode": 2, "brushSize": 30.0}));
    let s = state_from(&inspect(&app, &ctx), &StateExtra::of(&app));
    assert_eq!((s["tool"].as_str(), s["selectionMode"].as_u64()), (Some("magicWand"), Some(2)));
    assert_eq!(s["doc"]["maskTarget"], false);
    exec(&mut app, &ctx, "layer.layerMask.revealAll", json!({"layer": p}));
    host(&mut app, &ctx, "ui.set", json!({"maskTarget": true}));
    let m = state_from(&inspect(&app, &ctx), &StateExtra::of(&app));
    assert_eq!(m["doc"]["maskTarget"], true, "switching where the brush paints changes the digest");
    let mut d = Digest::default();
    assert!(d.next(s, 0.0).is_some() && d.next(m, 1.0).is_some());
}

#[test]
fn thumb_rev_follows_each_layers_pixels() {
    let (mut app, ctx, p) = app64();
    let q = app.run("layer.new.layer", json!({"name": "q"})).unwrap()["layer"].as_u64().unwrap();
    let revs = |app: &PhotocraftApp| {
        let s = state_from(&inspect(app, &ctx), &StateExtra::of(app));
        let get = |id: u64| s["layers"].as_array().unwrap().iter().find(|l| l["id"] == id).unwrap()["thumbRev"].clone();
        (get(p), get(q))
    };
    let (p0, q0) = revs(&app);
    assert!(p0.is_string() && q0.is_string());
    assert_eq!(p0, layer_thumbs(&app, &json!({"ids": [p]})).unwrap()["thumbs"][0]["rev"], "same value as ab.layer.thumbs");
    // Paint on q only.
    app.run("layer.select", json!({"layer": q})).unwrap();
    app.run("select.rect", json!({"x": 40, "y": 40, "width": 8, "height": 8})).unwrap();
    app.run("edit.fill", json!({"color": "#00ff00"})).unwrap();
    let (p1, q1) = revs(&app);
    assert_eq!(p1, p0, "an untouched layer keeps its fingerprint");
    assert_ne!(q1, q0, "the painted layer's fingerprint changes");
}

#[test]
fn tool_options_and_thumbs() {
    let (mut app, ctx, p) = app64();
    let r = host(&mut app, &ctx, "ab.toolOptions", json!({"wandTolerance": 12, "wandContiguous": false}));
    assert_eq!(r, json!({"tolerance": 12.0, "contiguous": false}));
    assert!(set_tool_options(&mut app, &json!({"wandTolerance": 999})).is_err());
    assert!(set_tool_options(&mut app, &json!({"wandContiguous": true, "feather": 2})).is_err());
    assert!(!app.ui.tool_options.contiguous, "a refused call changes nothing");
    let s = state_from(&inspect(&app, &ctx), &StateExtra::of(&app));
    assert_eq!(s["wand"], json!({"tolerance": 12.0, "contiguous": false}));

    let adj = exec(&mut app, &ctx, "layer.newAdjustmentLayer.levels", json!({}))["layer"].as_u64().unwrap();
    let r = host(&mut app, &ctx, "ab.layer.thumbs", json!({"ids": [p, adj, 9999], "size": 32}));
    let thumbs = r["thumbs"].as_array().unwrap();
    assert_eq!(thumbs.len(), 2, "unknown ids are skipped: {r}");
    let t = &thumbs[0];
    let (w, h) = (t["w"].as_u64().unwrap() as usize, t["h"].as_u64().unwrap() as usize);
    assert_eq!((w, h), (32, 32));
    use base64::Engine as _;
    let rgba = base64::engine::general_purpose::STANDARD.decode(t["rgba"].as_str().unwrap()).unwrap();
    assert_eq!(rgba.len(), w * h * 4);
    // The red square (8..24 of 64) lands at 4..12 of 32: straight red, opaque; outside it transparent.
    assert_eq!(&rgba[(8 * w + 8) * 4..(8 * w + 8) * 4 + 4], &[255, 0, 0, 255]);
    assert_eq!(rgba[(28 * w + 28) * 4 + 3], 0);
    assert!(thumbs[1]["rgba"].is_null(), "adjustment layers have no pixels");
    let rev = t["rev"].clone();
    exec(&mut app, &ctx, "layer.select", json!({"layer": p}));
    app.run("edit.fill", json!({"color": "#00ff00"})).unwrap();
    let again = layer_thumbs(&app, &json!({"ids": [p]})).unwrap();
    assert_ne!(again["thumbs"][0]["rev"], rev, "rev follows the pixels");
    assert_eq!(again["thumbs"][0]["w"], 64, "default size");
}

// ── Enter / Escape on a Free Transform: synthetic keys need frames ──────────────────────────────

#[test]
fn free_transform_enter_commits_escape_cancels() {
    use egui_kittest::Harness;
    let (tx, rx) = std::sync::mpsc::channel::<ControlRequest>();
    let rx = std::cell::RefCell::new(Some(rx));
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 700.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let (app, _, _) = app64();
        app.with_control(rx.borrow_mut().take().unwrap())
    });
    h.run_steps(4);
    let send = |h: &mut Harness<'_, PhotocraftApp>, method: &str, params: Value| -> Value {
        request_allowed(method, &params).unwrap();
        let (req, reply) = ControlRequest::new(method, params);
        tx.send(req).unwrap();
        for _ in 0..30 {
            // The web shell's eframe feeds queued synthetic input through `raw_input_hook`; the harness doesn't.
            for e in h.state_mut().take_synthetic_step() {
                h.event(e);
            }
            h.run_steps(1);
            if let Ok(v) = reply.try_recv() {
                return v;
            }
        }
        panic!("{method}: no reply");
    };
    let p = h.state().session.active().unwrap().active_layer.unwrap().0;
    let red = |h: &Harness<'_, PhotocraftApp>, x, y| pixel(h.state(), p, x, y)[0] > 0.9;
    assert!(red(&h, 10, 10) && !red(&h, 40, 10));

    assert_eq!(send(&mut h, "engine.execute", json!({"command": "edit.freeTransform"}))["ok"], true);
    assert!(h.state().ui.transform.is_some());
    assert_eq!(send(&mut h, "ui.key", json!({"key": "Escape"}))["ok"], true);
    assert!(h.state().ui.transform.is_none(), "Escape cancels");
    assert!(red(&h, 10, 10));

    send(&mut h, "engine.execute", json!({"command": "edit.freeTransform"}));
    // Drag the box 24 px to the right (what a pointer drag inside it does), then Enter.
    for q in h.state_mut().ui.transform.as_mut().unwrap().quad.iter_mut() {
        q[0] += 24.0;
    }
    let rev = h.state().session.active().unwrap().revision;
    assert_eq!(send(&mut h, "ui.key", json!({"key": "Enter"}))["ok"], true);
    assert!(h.state().ui.transform.is_none(), "Enter commits");
    assert!(h.state().session.active().unwrap().revision > rev, "the commit ran (edit.transform, which the host can't call itself)");
    assert!(!red(&h, 10, 10) && red(&h, 40, 10), "pixels moved");
}
