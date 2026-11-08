//! Appearance of the mobile shell and the shared widgets it hosts.

use egui::{Color32, Context, FontId, Id, Style, TextStyle, Vec2, vec2};

use crate::theme::{self, ThemeKind, Tokens};

use super::shell::metrics;

const GRAPHITE: Color32 = Color32::from_rgb(0x14, 0x16, 0x19);
const PANEL: Color32 = Color32::from_rgb(0x20, 0x23, 0x28);
const SEPARATOR: Color32 = Color32::from_rgb(0x30, 0x34, 0x3A);
const MINT: Color32 = Color32::from_rgb(0x92, 0xE3, 0xBF);
const MINT_TINT: Color32 = Color32::from_rgb(0x27, 0x42, 0x39);

pub const TOUCH_INTERACT_SIZE: Vec2 = Vec2::splat(metrics::MIN_TOUCH_TARGET);
pub const TOUCH_ITEM_SPACING: Vec2 = vec2(10.0, 10.0);
pub const TOUCH_BUTTON_PADDING: Vec2 = vec2(14.0, 10.0);

fn enabled_id() -> Id {
    Id::unique("photocraft-mobile-theme")
}

pub fn is_enabled(ctx: &Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(enabled_id()).unwrap_or(false))
}

/// Reapply the selected appearance when switching shells, including through `ui.set`.
pub fn sync(ctx: &Context, enabled: bool, kind: ThemeKind) -> bool {
    if is_enabled(ctx) == enabled {
        return false;
    }
    ctx.data_mut(|data| data.insert_temp(enabled_id(), enabled));

    if enabled {
        theme::apply(ctx, kind);
        let base = Tokens::get(ctx);
        let mobile_tokens = tokens(base);
        theme::apply_tokens(ctx, mobile_tokens);
        ctx.global_style_mut(|style| {
            apply_style(style);
        });
    } else {
        theme::apply(ctx, kind);
    }
    true
}

/// Give the default mobile appearance the reference palette. Preserve alternate palettes and
/// the base theme's layout flags so shared panels keep their tab ordering and behavior.
pub fn tokens(base: Tokens) -> Tokens {
    if base.kind != ThemeKind::ProMedium {
        return base;
    }

    Tokens {
        chrome: GRAPHITE,
        canvas: GRAPHITE,
        canvas_dot: GRAPHITE,
        dock: PANEL,
        card: PANEL,
        card_border: SEPARATOR,
        field: Color32::from_rgb(0x29, 0x2D, 0x33),
        field_border: Color32::from_rgb(0x51, 0x58, 0x62),
        hover: Color32::from_rgb(0x32, 0x37, 0x3F),
        pressed: Color32::from_rgb(0x3A, 0x42, 0x4B),
        text: Color32::from_rgb(0xF3, 0xF4, 0xF6),
        text_dim: Color32::from_rgb(0xB8, 0xBE, 0xC7),
        text_faint: Color32::from_rgb(0x94, 0x9C, 0xA8),
        icon: Color32::from_rgb(0xE7, 0xE9, 0xEF),
        accent: MINT,
        accent_soft: MINT_TINT,
        accent_border: MINT,
        accent_text: MINT,
        separator: SEPARATOR,
        primary_bg: MINT,
        primary_text: GRAPHITE,
        danger: Color32::from_rgb(0xFF, 0x8D, 0x8B),
        warning: Color32::from_rgb(0xF3, 0xCB, 0x86),
        radius_sm: 8.0,
        radius: 12.0,
        radius_lg: 12.0,
        tab_strip: PANEL,
        row_selected: MINT_TINT,
        histogram_bg: GRAPHITE,
        scrim: Color32::from_black_alpha(180),
        ..base
    }
}

/// Set the default text scale for mobile. Individual controls can choose a tighter label size.
pub fn apply_style(style: &mut Style) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Heading, theme::semibold(18.0)),
        (TextStyle::Monospace, theme::mono(13.0)),
    ]
    .into();
}

/// Size standard egui widgets in a panel with room for touch controls.
pub fn apply_touch_style(style: &mut Style) {
    let spacing = &mut style.spacing;
    spacing.interact_size.x = spacing.interact_size.x.max(TOUCH_INTERACT_SIZE.x);
    spacing.interact_size.y = spacing.interact_size.y.max(TOUCH_INTERACT_SIZE.y);
    spacing.item_spacing = TOUCH_ITEM_SPACING;
    spacing.button_padding = TOUCH_BUTTON_PADDING;
}
