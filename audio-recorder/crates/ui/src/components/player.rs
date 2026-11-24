use audio_recorder_schema::ipc::{NoArgs, PlaybackStateDto, SeekArgs};
use leptos::prelude::*;

use crate::{
    fmt,
    store::{Store, run},
};

#[component]
pub fn PlayerBar(store: Store) -> impl IntoView {
    let playback = store.playback;
    move || {
        let state = playback.get();
        state.playing.then(|| {
            let label = match &state.track {
                Some(track) => format!("{} · {track}", state.session.clone().unwrap_or_default()),
                None => format!("{} · mix", state.session.clone().unwrap_or_default()),
            };
            view! {
                <footer class="player" role="region" aria-label="Playback">
                    <span class="player-title">{label}</span>
                    <button
                        class="btn"
                        on:click=move |_| {
                            let command = if playback.get_untracked().paused { "resume" } else { "pause" };
                            run(store, command, NoArgs::default(), move |s: PlaybackStateDto| playback.set(s));
                        }
                    >
                        {move || if playback.get().paused { "Resume" } else { "Pause" }}
                    </button>
                    <button
                        class="btn"
                        on:click=move |_| run(store, "stop_playback", NoArgs::default(), move |s: PlaybackStateDto| playback.set(s))
                    >
                        "Stop"
                    </button>
                    <input
                        type="range"
                        class="seek"
                        aria-label="Seek"
                        min="0"
                        max=move || playback.get().duration_ms.to_string()
                        prop:value=move || playback.get().position_ms.to_string()
                        on:change=move |ev| {
                            let ms = event_target_value(&ev).parse::<u64>().unwrap_or(0);
                            run(store, "seek", SeekArgs { position_ms: ms }, move |s: PlaybackStateDto| playback.set(s));
                        }
                    />
                    <span class="clock">
                        {move || format!("{} / {}", fmt::clock(playback.get().position_ms), fmt::clock(playback.get().duration_ms))}
                    </span>
                </footer>
            }
        })
    }
}
