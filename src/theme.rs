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
};

pub fn by_name(name: &str) -> &'static Theme {
    match name {
        "light" => &LIGHT,
        _ => &DARK,
    }
}
