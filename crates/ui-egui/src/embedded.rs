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

/// Whether embedded mode is on.
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
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
/// embedded mode turns on, after the app's own theme is set up. Empty for now; the host's palette
/// (egui `Visuals`) lands here in a later step, read with `ctx.global_style()` / `ctx.set_visuals`.
pub fn apply_embedded_visuals(ctx: &egui::Context) {
    if !on() {
        return;
    }
    let _ = ctx;
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
}
