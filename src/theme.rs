use ratatui::style::Color;

pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub surface: Color,
    pub muted: Color,
    pub success: Color,
    pub warn: Color,
    pub error: Color,
    pub is_dark: bool,
}

pub const DARK: Theme = Theme {
    bg: Color::Rgb(20, 22, 26),
    fg: Color::Rgb(220, 223, 228),
    accent: Color::Rgb(137, 180, 250),
    surface: Color::Rgb(68, 71, 77),
    muted: Color::Rgb(120, 124, 132),
    success: Color::Rgb(166, 227, 161),
    warn: Color::Rgb(249, 226, 175),
    error: Color::Rgb(243, 139, 168),
    is_dark: true,
};

pub const LIGHT: Theme = Theme {
    bg: Color::Rgb(250, 250, 250),
    fg: Color::Rgb(30, 30, 30),
    accent: Color::Rgb(30, 102, 245),
    surface: Color::Rgb(200, 200, 210),
    muted: Color::Rgb(130, 130, 140),
    success: Color::Rgb(64, 160, 43),
    warn: Color::Rgb(223, 142, 29),
    error: Color::Rgb(210, 15, 57),
    is_dark: false,
};

pub fn by_name(name: &str) -> &'static Theme {
    match name {
        "light" => &LIGHT,
        _ => &DARK,
    }
}

/// Stable per-string color via FNV-1a → HSL → RGB.
/// Saturation/lightness chosen for legibility on the active theme background.
pub fn hashed_color(s: &str, theme: &Theme) -> Color {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let hue = (hash % 360) as f32;
    let (sat, lum) = if theme.is_dark { (0.55, 0.72) } else { (0.70, 0.38) };
    let (r, g, b) = hsl_to_rgb(hue, sat, lum);
    Color::Rgb(r, g, b)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_ = h / 60.0;
    let x = c * (1.0 - (h_ % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_ as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}
