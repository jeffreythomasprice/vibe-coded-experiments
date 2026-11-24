use audio_recorder_schema::ipc::{DeviceDto, Direction, Empty, SetDeviceEnabledArgs};
use leptos::prelude::*;

use crate::store::{Store, run};

#[component]
pub fn Devices(store: Store) -> impl IntoView {
    let group = move |direction: Direction| {
        store.devices.get().into_iter().filter(|d| d.direction == direction).collect::<Vec<_>>()
    };
    view! {
        <section class="panel" aria-labelledby="devices-title">
            <h2 id="devices-title">"Devices"</h2>
            <h3>"Inputs"</h3>
            <DeviceList store=store devices=Signal::derive(move || group(Direction::Input)) />
            <h3>"Outputs"</h3>
            <DeviceList store=store devices=Signal::derive(move || group(Direction::Output)) />
        </section>
    }
}

#[component]
fn DeviceList(store: Store, devices: Signal<Vec<DeviceDto>>) -> impl IntoView {
    view! {
        <ul class="devices">
            {move || {
                let list = devices.get();
                if list.is_empty() {
                    view! { <li class="empty">"No devices"</li> }.into_any()
                } else {
                    list.into_iter().map(|d| view! { <DeviceRow store=store device=d /> }).collect_view().into_any()
                }
            }}
        </ul>
    }
}

#[component]
fn DeviceRow(store: Store, device: DeviceDto) -> impl IntoView {
    let meter_key = device.key.clone();
    let toggle_key = device.key.clone();
    let level = move || {
        let peak = store.levels.get().get(&meter_key).copied().unwrap_or(0.0);
        format!("{:.0}%", peak.clamp(0.0, 1.0) * 100.0)
    };
    let on_change = move |ev| {
        let enabled = event_target_checked(&ev);
        run(store, "set_device_enabled", SetDeviceEnabledArgs { key: toggle_key.clone(), enabled }, move |_: Empty| {});
    };
    view! {
        <li class:absent=!device.present class:off=!device.enabled>
            <label>
                <input type="checkbox" prop:checked=device.enabled on:change=on_change />
                <span class="name" title=device.key.clone()>{device.description.clone()}</span>
            </label>
            {(!device.present).then(|| view! { <span class="badge">"not connected"</span> })}
            {device.recording.then(|| view! { <span class="rec-dot" role="img" aria-label="recording" title="recording"></span> })}
            <div class="meter" aria-hidden="true">
                <div class="meter-fill" style:width=level></div>
            </div>
        </li>
    }
}
