use chess_shared::{BoardColors, PieceStyle, PieceStyleMode, ThemeName};
use leptos::prelude::*;

use crate::theme::{self, ThemeState};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let theme_state = expect_context::<ThemeState>();
    let toast = expect_context::<crate::ToastState>();

    // Local signals for editing
    let (preset, set_preset) = signal(String::new());
    let (light_sq, set_light_sq) = signal(String::new());
    let (dark_sq, set_dark_sq) = signal(String::new());
    let (piece_mode, set_piece_mode) = signal(String::new());
    let (white_fill, set_white_fill) = signal(String::new());
    let (black_fill, set_black_fill) = signal(String::new());
    let (white_text, set_white_text) = signal(String::new());
    let (black_text, set_black_text) = signal(String::new());
    let (svg_base_path, set_svg_base_path) = signal(String::new());
    let (white_tint, set_white_tint) = signal(String::new());
    let (black_tint, set_black_tint) = signal(String::new());
    let (saving, set_saving) = signal(false);

    // Initialize from current theme
    let init_from_theme = move |t: &chess_shared::Theme| {
        set_light_sq.set(t.board.light_square.clone());
        set_dark_sq.set(t.board.dark_square.clone());
        match t.pieces.mode {
            PieceStyleMode::Circle => set_piece_mode.set("circle".into()),
            PieceStyleMode::Svg => set_piece_mode.set("svg".into()),
        }
        set_white_fill.set(t.pieces.white_fill.clone().unwrap_or_default());
        set_black_fill.set(t.pieces.black_fill.clone().unwrap_or_default());
        set_white_text.set(t.pieces.white_text.clone().unwrap_or_default());
        set_black_text.set(t.pieces.black_text.clone().unwrap_or_default());
        set_svg_base_path.set(t.pieces.svg_base_path.clone().unwrap_or_default());
        set_white_tint.set(t.pieces.white_tint.clone().unwrap_or_default());
        set_black_tint.set(t.pieces.black_tint.clone().unwrap_or_default());
        set_preset.set(t.name.to_string());
    };

    // Init on mount
    init_from_theme(&theme_state.theme.get_untracked());

    let on_preset_change = move |ev: leptos::ev::Event| {
        let val = event_target_value(&ev);
        set_preset.set(val.clone());
        match val.as_str() {
            "Classic" => init_from_theme(&theme::default_classic_theme()),
            "Standard" => init_from_theme(&theme::default_standard_theme()),
            _ => {}
        }
    };

    let build_theme = move || -> chess_shared::Theme {
        let mode = if piece_mode.get() == "svg" {
            PieceStyleMode::Svg
        } else {
            PieceStyleMode::Circle
        };
        chess_shared::Theme {
            name: ThemeName::try_from(preset.get().as_str())
                .unwrap_or_else(|_| ThemeName::try_from("Custom").unwrap()),
            board: BoardColors {
                light_square: light_sq.get(),
                dark_square: dark_sq.get(),
            },
            pieces: PieceStyle {
                mode,
                white_fill: Some(white_fill.get()).filter(|s| !s.is_empty()),
                black_fill: Some(black_fill.get()).filter(|s| !s.is_empty()),
                white_text: Some(white_text.get()).filter(|s| !s.is_empty()),
                black_text: Some(black_text.get()).filter(|s| !s.is_empty()),
                svg_base_path: Some(svg_base_path.get()).filter(|s| !s.is_empty()),
                white_tint: Some(white_tint.get()).filter(|s| !s.is_empty()),
                black_tint: Some(black_tint.get()).filter(|s| !s.is_empty()),
            },
        }
    };

    let on_save = move |ev: leptos::ev::MouseEvent| {
        ev.prevent_default();
        let theme = build_theme();
        set_saving.set(true);
        let theme_json = serde_json::to_string(&theme).unwrap();
        leptos::task::spawn_local(async move {
            let result = crate::api::put("/api/me/theme")
                .body(&theme_json)
                .unwrap()
                .send()
                .await;
            set_saving.set(false);
            match result {
                Ok(resp) if resp.ok() => {
                    let saved_theme: chess_shared::Theme = serde_json::from_str(&theme_json).unwrap();
                    theme_state.set(saved_theme);
                }
                _ => {
                    toast.message.set(Some("Failed to save theme.".into()));
                }
            }
        });
    };

    let on_reset = move |ev: leptos::ev::MouseEvent| {
        ev.prevent_default();
        init_from_theme(&theme::default_classic_theme());
    };

    view! {
        <h1>"Settings"</h1>
        <div style="max-width:600px;">
            <h2>"Theme"</h2>

            // Preset selector
            <div style="margin-bottom:1em;">
                <label for="preset"><strong>"Preset: "</strong></label>
                <select id="preset" on:change=on_preset_change prop:value=preset>
                    <option value="Classic">"Classic"</option>
                    <option value="Standard">"Standard"</option>
                    <option value="Custom">"Custom"</option>
                </select>
            </div>

            // Board colors
            <fieldset style="margin-bottom:1em; padding:0.5em;">
                <legend><strong>"Board Colors"</strong></legend>
                <div style="display:flex; gap:1em; align-items:center; margin:0.5em 0;">
                    <label>"Light square: "</label>
                    <input type="color"
                        prop:value=light_sq
                        on:input=move |ev| {
                            set_light_sq.set(event_target_value(&ev));
                            set_preset.set("Custom".into());
                        }
                    />
                    <input type="text" prop:value=light_sq style="width:80px;"
                        on:input=move |ev| {
                            set_light_sq.set(event_target_value(&ev));
                            set_preset.set("Custom".into());
                        }
                    />
                </div>
                <div style="display:flex; gap:1em; align-items:center; margin:0.5em 0;">
                    <label>"Dark square: "</label>
                    <input type="color"
                        prop:value=dark_sq
                        on:input=move |ev| {
                            set_dark_sq.set(event_target_value(&ev));
                            set_preset.set("Custom".into());
                        }
                    />
                    <input type="text" prop:value=dark_sq style="width:80px;"
                        on:input=move |ev| {
                            set_dark_sq.set(event_target_value(&ev));
                            set_preset.set("Custom".into());
                        }
                    />
                </div>
            </fieldset>

            // Piece mode
            <fieldset style="margin-bottom:1em; padding:0.5em;">
                <legend><strong>"Piece Style"</strong></legend>
                <div style="margin:0.5em 0;">
                    <label>
                        <input type="radio" name="piece_mode" value="circle"
                            prop:checked=move || piece_mode.get() == "circle"
                            on:change=move |_| {
                                set_piece_mode.set("circle".into());
                                set_preset.set("Custom".into());
                            }
                        />
                        " Circle (letters)"
                    </label>
                    " "
                    <label>
                        <input type="radio" name="piece_mode" value="svg"
                            prop:checked=move || piece_mode.get() == "svg"
                            on:change=move |_| {
                                set_piece_mode.set("svg".into());
                                set_preset.set("Custom".into());
                            }
                        />
                        " SVG Graphics"
                    </label>
                </div>

                // Circle options
                {move || {
                    if piece_mode.get() == "circle" {
                        Some(view! {
                            <div style="margin-top:0.5em;">
                                <div style="display:flex; gap:1em; align-items:center; margin:0.25em 0;">
                                    <label style="width:100px;">"White fill:"</label>
                                    <input type="color" prop:value=white_fill
                                        on:input=move |ev| { set_white_fill.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                    <input type="text" prop:value=white_fill style="width:80px;"
                                        on:input=move |ev| { set_white_fill.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                </div>
                                <div style="display:flex; gap:1em; align-items:center; margin:0.25em 0;">
                                    <label style="width:100px;">"White text:"</label>
                                    <input type="color" prop:value=white_text
                                        on:input=move |ev| { set_white_text.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                    <input type="text" prop:value=white_text style="width:80px;"
                                        on:input=move |ev| { set_white_text.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                </div>
                                <div style="display:flex; gap:1em; align-items:center; margin:0.25em 0;">
                                    <label style="width:100px;">"Black fill:"</label>
                                    <input type="color" prop:value=black_fill
                                        on:input=move |ev| { set_black_fill.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                    <input type="text" prop:value=black_fill style="width:80px;"
                                        on:input=move |ev| { set_black_fill.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                </div>
                                <div style="display:flex; gap:1em; align-items:center; margin:0.25em 0;">
                                    <label style="width:100px;">"Black text:"</label>
                                    <input type="color" prop:value=black_text
                                        on:input=move |ev| { set_black_text.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                    <input type="text" prop:value=black_text style="width:80px;"
                                        on:input=move |ev| { set_black_text.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                </div>
                            </div>
                        })
                    } else {
                        None
                    }
                }}

                // SVG options
                {move || {
                    if piece_mode.get() == "svg" {
                        Some(view! {
                            <div style="margin-top:0.5em;">
                                <div style="display:flex; gap:1em; align-items:center; margin:0.25em 0;">
                                    <label style="width:100px;">"SVG path:"</label>
                                    <input type="text" prop:value=svg_base_path style="width:200px;"
                                        on:input=move |ev| { set_svg_base_path.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                </div>
                                <div style="display:flex; gap:1em; align-items:center; margin:0.25em 0;">
                                    <label style="width:100px;">"White tint:"</label>
                                    <input type="color" prop:value=white_tint
                                        on:input=move |ev| { set_white_tint.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                    <input type="text" prop:value=white_tint style="width:80px;"
                                        on:input=move |ev| { set_white_tint.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                    <span style="color:#888; font-size:0.9em;">"(leave empty for no tint)"</span>
                                </div>
                                <div style="display:flex; gap:1em; align-items:center; margin:0.25em 0;">
                                    <label style="width:100px;">"Black tint:"</label>
                                    <input type="color" prop:value=black_tint
                                        on:input=move |ev| { set_black_tint.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                    <input type="text" prop:value=black_tint style="width:80px;"
                                        on:input=move |ev| { set_black_tint.set(event_target_value(&ev)); set_preset.set("Custom".into()); }
                                    />
                                    <span style="color:#888; font-size:0.9em;">"(leave empty for no tint)"</span>
                                </div>
                            </div>
                        })
                    } else {
                        None
                    }
                }}
            </fieldset>

            // Actions
            <div style="display:flex; gap:1em;">
                <button on:click=on_save disabled=saving>
                    {move || if saving.get() { "Saving..." } else { "Save Theme" }}
                </button>
                <button on:click=on_reset style="background:#666; color:white;">
                    "Reset to Classic"
                </button>
            </div>
        </div>
    }
}
