//! Apple Box embedded look (AB_FORK.md): the host product's dark palette applied to egui's
//! `Visuals` and `Spacing` when embedded mode is on. Appearance only: no panel, command or file
//! behaviour changes. The colour table lives here and nowhere else.

use egui::{CornerRadius, Stroke, Visuals};

/// Colour table (the host's dark tokens; the accent is its "Sexy Pink").
pub mod cis {
    use egui::Color32;
    pub const NIGHT: Color32 = Color32::from_rgb(0x12, 0x0C, 0x14); // window/canvas surround
    pub const PANEL: Color32 = Color32::from_rgb(0x1E, 0x15, 0x21); // panels
    pub const HOVER: Color32 = Color32::from_rgb(0x2A, 0x1F, 0x2E); // hover fill
    pub const INK: Color32 = Color32::from_rgb(0xFF, 0xF0, 0xF5); // primary text
    pub const INK_2: Color32 = Color32::from_rgb(0xC9, 0xB3, 0xBF); // secondary text
    pub const DIM: Color32 = Color32::from_rgb(0x9A, 0x85, 0x92); // disabled text
    pub const LINE: Color32 = Color32::from_rgb(0x33, 0x24, 0x3A); // separators
    pub const LINE_2: Color32 = Color32::from_rgb(0x47, 0x34, 0x4F); // field borders
    pub const PINK: Color32 = Color32::from_rgb(0xFF, 0x3D, 0x7F); // accent: selection, focus, pressed
    pub const PINK_TEXT: Color32 = Color32::from_rgb(0xFF, 0x6B, 0xA3); // pink text, links
    pub const ON_PINK: Color32 = NIGHT; // text on the pink fill (white would only reach 3.37:1)
    pub const SEL_BG: Color32 = Color32::from_rgb(0x3A, 0x1C, 0x30); // selected rows / pressed (keeps INK readable)
    pub const RADIUS_SMALL: u8 = 10;
    pub const RADIUS_CARD: u8 = 18;
}

/// egui visuals in the host palette (starting from `Visuals::dark()`).
pub fn applebox_visuals() -> Visuals {
    use cis::*;
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = NIGHT;
    v.faint_bg_color = HOVER;
    v.code_bg_color = NIGHT;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(RADIUS_CARD);
    v.menu_corner_radius = CornerRadius::same(RADIUS_SMALL);
    v.hyperlink_color = PINK_TEXT;
    v.override_text_color = Some(INK);
    v.selection.bg_fill = PINK;
    v.selection.stroke = Stroke::new(1.0, ON_PINK);
    for (w, fill, stroke) in [
        (&mut v.widgets.noninteractive, PANEL, LINE),
        (&mut v.widgets.inactive, PANEL, LINE_2),
        (&mut v.widgets.hovered, HOVER, PINK),
        (&mut v.widgets.active, PINK, PINK),
        (&mut v.widgets.open, HOVER, PINK),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.corner_radius = CornerRadius::same(RADIUS_SMALL);
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK_2);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);
    v.widgets.active.fg_stroke = Stroke::new(1.0, ON_PINK);
    v.widgets.open.fg_stroke = Stroke::new(1.0, INK);
    v
}

/// Spacing aligned with the host's 6–8 px rhythm.
pub fn applebox_spacing(s: &mut egui::Spacing) {
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.button_padding = egui::vec2(10.0, 5.0);
    s.window_margin = egui::Margin::same(12);
}

/// The app's own colour tokens (most panels paint from these, not from egui's `Visuals`) mapped to
/// the host palette. Layout flags (`pro`, `bevel`, radii) stay as the active theme set them;
/// warning/danger and the scope colours stay too (their meaning is the colour).
pub fn applebox_tokens(base: crate::theme::Tokens) -> crate::theme::Tokens {
    use cis::*;
    let mut t = base;
    t.chrome = NIGHT;
    t.canvas = NIGHT;
    t.canvas_dot = LINE;
    t.dock = NIGHT;
    t.card = PANEL;
    t.card_border = LINE;
    t.field = NIGHT;
    t.field_border = LINE_2;
    t.hover = HOVER;
    t.pressed = SEL_BG;
    t.text = INK;
    t.text_dim = INK_2;
    t.text_faint = DIM;
    t.icon = INK_2;
    t.accent = PINK;
    t.accent_soft = SEL_BG;
    t.accent_border = PINK;
    t.accent_text = PINK_TEXT;
    t.separator = LINE;
    t.primary_bg = PINK;
    t.primary_text = ON_PINK;
    t.tab_strip = NIGHT;
    t.row_selected = SEL_BG;
    t
}

