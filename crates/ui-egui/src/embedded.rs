//! Embedded mode (Apple Box fork, see `AB_FORK.md`): the editor runs inside another product's page.
//! The host turns it on once at start-up (`set_on(true)`); from then on product names, community
//! links and the brand mark stay out of the interface, and the Help links / About / Print commands
//! are hidden from the menus and refused when invoked. Everything else is unchanged.
//!
//! The pure helpers take `on: bool` so tests cover both modes without touching the global flag.

use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(false);

/// Turn embedded mode on or off (the web shell calls this from the host page).
pub fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

#[cfg(test)]
thread_local! {
    // Tests that run whole frames switch the mode for their own thread only (tests run in parallel).
    static THIS_THREAD: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Test only: embedded mode on/off for the calling thread (`None` = the global flag).
#[cfg(test)]
pub(crate) fn set_on_for_this_thread(on: Option<bool>) {
    THIS_THREAD.with(|t| t.set(on));
}

/// Whether embedded mode is on.
pub fn on() -> bool {
    #[cfg(test)]
    if let Some(on) = THIS_THREAD.with(|t| t.get()) {
        return on;
    }
    ON.load(Ordering::Relaxed)
}

/// Embedded mode draws only the canvas: no title/options/status bars, toolbar, dock, document
/// tabs, Home Screen, floating panels, dialogs or canvas context menus. The host page owns every
/// other control (through the bridge, `ab_bridge`).
pub fn canvas_only() -> bool {
    on()
}

/// The product name shown to users in embedded mode.
pub const NEUTRAL_NAME: &str = "Editor";

/// Commands that leave the menus (and refuse to run) in embedded mode: community links, About and
/// printing (a browser download of a print page makes no sense inside a host page).
pub const HIDDEN_COMMANDS: &[&str] =
    &["help.discord", "help.website", "help.artcraftWebsite", "help.github", "help.reportIssue", "help.about", "file.print", "file.printOneCopy"];

/// `app_name_for(on())`.
pub fn app_name() -> &'static str {
    app_name_for(on())
}

/// "PhotoCraft", or the neutral name in embedded mode.
pub fn app_name_for(on: bool) -> &'static str {
    if on { NEUTRAL_NAME } else { "PhotoCraft" }
}

/// `brand_for(on(), s)`.
pub fn brand(s: &str) -> String {
    brand_for(on(), s)
}

/// A user-visible string with the product name replaced by the neutral name in embedded mode
/// (both spellings: "PhotoCraft" and the crate-style "photocraft").
pub fn brand_for(on: bool, s: &str) -> String {
    if on { s.replace("PhotoCraft", NEUTRAL_NAME).replace("photocraft", &NEUTRAL_NAME.to_ascii_lowercase()) } else { s.to_string() }
}

/// Whether `id` is hidden and refused in embedded mode (`on` given).
pub fn hidden_for(on: bool, id: &str) -> bool {
    on && HIDDEN_COMMANDS.contains(&id)
}

/// `hidden_for(on(), id)`.
pub fn hidden(id: &str) -> bool {
    hidden_for(on(), id)
}

/// Hook for the host's colour scheme, kept apart from the brand hiding above: called once when
/// embedded mode turns on, after the app's own theme is set up (`theme::apply` also calls it, so a
/// later theme change keeps the host look). The palette itself lives in `applebox_theme`.
pub fn apply_embedded_visuals(ctx: &egui::Context) {
    crate::applebox_theme::apply_if(ctx, on());
}

