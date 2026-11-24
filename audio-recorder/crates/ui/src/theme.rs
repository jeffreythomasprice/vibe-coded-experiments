use audio_recorder_schema::{
    from_json_str,
    theme::{Palette, Theme, ThemeScheme},
};

pub const SYSTEM_ID: &str = "system";
const STYLE_ID: &str = "app-theme";

const SOURCES: [&str; 8] = [
    include_str!("../themes/light.theme.json"),
    include_str!("../themes/dark.theme.json"),
    include_str!("../themes/gruvbox-light-hard.theme.json"),
    include_str!("../themes/gruvbox-light.theme.json"),
    include_str!("../themes/gruvbox-light-soft.theme.json"),
    include_str!("../themes/gruvbox-dark-hard.theme.json"),
    include_str!("../themes/gruvbox-dark.theme.json"),
    include_str!("../themes/gruvbox-dark-soft.theme.json"),
];

pub fn all() -> Vec<Theme> {
    SOURCES
        .iter()
        .map(|text| from_json_str(text).expect("bundled theme matches the theme schema"))
        .collect()
}

fn declarations(p: &Palette) -> String {
    [
        ("bg", &p.bg),
        ("panel", &p.panel),
        ("panel-raised", &p.panel_raised),
        ("text", &p.text),
        ("muted", &p.muted),
        ("border", &p.border),
        ("accent", &p.accent),
        ("accent-text", &p.accent_text),
        ("danger", &p.danger),
        ("danger-text", &p.danger_text),
        ("meter-bg", &p.meter_bg),
        ("meter-fill", &p.meter_fill),
        ("overlay", &p.overlay),
    ]
    .iter()
    .map(|(name, value)| format!("--{name}:{value};"))
    .collect()
}

fn scheme_name(theme: &Theme) -> &'static str {
    match theme.scheme {
        ThemeScheme::Light => "light",
        ThemeScheme::Dark => "dark",
    }
}

pub fn css(themes: &[Theme]) -> String {
    let find = |scheme: ThemeScheme, id: &str| themes.iter().find(|t| t.scheme == scheme && t.id == id);
    let mut out = String::new();
    if let (Some(light), Some(dark)) = (find(ThemeScheme::Light, "light"), find(ThemeScheme::Dark, "dark")) {
        out.push_str(&format!(
            ":root{{color-scheme:light;{}}}@media (prefers-color-scheme: dark){{:root{{color-scheme:dark;{}}}}}",
            declarations(&light.palette),
            declarations(&dark.palette)
        ));
    }
    for theme in themes {
        out.push_str(&format!(
            ":root[data-theme=\"{}\"]{{color-scheme:{};{}}}",
            theme.id,
            scheme_name(theme),
            declarations(&theme.palette)
        ));
    }
    out
}

fn document() -> Option<web_sys::Document> {
    web_sys::window().and_then(|w| w.document())
}

pub fn install(themes: &[Theme]) {
    let Some(document) = document() else { return };
    let style = match document.get_element_by_id(STYLE_ID) {
        Some(existing) => existing,
        None => {
            let Ok(created) = document.create_element("style") else { return };
            created.set_id(STYLE_ID);
            let Some(head) = document.head() else { return };
            let _ = head.append_child(&created);
            created
        }
    };
    style.set_text_content(Some(&css(themes)));
}

pub fn select(themes: &[Theme], id: &str) {
    let Some(root) = document().and_then(|d| d.document_element()) else { return };
    if themes.iter().any(|t| t.id == id) {
        let _ = root.set_attribute("data-theme", id);
    } else {
        let _ = root.remove_attribute("data-theme");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_themes_are_unique_and_include_system_sources() {
        let themes = all();
        let mut ids: Vec<_> = themes.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), themes.len());
        assert!(ids.contains(&"light") && ids.contains(&"dark"));
        assert!(!ids.contains(&SYSTEM_ID));
    }

    #[test]
    fn css_emits_every_token_per_theme_and_system_default() {
        let themes = all();
        let css = css(&themes);
        for token in [
            "bg", "panel", "panel-raised", "text", "muted", "border", "accent", "accent-text", "danger",
            "danger-text", "meter-bg", "meter-fill", "overlay",
        ] {
            assert_eq!(css.matches(&format!("--{token}:")).count(), themes.len() + 2, "{token}");
        }
        assert!(css.starts_with(":root{color-scheme:light;"));
        assert!(css.contains("@media (prefers-color-scheme: dark)"));
        assert!(css.contains(":root[data-theme=\"gruvbox-dark\"]{color-scheme:dark;"));
    }
}