/// Apply the host look when `on` (what `theme::apply` and `embedded::apply_embedded_visuals` call);
/// `on == false` leaves the context untouched.
pub fn apply_if(ctx: &egui::Context, on: bool) {
    if !on {
        return;
    }
    let t = applebox_tokens(crate::theme::Tokens::get(ctx));
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("photocraft-theme"), t));
    ctx.set_visuals(applebox_visuals());
    ctx.global_style_mut(|s| applebox_spacing(&mut s.spacing));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(c: egui::Color32) -> f64 {
        let f = |x: u8| {
            let v = x as f64 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }
    fn contrast(a: egui::Color32, b: egui::Color32) -> f64 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn visuals_use_cis_values() {
        let v = applebox_visuals();
        assert_eq!(v.selection.bg_fill, cis::PINK);
        assert_eq!(v.panel_fill, cis::PANEL);
        assert_eq!(v.extreme_bg_color, cis::NIGHT);
        assert_eq!(v.hyperlink_color, cis::PINK_TEXT);
        assert_eq!(v.override_text_color, Some(cis::INK));
        assert_eq!(v.widgets.active.bg_fill, cis::PINK);
        assert_eq!(v.widgets.active.fg_stroke.color, cis::ON_PINK);
        assert_eq!(v.widgets.hovered.bg_stroke.color, cis::PINK);
        assert_eq!(v.window_corner_radius, CornerRadius::same(cis::RADIUS_CARD));
    }

    #[test]
    fn text_pairs_reach_4_5_to_1() {
        use cis::*;
        for (fg, bg, what) in [
            (INK, PANEL, "primary text on panel"),
            (INK_2, PANEL, "secondary text on panel"),
            (DIM, NIGHT, "disabled text on night"),
            (PINK_TEXT, NIGHT, "links on night"),
            (ON_PINK, PINK, "text on the pink fill"),
            (INK, HOVER, "text on hover"),
        ] {
            let c = contrast(fg, bg);
            assert!(c >= 4.5, "{what}: {c:.2}");
        }
        assert!(contrast(PINK, PANEL) >= 3.0, "focus ring on panel");
        assert!(contrast(egui::Color32::WHITE, PINK) < 4.5, "white on pink is why ON_PINK is night");
    }

    #[test]
    fn tokens_use_cis_values_and_stay_readable() {
        use cis::*;
        let base = crate::theme::Tokens::for_kind(crate::theme::ThemeKind::Pro);
        let t = applebox_tokens(base);
        assert_eq!((t.card, t.chrome, t.accent, t.text, t.row_selected), (PANEL, NIGHT, PINK, INK, SEL_BG));
        assert_eq!((t.pro, t.bevel, t.radius), (base.pro, base.bevel, base.radius));
        assert_eq!((t.warning, t.danger), (base.warning, base.danger));
        for (fg, bg, what) in [
            (t.text, t.card, "text on card"),
            (t.text, t.chrome, "text on chrome"),
            (t.text, t.pressed, "text on pressed"),
            (t.text, t.row_selected, "text on selected row"),
            (t.text, t.field, "text in fields"),
            (t.text_dim, t.card, "dim text on card"),
            (t.accent_text, t.card, "accent text on card"),
            (t.accent_text, t.row_selected, "accent text on selected row"),
            (t.primary_text, t.primary_bg, "primary button text"),
        ] {
            let c = contrast(fg, bg);
            assert!(c >= 4.5, "{what}: {c:.2}");
        }
    }

    #[test]
    fn apply_if_switches_the_context() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx, crate::theme::ThemeKind::Pro);
        let before = ctx.global_style().visuals.selection.bg_fill;
        apply_if(&ctx, false);
        assert_eq!(ctx.global_style().visuals.selection.bg_fill, before);
        apply_if(&ctx, true);
        let s = ctx.global_style();
        assert_eq!(s.visuals.selection.bg_fill, cis::PINK);
        assert_eq!(s.visuals.panel_fill, cis::PANEL);
        assert_eq!(s.spacing.item_spacing, egui::vec2(8.0, 6.0));
        assert_ne!(before, cis::PINK, "the upstream theme does not use the host accent");
        let t = crate::theme::Tokens::get(&ctx);
        assert_eq!((t.card, t.accent), (cis::PANEL, cis::PINK), "panels paint from the tokens");
    }
}
