//! Shared egui helpers for the light, rounded look.

use bevy_egui::egui;

pub const TEXT: egui::Color32 = egui::Color32::from_rgb(35, 38, 48);
pub const CARD: egui::Color32 = egui::Color32::from_rgba_premultiplied(250, 250, 252, 240);

pub fn card<R>(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(14.0)
        .inner_margin(12.0)
        .shadow(egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: egui::Color32::from_black_alpha(45) })
        .show(ui, f)
        .inner
}

/// Text with a soft dark shadow so it stays readable over the sky and terrain.
pub fn shadow_label(ui: &mut egui::Ui, text: &str, size: f32, color: egui::Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(size), color);
    let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    let off = (size * 0.04).max(1.0);
    for d in [egui::vec2(off, off), egui::vec2(0.0, off * 1.6)] {
        ui.painter().galley(rect.min + d, galley.clone(), egui::Color32::from_black_alpha(90));
    }
    ui.painter().galley(rect.min, galley, color);
}

pub fn pill(ui: &mut egui::Ui, text: &str, size: f32) {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(20.0)
        .inner_margin(egui::Margin::symmetric(14, 6))
        .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: egui::Color32::from_black_alpha(40) })
        .show(ui, |ui| {
            // Extend: anchored areas remember their first width, which wrapped "$ 150".
            ui.add(egui::Label::new(egui::RichText::new(text).size(size).strong().color(TEXT)).wrap_mode(egui::TextWrapMode::Extend));
        });
}

pub fn amber_pill(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(255, 214, 120))
        .corner_radius(20.0)
        .inner_margin(egui::Margin::symmetric(14, 6))
        .show(ui, |ui| {
            // Extend: an anchored area remembers its first-frame width and would wrap this
            // one character per line.
            ui.add(egui::Label::new(egui::RichText::new(text).size(13.0).strong().color(TEXT)).wrap_mode(egui::TextWrapMode::Extend));
        });
}

pub fn pill_btn(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_owned()).size(15.0).strong().color(TEXT))
        .fill(egui::Color32::from_rgb(236, 237, 242))
        .corner_radius(18.0)
        .min_size(egui::vec2(0.0, 30.0))
}

pub fn pill_button(ui: &mut egui::Ui, text: &str, active: bool) -> egui::Response {
    let (fill, color) = if active { (egui::Color32::from_rgb(70, 130, 230), egui::Color32::WHITE) } else { (egui::Color32::from_rgb(236, 237, 242), TEXT) };
    ui.add(
        egui::Button::new(egui::RichText::new(text).size(14.0).strong().color(color))
            .fill(fill)
            .corner_radius(16.0)
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

/// Full-width row button like the reference airport card: a glyph on the left, then the
/// label, both left-aligned. Disabled rows are drawn but do not react.
pub fn icon_row_btn(ui: &mut egui::Ui, icon: &str, text: &str, width: f32, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 34.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let fill = if !enabled {
        egui::Color32::from_rgb(241, 241, 245)
    } else if resp.hovered() {
        egui::Color32::from_rgb(222, 224, 232)
    } else {
        egui::Color32::from_rgb(236, 237, 242)
    };
    let col = if enabled { TEXT } else { egui::Color32::from_gray(150) };
    let p = ui.painter();
    p.rect_filled(rect, 17.0, fill);
    p.text(egui::pos2(rect.min.x + 14.0, rect.center().y), egui::Align2::LEFT_CENTER, icon, egui::FontId::proportional(17.0), col);
    p.text(egui::pos2(rect.min.x + 46.0, rect.center().y), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(15.0), col);
    resp
}

/// A toggle drawn as a pill; `on` renders it filled.
pub fn toggle_button(ui: &mut egui::Ui, text: &str, on: bool) -> egui::Response {
    let (fill, color) = if on { (egui::Color32::from_rgb(70, 130, 230), egui::Color32::WHITE) } else { (CARD, TEXT) };
    ui.add(
        egui::Button::new(egui::RichText::new(text).size(15.0).strong().color(color))
            .fill(fill)
            .corner_radius(20.0)
            .min_size(egui::vec2(0.0, 36.0)),
    )
}

/// Odometer-style digit boxes: leading zeros in light grey, the number in dark.
pub fn odometer(ui: &mut egui::Ui, label: &str, value: u32, digits: usize) {
    ui.vertical(|ui| {
        shadow_label(ui, label, 15.0, egui::Color32::WHITE);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let s = format!("{:0width$}", value.min(10u32.pow(digits as u32) - 1), width = digits);
            let mut leading = true;
            for (k, ch) in s.chars().enumerate() {
                if ch != '0' || k == s.len() - 1 {
                    leading = false;
                }
                let col = if leading { egui::Color32::from_gray(178) } else { TEXT };
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(232, 233, 237))
                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(150)))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(ch).size(18.0).monospace().strong().color(col));
                    });
            }
        });
    });
}

