//! Inter, the typeface the shipped SecretFolder uses: Fontsource's `@fontsource/inter` 5.3.0 as the
//! React app imported it (`src/main.tsx`: weights 400, 500, 600, 700 and 800, normal style),
//! every script subset the browser loaded from it (Latin, Latin Extended, Cyrillic, Cyrillic
//! Extended, Greek, Greek Extended and Vietnamese), as the same WOFF2 files. SIL Open Font
//! License 1.1 (`assets/fonts/inter/OFL.txt`).

use remedy_ui::AppConfig;
use remedy_ui::text::FontFile;

/// The family the files hold.
pub const FAMILY: &str = "Inter";

/// Every file, Latin first: each weight's subsets join as one family, and each character is
/// drawn from the subset that covers it, as CSS's `unicode-range` does.
const FILES: [&[u8]; 35] = [
    include_bytes!("../assets/fonts/inter/inter-latin-400-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-500-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-600-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-700-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-800-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-ext-400-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-ext-500-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-ext-600-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-ext-700-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-latin-ext-800-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-400-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-500-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-600-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-700-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-800-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-ext-400-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-ext-500-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-ext-600-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-ext-700-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-cyrillic-ext-800-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-400-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-500-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-600-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-700-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-800-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-ext-400-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-ext-500-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-ext-600-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-ext-700-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-greek-ext-800-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-vietnamese-400-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-vietnamese-500-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-vietnamese-600-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-vietnamese-700-normal.woff2"),
    include_bytes!("../assets/fonts/inter/inter-vietnamese-800-normal.woff2"),
];

/// `config` with Inter on every window, first in the theme's families (the Remedy families stay
/// behind it, as `system-ui` and `sans-serif` were behind it in the shipped CSS).
pub fn with_inter(mut config: AppConfig) -> AppConfig {
    for data in FILES {
        // Each file is checked by `every_inter_file_loads`; one that would not read is left
        // out rather than stopping the app.
        if let Ok(file) = FontFile::new(data) {
            config = config.font(file);
        }
    }
    if !config.theme.font_families.iter().any(|f| f == FAMILY) {
        config.theme.font_families.insert(0, FAMILY.to_owned());
    }
    config
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_inter_file_loads() {
        for data in super::FILES {
            let file = remedy_ui::text::FontFile::new(data).expect("a WOFF2 font");
            assert_eq!(file.families(), [super::FAMILY]);
        }
        let config = super::with_inter(remedy_ui::AppConfig::new("Inter"));
        assert_eq!(config.theme.font_families.first().map(String::as_str), Some(super::FAMILY));
    }
}
