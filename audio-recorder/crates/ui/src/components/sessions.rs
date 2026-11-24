use audio_recorder_schema::ipc::{
    ChannelGainDto, Empty, EnqueueCompressionArgs, JobDto, JobDtoKind, MixDto, PlayArgs, PlaybackStateDto, SegmentDtoStatus, SessionDetailDto, SessionNameArgs,
    SessionSummaryDto, SetMixArgs,
};
use leptos::prelude::*;

use crate::{
    components::{summary::SummarySection, transcript::TranscriptSection},
    fmt,
    store::{Confirm, ConfirmAction, PLAYBACK_LOCKED_TIP, Store, job_active, refresh_jobs, refresh_sessions, run, run_then},
};

fn play(store: Store, name: String, track: Option<String>) {
    run(store, "play", PlayArgs { name, track }, move |s: PlaybackStateDto| store.playback.set(s));
}

fn toggle_detail(store: Store, name: String) {
    if store.detail.get_untracked().is_some_and(|d| d.summary.name == name) {
        store.detail.set(None);
        return;
    }
    run(store, "get_session", SessionNameArgs { name }, move |d: SessionDetailDto| store.detail.set(Some(d)));
}

#[component]
pub fn Sessions(store: Store) -> impl IntoView {
    view! {
        <section class="panel" aria-labelledby="sessions-title">
            <div class="panel-head">
                <h2 id="sessions-title">"Sessions"</h2>
                <button class="btn small" on:click=move |_| refresh_sessions(store)>"Refresh"</button>
            </div>
            <ul class="sessions">
                {move || {
                    let list = store.sessions.get();
                    if list.is_empty() {
                        view! { <li class="empty">"No sessions yet. Press Record to make one."</li> }.into_any()
                    } else {
                        list.into_iter().map(|s| view! { <SessionRow store=store summary=s /> }).collect_view().into_any()
                    }
                }}
            </ul>
        </section>
    }
}

#[component]
fn SessionRow(store: Store, summary: SessionSummaryDto) -> impl IntoView {
    let expanded_name = summary.name.clone();
    let toggle_name = summary.name.clone();
    let play_name = summary.name.clone();
    let delete_summary = summary.clone();
    let locked = Signal::derive(move || store.playback_locked());
    let expanded = Signal::derive(move || store.detail.get().filter(|d| d.summary.name == expanded_name));
    let broken = summary.error.is_some();

    let on_delete = move |_| {
        let s = delete_summary.clone();
        store.confirm.set(Some(Confirm {
            title: "Delete session?".into(),
            body: format!(
                "{} ({}, {}) will be permanently deleted along with all of its tracks. This cannot be undone.",
                fmt::started(&s.started_at),
                fmt::clock(s.duration_ms),
                fmt::size(s.bytes),
            ),
            confirm_label: "Delete".into(),
            action: ConfirmAction::DeleteSession(s.name),
        }));
    };

    view! {
        <li class="session">
            <div class="session-row">
                <button
                    class="session-main"
                    aria-expanded=move || expanded.get().is_some().to_string()
                    disabled=broken || summary.recording
                    on:click=move |_| toggle_detail(store, toggle_name.clone())
                >
                    <span class="when">{fmt::started(&summary.started_at)}</span>
                    <span class="meta">
                        {format!(
                            "{} · {} tracks · {}",
                            fmt::clock(summary.duration_ms),
                            summary.device_count,
                            fmt::size(summary.bytes),
                        )}
                    </span>
                </button>
                {summary.error.clone().map(|e| view! { <span class="badge" title=e.clone()>"unreadable"</span> })}
                {summary.recording.then(|| view! { <span class="badge rec">"recording"</span> })}
                {summary.has_transcript.then(|| view! { <span class="badge">"transcript"</span> })}
                {summary.has_summary.then(|| view! { <span class="badge">"summary"</span> })}
                <span class="tip" title=move || locked.get().then_some(PLAYBACK_LOCKED_TIP)>
                    <button
                        class="btn small"
                        disabled=move || locked.get() || broken
                        on:click=move |_| play(store, play_name.clone(), None)
                    >
                        "Play"
                    </button>
                </span>
                <button class="btn small danger-outline" disabled=summary.recording on:click=on_delete>
                    "Delete"
                </button>
            </div>
            {move || expanded.get().map(|d| view! { <SessionDetail store=store detail=d /> })}
        </li>
    }
}

#[derive(Clone, Copy)]
enum Scope {
    Device,
    Segment,
}

fn entry(mix: &MixDto, scope: Scope, key: &str) -> ChannelGainDto {
    let map = match scope {
        Scope::Device => &mix.devices,
        Scope::Segment => &mix.segments,
    };
    map.get(key).cloned().unwrap_or(ChannelGainDto { gain: 1.0, muted: false })
}