/// Drop the hidden commands from a menu list (`menus::menu_items`).
pub fn filter_menu_for(on: bool, items: &mut Vec<crate::menus::MenuItem>) {
    if on {
        items.retain(|i| !HIDDEN_COMMANDS.contains(&i.id.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every user-visible string the fork rewrites, in its upstream form.
    const BRANDED: &[&str] = &[
        "PhotoCraft",
        "About PhotoCraft",
        "PhotoCraft — an open-source, native image editor written in Rust.",
        "PhotoCraft Print Settings",
        "PhotoCraft Manages Colors",
        "Save your work and restart PhotoCraft to retry GPU acceleration. CPU rendering remains active for this session.",
        "PhotoCraft is using the CPU image compositor. The window may still use your graphics adapter.",
        "PhotoCraft keeps retrying; until a save succeeds, preference changes are lost when it closes.",
        "These settings aren't available in PhotoCraft yet.",
        "egui · wgpu · photocraft-engine",
    ];

    #[test]
    fn embedded_strings_carry_no_product_name() {
        for s in BRANDED {
            let out = brand_for(true, s);
            assert!(!out.contains("PhotoCraft") && !out.contains("photocraft"), "{s:?} -> {out:?}");
            assert!(out.contains(NEUTRAL_NAME) || out.contains(&NEUTRAL_NAME.to_ascii_lowercase()), "{out:?}");
        }
        assert_eq!(app_name_for(true), NEUTRAL_NAME);
    }

    #[test]
    fn normal_mode_is_untouched() {
        for s in BRANDED {
            assert_eq!(brand_for(false, s), *s);
        }
        assert_eq!(app_name_for(false), "PhotoCraft");
        assert!(!hidden_for(false, "help.about"));
    }

    #[test]
    fn embedded_menus_drop_links_about_and_print() {
        let app = crate::PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let mut items = crate::menus::menu_items(&app);
        let before = items.len();
        assert!(items.iter().any(|i| i.id == "help.about") && items.iter().any(|i| i.id == "file.print"));
        filter_menu_for(true, &mut items);
        assert!(items.len() < before);
        for id in HIDDEN_COMMANDS {
            assert!(!items.iter().any(|i| i.id == *id), "{id}");
            assert!(hidden_for(true, id), "{id}");
        }
        // No label in the remaining menu names the product.
        for i in &items {
            assert!(!i.label.contains("PhotoCraft"), "{}: {}", i.id, i.label);
        }
    }

    /// Whole app, real frames: embedded draws no button, menu, tab, panel or dialog (only the
    /// canvas on the host's backdrop); off, upstream's chrome is back.
    #[test]
    fn canvas_only_draws_no_chrome_and_off_is_upstream() {
        use egui::accesskit::Role;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let harness = || {
            let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(64).build_eframe(|cc| {
                crate::PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut s = photocraft_engine::Session::new();
                s.execute("file.new", serde_json::json!({"width": 64, "height": 64})).unwrap();
                crate::PhotocraftApp::new(s, crate::Services::default())
            });
            h.run_steps(4);
            // A dialog the user could have opened from a menu (Gaussian Blur…).
            let ctx = h.ctx.clone();
            crate::menus::invoke(h.state_mut(), &ctx, "filter.blur.gaussianBlur", serde_json::json!({})).unwrap();
            h.run_steps(4);
            assert!(!h.state().ui.dialogs.is_empty());
            h
        };
        let widgets = |h: &Harness<'_, crate::PhotocraftApp>| {
            [Role::Button, Role::MenuItem, Role::Tab, Role::CheckBox, Role::Slider, Role::TextInput, Role::Window]
                .into_iter()
                .map(|r| h.query_all_by_role(r).count())
                .sum::<usize>()
        };

        set_on_for_this_thread(Some(true));
        let h = harness();
        let on = widgets(&h);
        set_on_for_this_thread(Some(false));
        let h2 = harness();
        let off = widgets(&h2);
        set_on_for_this_thread(None);
        drop((h, h2));
        assert_eq!(on, 0, "embedded must draw only the canvas");
        assert!(off >= 10, "off = upstream chrome ({off} widgets)");
    }

    // ── Whole-app regressions for canvas_only (real frames, this thread only) ──────────────────

    type H = egui_kittest::Harness<'static, crate::PhotocraftApp>;

    fn app_with(build: impl FnOnce(&mut photocraft_engine::Session) + 'static, tool: crate::state::Tool) -> H {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(64).build_eframe(move |cc| {
            crate::PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut s = photocraft_engine::Session::new();
            s.execute("file.new", serde_json::json!({"width": 64, "height": 64})).unwrap();
            build(&mut s);
            let mut app = crate::PhotocraftApp::new(s, crate::Services::default());
            app.ui.tool = tool;
            app
        });
        h.run_steps(4);
        h
    }

    fn widgets(h: &H) -> usize {
        use egui::accesskit::Role;
        use egui_kittest::kittest::Queryable;
        [Role::Button, Role::MenuItem, Role::Tab, Role::CheckBox, Role::Slider, Role::TextInput, Role::Window]
            .into_iter()
            .map(|r| h.query_all_by_role(r).count())
            .sum()
    }

    fn click(h: &mut H, button: egui::PointerButton) {
        let p = h.state().last_canvas_rect.center();
        h.event(egui::Event::PointerMoved(p));
        h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: p, button, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: p, button, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(3);
    }

    #[test]
    fn canvas_only_right_click_opens_no_picker_or_menu() {
        use crate::state::Tool;
        set_on_for_this_thread(Some(true));
        for tool in [Tool::Brush, Tool::Eraser, Tool::Move, Tool::RectMarquee] {
            let mut h = app_with(|_| {}, tool);
            click(&mut h, egui::PointerButton::Secondary);
            let ui = &h.state().ui;
            assert!(ui.brush_picker.is_none() && ui.layer_menu.is_none() && ui.canvas_tool_menu.is_none(), "{tool:?}: a canvas pop-up opened");
            assert_eq!(widgets(&h), 0, "{tool:?}: something besides the canvas is drawn");
        }
        // A picker left open from before the switch is not drawn either.
        let mut h = app_with(|_| {}, Tool::Brush);
        h.state_mut().ui.brush_picker = Some([300.0, 300.0]);
        h.run_steps(3);
        assert_eq!(widgets(&h), 0);
        set_on_for_this_thread(Some(false));
        let mut h = app_with(|_| {}, Tool::Brush);
        click(&mut h, egui::PointerButton::Secondary);
        assert!(h.state().ui.brush_picker.is_some(), "off: upstream's Brush Preset picker");
        set_on_for_this_thread(None);
    }

    #[test]
    fn canvas_only_never_draws_the_home_screen_over_a_document() {
        set_on_for_this_thread(Some(false));
        let mut h = app_with(|_| {}, crate::state::Tool::Brush);
        h.state_mut().ui.chrome.home = Some(1); // Home Screen shown with one document open
        h.run_steps(3);
        let upstream = widgets(&h);
        set_on_for_this_thread(Some(true));
        h.run_steps(3);
        let embedded = widgets(&h);
        set_on_for_this_thread(None);
        assert!(upstream > 0);
        assert_eq!(embedded, 0, "embedded: the canvas, not the Home Screen");
    }

    #[test]
    fn canvas_only_delete_clears_the_selected_pixels() {
        set_on_for_this_thread(Some(true));
        let mut h = app_with(
            |s| {
                s.execute("layer.new.layer", serde_json::json!({})).unwrap();
                s.execute("select.rect", serde_json::json!({"x": 8, "y": 8, "width": 16, "height": 16})).unwrap();
                s.execute("edit.fill", serde_json::json!({"color": "#ff0000"})).unwrap();
            },
            crate::state::Tool::RectMarquee,
        );
        let alpha = |h: &H| {
            let st = h.state().session.active().unwrap();
            let s = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap();
            let mut px = [0.0f32; 8];
            let n = s.channels();
            s.read_pixel(12, 12, &mut px[..n]);
            px[n - 1]
        };
        assert!(alpha(&h) > 0.9);
        // The key passes the embedded filter, then the app clears the selection's pixels.
        let mut ev = vec![egui::Event::Key { key: egui::Key::Delete, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }];
        let mut tool = h.state().ui.tool;
        crate::ab_bridge::filter_canvas_input(&mut tool, &mut ev);
        assert_eq!(ev.len(), 1);
        h.key_press(egui::Key::Delete);
        h.run_steps(3);
        set_on_for_this_thread(None);
        assert!(alpha(&h) < 0.01, "Delete clears the selected pixels in embedded mode");
    }

    /// The rasterize prompt in embedded mode: not drawn, reported in the state digest, answered
    /// by the host with ui.dialog.confirm / ui.dialog.cancel.
    #[test]
    fn canvas_only_rasterize_prompt_goes_through_state_and_host_answers() {
        use crate::ab_bridge::{StateExtra, request_allowed, state_from};
        use crate::control::{ControlRequest, Outcome, handle, inspect};
        set_on_for_this_thread(Some(true));
        for answer in ["ui.dialog.cancel", "ui.dialog.confirm"] {
            let mut h = app_with(
                |s| {
                    s.execute("type.create", serde_json::json!({"x": 4, "y": 40, "text": "AB", "size": 30})).unwrap();
                },
                crate::state::Tool::Brush,
            );
            click(&mut h, egui::PointerButton::Primary);
            assert_eq!(widgets(&h), 0, "the prompt is not drawn");
            let ctx = h.ctx.clone();
            let s = state_from(&inspect(h.state(), &ctx), &StateExtra::of(h.state()));
            assert_eq!(s["dialog"]["fields"]["__rasterize"], "type", "{}", s["dialog"]);
            let active = s["active"].as_u64().unwrap();
            let kind_of = |s: &serde_json::Value| s["layers"].as_array().unwrap().iter().find(|l| l["id"] == active).unwrap()["kind"].clone();
            assert_eq!(kind_of(&s), "type");
            let params = serde_json::json!({"dialog": s["dialog"]["id"]});
            request_allowed(answer, &params).unwrap();
            let (req, _rx) = ControlRequest::new(answer, params);
            if let Outcome::Done(v) = handle(h.state_mut(), &ctx, &req) {
                assert_eq!(v["ok"], true, "{answer}: {v}");
            }
            h.run_steps(3);
            assert!(h.state().ui.dialogs.is_empty(), "{answer} closes it");
            let s = state_from(&inspect(h.state(), &ctx), &StateExtra::of(h.state()));
            assert!(s["dialog"].is_null());
            let expect = if answer == "ui.dialog.confirm" { "pixel" } else { "type" };
            assert_eq!(kind_of(&s), expect, "{answer}");
        }
        set_on_for_this_thread(None);
    }
}