/// A labelled horizontal bar (fuel, electricity): label above, dark-rimmed track, colour fill.
pub fn bar(ui: &mut egui::Ui, label: &str, frac: f32, color: egui::Color32) {
    shadow_label(ui, label, 15.0, egui::Color32::WHITE);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(250.0, 24.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 6.0, egui::Color32::from_rgb(58, 60, 66));
    let inner = rect.shrink(2.5);
    p.rect_filled(inner, 4.5, egui::Color32::from_rgb(214, 215, 220));
    let mut fill = inner;
    fill.max.x = inner.min.x + inner.width() * frac.clamp(0.0, 1.0);
    if fill.width() > 1.0 {
        p.rect_filled(fill, 4.5, color);
    }
}

/// Round white button with a painted glyph (the reference game's top-right corner).
pub fn round_icon_btn(ui: &mut egui::Ui, glyph: impl FnOnce(&egui::Painter, egui::Pos2, f32, egui::Color32), tip: &str) -> egui::Response {
    let r = 19.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(r * 2.0, r * 2.0), egui::Sense::click());
    let fill = if resp.hovered() { egui::Color32::from_rgb(228, 230, 236) } else { egui::Color32::WHITE };
    let p = ui.painter();
    p.circle_filled(rect.center() + egui::vec2(0.0, 1.5), r, egui::Color32::from_black_alpha(40));
    p.circle_filled(rect.center(), r, fill);
    glyph(p, rect.center(), r * 0.55, TEXT);
    resp.on_hover_text(tip)
}

/// Simple pictograms for `round_icon_btn`.
pub fn glyph_house(p: &egui::Painter, c: egui::Pos2, s: f32, col: egui::Color32) {
    let pt = |x: f32, y: f32| egui::pos2(c.x + x * s, c.y + y * s);
    p.add(egui::Shape::convex_polygon(vec![pt(-1.1, 0.0), pt(0.0, -1.0), pt(1.1, 0.0)], col, egui::Stroke::NONE));
    p.rect_filled(egui::Rect::from_min_max(pt(-0.8, 0.0), pt(0.8, 1.0)), 0.0, col);
    p.rect_filled(egui::Rect::from_min_max(pt(-0.2, 0.4), pt(0.2, 1.0)), 0.0, egui::Color32::WHITE);
}

pub fn glyph_box(p: &egui::Painter, c: egui::Pos2, s: f32, col: egui::Color32) {
    let pt = |x: f32, y: f32| egui::pos2(c.x + x * s, c.y + y * s);
    let st = egui::Stroke::new((0.18 * s).max(1.5), col);
    // Isometric crate: top face, two sides.
    p.add(egui::Shape::closed_line(vec![pt(0.0, -1.0), pt(1.0, -0.5), pt(0.0, 0.0), pt(-1.0, -0.5)], st));
    p.add(egui::Shape::closed_line(vec![pt(-1.0, -0.5), pt(0.0, 0.0), pt(0.0, 1.0), pt(-1.0, 0.5)], st));
    p.add(egui::Shape::closed_line(vec![pt(1.0, -0.5), pt(0.0, 0.0), pt(0.0, 1.0), pt(1.0, 0.5)], st));
}

pub fn glyph_pin(p: &egui::Painter, c: egui::Pos2, s: f32, col: egui::Color32) {
    let pt = |x: f32, y: f32| egui::pos2(c.x + x * s, c.y + y * s);
    p.circle_filled(pt(0.0, -0.35), 0.62 * s, col);
    p.add(egui::Shape::convex_polygon(vec![pt(-0.5, -0.05), pt(0.5, -0.05), pt(0.0, 1.0)], col, egui::Stroke::NONE));
    p.circle_filled(pt(0.0, -0.35), 0.26 * s, egui::Color32::WHITE);
}