fn edit(store: Store, name: &str, mix: RwSignal<MixDto>, scope: Scope, key: &str, f: impl FnOnce(&mut ChannelGainDto)) {
    let mut e = entry(&mix.get_untracked(), scope, key);
    f(&mut e);
    mix.update(|m| {
        let map = match scope {
            Scope::Device => &mut m.devices,
            Scope::Segment => &mut m.segments,
        };
        if e.gain == 1.0 && !e.muted {
            map.remove(key);
        } else {
            map.insert(key.to_string(), e);
        }
        m.auto = false;
    });
    send_mix(store, name, mix);
}

fn send_mix(store: Store, name: &str, mix: RwSignal<MixDto>) {
    run(store, "set_mix", SetMixArgs { name: name.to_string(), mix: mix.get_untracked() }, |_: Empty| {});
}

fn percent(g: f64) -> u32 {
    (g * 100.0).round() as u32
}

#[component]
fn GainControl(store: Store, name: String, mix: RwSignal<MixDto>, scope: Scope, key: String, label: String) -> impl IntoView {
    let current = {
        let key = key.clone();
        Signal::derive(move || entry(&mix.get(), scope, &key))
    };
    let on_mute = {
        let (name, key) = (name.clone(), key.clone());
        move |_| edit(store, &name, mix, scope, &key, |e| e.muted = !e.muted)
    };
    let on_reset = {
        let (name, key) = (name.clone(), key.clone());
        move |_| edit(store, &name, mix, scope, &key, |e| *e = ChannelGainDto { gain: 1.0, muted: false })
    };
    let on_gain = move |ev| {
        let pct = event_target_value(&ev).parse::<f64>().unwrap_or(100.0);
        edit(store, &name, mix, scope, &key, |e| e.gain = (pct / 100.0).clamp(0.0, 4.0));
    };
    view! {
        <span class="gain-group">
            <button
                class="btn small mute"
                aria-pressed=move || current.get().muted.to_string()
                aria-label=format!("Mute {label}")
                on:click=on_mute
            >
                "Mute"
            </button>
            <input
                type="range"
                class="gain"
                min="0"
                max="400"
                step="1"
                aria-label=format!("Volume {label}")
                prop:value=move || percent(current.get().gain).to_string()
                on:input=on_gain
            />
            <span class="gain-value">{move || format!("{}%", percent(current.get().gain))}</span>
            <button
                class="btn small"
                aria-label=format!("Reset {label} to 100%")
                disabled=move || {
                    let c = current.get();
                    c.gain == 1.0 && !c.muted
                }
                on:click=on_reset
            >
                "100%"
            </button>
        </span>
    }
}

