use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::mpsc,
    time::Instant,
};

use crossbeam_channel::{Sender, TrySendError};
use pipewire as pw;
use pw::{properties::properties, spa, types::ObjectType};
use spa::param::{
    audio::{AudioFormat, AudioInfoRaw},
    format::{MediaSubtype, MediaType},
    format_utils,
};
use spa::pod::Pod;

use super::{CaptureMsg, EndReason, PwCommand, PwEvent};
use crate::device::{DeviceInfo, DeviceKey, Direction};

struct Capture {
    key: DeviceKey,
    stream: pw::stream::StreamRc,
    _listener: pw::stream::StreamListener<CaptureData>,
}

struct CaptureData {
    id: u64,
    format: AudioInfoRaw,
    tx: Sender<CaptureMsg>,
    events: Sender<PwEvent>,
    format_sent: bool,
}

pub(super) fn run(
    cmd_rx: pw::channel::Receiver<PwCommand>,
    events: Sender<PwEvent>,
    init: mpsc::Sender<Result<(), String>>,
) {
    pw::init();
    if let Err(e) = run_loop(cmd_rx, &events, &init) {
        let _ = init.send(Err(e.to_string()));
    }
    let _ = events.send(PwEvent::Stopped);
}

fn node_device(props: &spa::utils::dict::DictRef, id: u32) -> Option<DeviceInfo> {
    let direction = match props.get("media.class")? {
        "Audio/Source" => Direction::Input,
        "Audio/Sink" => Direction::Output,
        _ => return None,
    };
    let name = props.get("node.name")?;
    let description = props
        .get("node.description")
        .or_else(|| props.get("node.nick"))
        .unwrap_or(name)
        .to_string();
    Some(DeviceInfo { key: DeviceKey::new(direction, name), description, pw_id: id })
}

fn run_loop(
    cmd_rx: pw::channel::Receiver<PwCommand>,
    events: &Sender<PwEvent>,
    init: &mpsc::Sender<Result<(), String>>,
) -> Result<(), pw::Error> {
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let nodes: Rc<RefCell<HashMap<u32, DeviceKey>>> = Rc::default();
    let captures: Rc<RefCell<HashMap<u64, Capture>>> = Rc::default();

    let sync_seq = Rc::new(Cell::new(None));
    let _core_listener = {
        let events = events.clone();
        let sync_seq = sync_seq.clone();
        let mainloop = mainloop.downgrade();
        core.add_listener_local()
            .done(move |id, seq| {
                if id == pw::core::PW_ID_CORE && Some(seq) == sync_seq.get() {
                    sync_seq.set(None);
                    let _ = events.send(PwEvent::Synced);
                }
            })
            .error(move |id, _seq, res, msg| {
                if id == pw::core::PW_ID_CORE && res == -32 {
                    tracing::error!("pipewire connection lost: {msg}");
                    if let Some(ml) = mainloop.upgrade() {
                        ml.quit();
                    }
                }
            })
            .register()
    };

    let _registry_listener = {
        let events_add = events.clone();
        let events_rm = events.clone();
        let nodes_add = nodes.clone();
        let nodes_rm = nodes.clone();
        let captures_rm = captures.clone();
        registry
            .add_listener_local()
            .global(move |obj| {
                if obj.type_ != ObjectType::Node {
                    return;
                }
                let Some(props) = obj.props else { return };
                if let Some(info) = node_device(props, obj.id) {
                    nodes_add.borrow_mut().insert(obj.id, info.key.clone());
                    let _ = events_add.send(PwEvent::DeviceAdded(info));
                }
            })
            .global_remove(move |id| {
                let Some(key) = nodes_rm.borrow_mut().remove(&id) else { return };
                let ended: Vec<u64> = captures_rm
                    .borrow()
                    .iter()
                    .filter(|(_, c)| c.key == key)
                    .map(|(id, _)| *id)
                    .collect();
                for cap_id in ended {
                    if let Some(cap) = captures_rm.borrow_mut().remove(&cap_id) {
                        let _ = cap.stream.disconnect();
                    }
                    let _ = events_rm.send(PwEvent::CaptureEnded { id: cap_id, reason: EndReason::DeviceRemoved });
                }
                let _ = events_rm.send(PwEvent::DeviceRemoved(key));
            })
            .register()
    };

    let _cmd_attached = {
        let mainloop_weak = mainloop.downgrade();
        let captures = captures.clone();
        let core = core.clone();
        let events = events.clone();
        cmd_rx.attach(mainloop.loop_(), move |cmd| match cmd {
            PwCommand::Shutdown => {
                if let Some(ml) = mainloop_weak.upgrade() {
                    ml.quit();
                }
            }
            PwCommand::StopCapture { id } => {
                if let Some(cap) = captures.borrow_mut().remove(&id) {
                    let _ = cap.stream.disconnect();
                }
            }
            PwCommand::StartCapture { id, key, tx } => match start_capture(&core, id, &key, tx, events.clone()) {
                Ok(cap) => {
                    captures.borrow_mut().insert(id, cap);
                }
                Err(e) => {
                    let _ = events.send(PwEvent::CaptureEnded { id, reason: EndReason::Error(e.to_string()) });
                }
            },
        })
    };

    sync_seq.set(Some(core.sync(0)?));
    let _ = init.send(Ok(()));
    mainloop.run();

    for (_, cap) in captures.borrow_mut().drain() {
        let _ = cap.stream.disconnect();
    }
    Ok(())
}

fn start_capture(
    core: &pw::core::CoreRc,
    id: u64,
    key: &DeviceKey,
    tx: Sender<CaptureMsg>,
    events: Sender<PwEvent>,
) -> Result<Capture, pw::Error> {
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Production",
        *pw::keys::APP_NAME => "audio-recorder",
        "target.object" => key.node_name.as_str(),
        "node.dont-reconnect" => "true",
    };
    if key.direction == Direction::Output {
        props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
    }
    let stream = pw::stream::StreamRc::new(core.clone(), "audio-recorder-capture", props)?;

    let data = CaptureData { id, format: Default::default(), tx, events, format_sent: false };
    let listener = stream
        .add_local_listener_with_user_data(data)
        .state_changed(|_, data, _old, new| {
            if let pw::stream::StreamState::Error(msg) = new {
                let _ = data.events.send(PwEvent::CaptureEnded { id: data.id, reason: EndReason::Error(msg) });
            }
        })
        .param_changed(|_, data, param_id, param| {
            let Some(param) = param else { return };
            if param_id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else { return };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }
            if data.format.parse(param).is_err() {
                return;
            }
            if !data.format_sent {
                data.format_sent = true;
                let _ = data.tx.send(CaptureMsg::Format { rate: data.format.rate(), channels: data.format.channels() });
            }
        })
        .process(|stream, data| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let datas = buffer.datas_mut();
            let Some(d) = datas.first_mut() else { return };
            let offset = d.chunk().offset() as usize;
            let size = d.chunk().size() as usize;
            let Some(bytes) = d.data() else { return };
            let Some(bytes) = bytes.get(offset..offset + size) else { return };
            let samples: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
            if samples.is_empty() || !data.format_sent {
                return;
            }
            match data.tx.try_send(CaptureMsg::Samples { arrived: Instant::now(), data: samples }) {
                Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
            }
        })
        .register()?;

    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(AudioFormat::F32LE);
    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|_| pw::Error::CreationFailed)?
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&values).ok_or(pw::Error::CreationFailed)?];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;

    Ok(Capture { key: key.clone(), stream, _listener: listener })
}
