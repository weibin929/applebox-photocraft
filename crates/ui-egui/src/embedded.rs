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
}