#[component]
fn SessionDetail(store: Store, detail: SessionDetailDto) -> impl IntoView {
    let name = detail.summary.name.clone();
    let total = detail.summary.duration_ms.max(1);
    let locked = Signal::derive(move || store.playback_locked());
    let mix = RwSignal::new(detail.mix.clone());
    let selected: RwSignal<Option<(String, u64, u64)>> = RwSignal::new(None);
    let busy = RwSignal::new(false);
    store.mix_event.set(None);
    {
        let name = name.clone();
        Effect::new(move |_| {
            if let Some((n, m)) = store.mix_event.get() {
                if n == name {
                    mix.set(m);
                }
            }
        });
    }

    let uncompressed = detail.segments.iter().any(|s| s.file.ends_with(".wav"));
    let compressing = {
        let name = name.clone();
        Signal::derive(move || {
            store.jobs.get().iter().any(|j| j.kind == JobDtoKind::Compress && j.session == name && job_active(j))
        })
    };
    let compress_button = {
        let name = name.clone();
        (uncompressed || compressing.get_untracked()).then(|| {
            view! {
                <button
                    class="btn small"
                    disabled=move || compressing.get()
                    title="Convert the raw WAV tracks to compressed audio"
                    on:click=move |_| {
                        run(
                            store,
                            "enqueue_compression",
                            EnqueueCompressionArgs { name: name.clone() },
                            move |_: JobDto| refresh_jobs(store),
                        )
                    }
                >
                    {move || if compressing.get() { "Compressing…" } else { "Compress tracks" }}
                </button>
            }
        })
    };

    let toolbar = {
        let (n_auto, n_reset, n_level) = (name.clone(), name.clone(), name.clone());
        view! {
            <div class="mix-toolbar">
                <button
                    class="btn small"
                    disabled=move || busy.get()
                    on:click=move |_| {
                        busy.set(true);
                        run_then(
                            store,
                            "auto_balance",
                            SessionNameArgs { name: n_auto.clone() },
                            move |m: MixDto| mix.set(m),
                            move || busy.set(false),
                        );
                    }
                >
                    {move || if busy.get() { "Balancing…" } else { "Auto-balance" }}
                </button>
                <button
                    class="btn small mute"
                    aria-pressed=move || mix.get().leveling.to_string()
                    on:click=move |_| {
                        mix.update(|m| {
                            m.leveling = !m.leveling;
                            m.auto = false;
                        });
                        send_mix(store, &n_level, mix);
                    }
                >
                    "Leveling"
                </button>
                <button
                    class="btn small"
                    on:click=move |_| {
                        run(store, "reset_mix", SessionNameArgs { name: n_reset.clone() }, move |m: MixDto| mix.set(m))
                    }
                >
                    "Reset all to 100%"
                </button>
                {compress_button}
                {move || mix.get().auto.then(|| view! { <span class="badge auto">"Auto-balanced"</span> })}
            </div>
        }
    };

    let rows = detail
        .devices
        .iter()
        .map(|dev| {
            let key = dev.key.clone();
            let play_name = name.clone();
            let bars = detail
                .segments
                .iter()
                .filter(|s| s.device == dev.key)
                .map(|s| {
                    let end = s.end_offset_ms.unwrap_or(total);
                    let left = s.start_offset_ms as f64 / total as f64 * 100.0;
                    let width = end.saturating_sub(s.start_offset_ms) as f64 / total as f64 * 100.0;
                    let file = s.file.clone();
                    let device = s.device.clone();
                    let truncated = s.status != SegmentDtoStatus::Complete;
                    let start = s.start_offset_ms;
                    let class_file = file.clone();
                    let muted_file = file.clone();
                    let opacity_file = file.clone();
                    let select_file = file.clone();
                    let opacity_device = device.clone();
                    let muted_device = device.clone();
                    view! {
                        <button
                            class="segment"
                            class:truncated=truncated
                            class:selected=move || selected.get().is_some_and(|(f, _, _)| f == class_file)
                            class:muted=move || {
                                let m = mix.get();
                                entry(&m, Scope::Segment, &muted_file).muted || entry(&m, Scope::Device, &muted_device).muted
                            }
                            style:left=format!("{left:.3}%")
                            style:width=format!("{:.3}%", width.max(0.3))
                            style:opacity=move || {
                                let m = mix.get();
                                let g = entry(&m, Scope::Segment, &opacity_file).gain * entry(&m, Scope::Device, &opacity_device).gain;
                                format!("{:.2}", 0.35 + 0.65 * g.min(1.0))
                            }
                            aria-label=format!("Segment {} – {}", fmt::clock(start), fmt::clock(end))
                            title=format!("{} – {}", fmt::clock(start), fmt::clock(end))
                            on:click=move |_| {
                                let next = (select_file.clone(), start, end);
                                selected.update(|cur| {
                                    *cur = if cur.as_ref().is_some_and(|(f, _, _)| *f == next.0) { None } else { Some(next) }
                                });
                            }
                        ></button>
                    }
                })
                .collect_view();
            view! {
                <div class="track">
                    <span class="tip" title=move || locked.get().then_some(PLAYBACK_LOCKED_TIP)>
                        <button
                            class="btn small"
                            aria-label=format!("Play {}", dev.description)
                            disabled=move || locked.get()
                            on:click=move |_| play(store, play_name.clone(), Some(key.clone()))
                        >
                            "Play"
                        </button>
                    </span>
                    <span class="track-name" title=dev.key.clone()>{dev.description.clone()}</span>
                    <GainControl
                        store=store
                        name=name.clone()
                        mix=mix
                        scope=Scope::Device
                        key=dev.key.clone()
                        label=dev.description.clone()
                    />
                    <div class="lane">{bars}</div>
                </div>
            }
        })
        .collect_view();

    let segment_editor = {
        let name = name.clone();
        move || {
            selected.get().map(|(file, start, end)| {
                let label = format!("segment {} – {}", fmt::clock(start), fmt::clock(end));
                view! {
                    <div class="seg-edit">
                        <span class="seg-label">{format!("Selected {label}")}</span>
                        <GainControl store=store name=name.clone() mix=mix scope=Scope::Segment key=file label=label />
                    </div>
                }
            })
        }
    };

    let transcript_name = name.clone();
    view! {
        <div class="detail">
            {toolbar}{rows}{segment_editor}<SummarySection store=store name=name.clone() />
            <TranscriptSection store=store name=transcript_name />
        </div>
    }
}
