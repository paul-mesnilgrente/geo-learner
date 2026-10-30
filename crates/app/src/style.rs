//! Thème visuel : palette, polices et styles des widgets.

use iced::border::Radius;
use iced::font::Weight;
use iced::theme::Palette;
use iced::widget::{button, container, progress_bar, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, color};

pub const FONT: Font = Font::with_name("Inter");
pub const FONT_SEMIBOLD: Font = Font {
    weight: Weight::Semibold,
    ..FONT
};
pub const FONT_BOLD: Font = Font {
    weight: Weight::Bold,
    ..FONT
};

pub const BG: Color = color!(0x0d1117);
pub const SURFACE: Color = color!(0x161b22);
pub const SURFACE_2: Color = color!(0x1f2630);
pub const SURFACE_3: Color = color!(0x2a3340);
pub const BORDER: Color = color!(0x2d3643);
pub const TEXT: Color = color!(0xe6edf3);
pub const MUTED: Color = color!(0x8b98a5);
pub const ACCENT: Color = color!(0x4f8cff);
pub const ACCENT_SOFT: Color = color!(0x1c2b45);

pub const SUCCESS: Color = color!(0x2fbf71);
pub const WARNING: Color = color!(0xe8c547);
pub const ORANGE: Color = color!(0xf0883e);
pub const DANGER: Color = color!(0xf0524f);

// Carte
pub const OCEAN: Color = color!(0x0b1724);
pub const LAND: Color = color!(0x3a4757);
pub const LAND_OUTSIDE: Color = color!(0x222c38);
pub const TERRITORY: Color = color!(0x1d2631);
pub const COAST: Color = color!(0x0b1724);
pub const LABEL_DARK: Color = color!(0x0b1015);

pub fn theme() -> Theme {
    Theme::custom(
        "GeoLearner",
        Palette {
            background: BG,
            text: TEXT,
            primary: ACCENT,
            success: SUCCESS,
            warning: WARNING,
            danger: DANGER,
        },
    )
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

pub fn lighten(c: Color, t: f32) -> Color {
    mix(c, Color::WHITE, t)
}

// --- Conteneurs ---

pub fn page(_: &Theme) -> container::Style {
    container::Style {
        background: Some(BG.into()),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

pub fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 14.0.into(),
        },
        ..Default::default()
    }
}

pub fn floating(_: &Theme) -> container::Style {
    container::Style {
        background: Some(with_alpha(SURFACE, 0.94).into()),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 16.0.into(),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, 0.45),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 24.0,
        },
        ..Default::default()
    }
}

pub fn top_bar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn sidebar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        ..Default::default()
    }
}

pub fn map_frame(_: &Theme) -> container::Style {
    container::Style {
        background: Some(OCEAN.into()),
        ..Default::default()
    }
}

pub fn chip(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(mix(SURFACE, color, 0.16).into()),
        text_color: Some(lighten(color, 0.25)),
        border: Border {
            color: with_alpha(color, 0.35),
            width: 1.0,
            radius: 999.0.into(),
        },
        ..Default::default()
    }
}

pub fn dot(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

// --- Boutons ---

fn base_button(bg: Color, fg: Color, border: Color, radius: f32) -> button::Style {
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            color: border,
            width: if border.a > 0.0 { 1.0 } else { 0.0 },
            radius: Radius::from(radius),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => lighten(ACCENT, 0.12),
        button::Status::Pressed => mix(ACCENT, Color::BLACK, 0.1),
        button::Status::Disabled => SURFACE_3,
        button::Status::Active => ACCENT,
    };
    let fg = if status == button::Status::Disabled {
        MUTED
    } else {
        Color::WHITE
    };
    base_button(bg, fg, Color::TRANSPARENT, 10.0)
}

pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => SURFACE_3,
        button::Status::Pressed => SURFACE_2,
        _ => SURFACE_2,
    };
    let fg = if status == button::Status::Disabled {
        MUTED
    } else {
        TEXT
    };
    base_button(bg, fg, BORDER, 10.0)
}

pub fn ghost(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => SURFACE_2,
        _ => Color::TRANSPARENT,
    };
    base_button(bg, TEXT, Color::TRANSPARENT, 10.0)
}

pub fn danger_soft(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => with_alpha(DANGER, 0.22),
        _ => with_alpha(DANGER, 0.12),
    };
    base_button(bg, lighten(DANGER, 0.2), with_alpha(DANGER, 0.3), 10.0)
}

/// Bouton circulaire posé sur la carte.
pub fn map_control(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => SURFACE_3,
        _ => with_alpha(SURFACE, 0.92),
    };
    base_button(bg, TEXT, BORDER, 999.0)
}

pub fn mic(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, fg) = match (active, hovered) {
            (true, false) => (DANGER, Color::WHITE),
            (true, true) => (lighten(DANGER, 0.12), Color::WHITE),
            (false, false) => (SURFACE_2, TEXT),
            (false, true) => (SURFACE_3, TEXT),
        };
        base_button(bg, fg, Color::TRANSPARENT, 999.0)
    }
}

/// Carte sélectionnable (choix du mode).
pub fn option_card(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = match (selected, hovered) {
            (true, _) => ACCENT_SOFT,
            (false, true) => SURFACE_2,
            (false, false) => SURFACE,
        };
        let border = if selected { ACCENT } else { BORDER };
        let mut style = base_button(bg, TEXT, border, 14.0);
        style.border.width = if selected { 2.0 } else { 1.0 };
        style
    }
}

/// Pastille sélectionnable (choix de la zone).
pub fn pill(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (bg, fg, border) = match (selected, hovered) {
            (true, _) => (ACCENT, Color::WHITE, ACCENT),
            (false, true) => (SURFACE_3, TEXT, BORDER),
            (false, false) => (SURFACE_2, TEXT, BORDER),
        };
        base_button(bg, fg, border, 999.0)
    }
}

// --- Champs ---

pub fn answer_input(
    flash: Option<Color>,
) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, status| {
        let focused = matches!(status, text_input::Status::Focused { .. });
        let border = flash.unwrap_or(if focused { ACCENT } else { BORDER });
        text_input::Style {
            background: SURFACE_2.into(),
            border: Border {
                color: border,
                width: if focused || flash.is_some() { 2.0 } else { 1.0 },
                radius: 12.0.into(),
            },
            icon: MUTED,
            placeholder: MUTED,
            value: TEXT,
            selection: with_alpha(ACCENT, 0.4),
        }
    }
}

pub fn progress(color: Color) -> impl Fn(&Theme) -> progress_bar::Style {
    move |_| progress_bar::Style {
        background: SURFACE_3.into(),
        bar: color.into(),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
    }
}
