use std::{
    path::PathBuf,
    process::Command,
    thread::sleep,
    time::{Duration, Instant},
};

use audio_recorder_core::{
    ResolvedConfig,
    device::DeviceKey,
    session::{LiveAudio, Recorder, manifest},
};
use audio_recorder_schema::{config::SampleFormat, manifest::SegmentStatus};

const SINK: &str = "arc_hotplug_test";

fn pactl(args: &[&str]) -> String {
    let out = Command::new("pactl").args(args).output().expect("pactl available");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn load_sink() -> String {
    pactl(&["load-module", "module-null-sink", &format!("sink_name={SINK}"), "media.class=Audio/Sink", "channel_map=stereo"])
}

fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "needs a running PipeWire with pactl"]
fn sink_hotplug_mid_session_produces_two_segments() {
    let base = std::env::temp_dir().join(format!("audio-recorder-hotplug-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let config = ResolvedConfig {
        sessions_dir: base.join("sessions"),
        sample_format: SampleFormat::F32,
        transcription: Default::default(),
        compression: Default::default(),
        llm: Default::default(),
        summarization: Default::default(),
        log_file: base.join("log.txt"),
        source: None,
    };

    let module = load_sink();
    let recorder = Recorder::start(config, &base.join("devices.json")).unwrap();
    let key: DeviceKey = format!("output:{SINK}").parse().unwrap();
    for d in recorder.devices().unwrap() {
        if d.key != key {
            recorder.set_enabled(&d.key, false).unwrap();
        }
    }
    let audio = recorder.subscribe_audio();

    let dir: PathBuf = recorder.start_session().unwrap();
    wait_for("capture to start", || recorder.devices().unwrap().iter().any(|d| d.key == key && d.recording));
    sleep(Duration::from_millis(1200));

    pactl(&["unload-module", &module]);
    wait_for("device removal", || recorder.devices().unwrap().iter().any(|d| d.key == key && !d.present));
    sleep(Duration::from_millis(500));

    let module = load_sink();
    wait_for("capture to resume", || recorder.devices().unwrap().iter().any(|d| d.key == key && d.recording));
    sleep(Duration::from_millis(1200));

    recorder.stop_session().unwrap();
    pactl(&["unload-module", &module]);

    let m = manifest::load(&dir).unwrap();
    let segs: Vec<_> = m.segments.iter().filter(|s| s.device == key.to_string()).collect();
    assert_eq!(segs.len(), 2, "{segs:#?}");
    assert!(segs.iter().all(|s| s.status == SegmentStatus::Complete && s.frames > 0));
    assert!(segs[1].start_offset_ns > segs[0].end_offset_ns.unwrap());

    let msgs: Vec<LiveAudio> = audio.try_iter().collect();
    for seg in &segs {
        let chunks: Vec<u64> = msgs
            .iter()
            .filter_map(|m| match m {
                LiveAudio::Chunk { file, offset_ns, .. } if *file == seg.file => Some(*offset_ns),
                _ => None,
            })
            .collect();
        assert!(!chunks.is_empty(), "no live chunks for {}", seg.file);
        assert!(chunks.windows(2).all(|w| w[0] < w[1]), "offsets not increasing for {}", seg.file);
        assert!(chunks[0].abs_diff(seg.start_offset_ns) < 50_000_000, "{} vs {}", chunks[0], seg.start_offset_ns);
        let ends = msgs.iter().filter(|m| matches!(m, LiveAudio::SegmentEnd { file, .. } if *file == seg.file)).count();
        assert_eq!(ends, 1, "{}", seg.file);
    }
}

struct Rig {
    recorder: Recorder,
    key: DeviceKey,
    module: String,
    base: PathBuf,
}

fn rig(name: &str) -> Rig {
    let base = std::env::temp_dir().join(format!("audio-recorder-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let module = load_sink();
    let recorder = start_recorder(&base);
    let key: DeviceKey = format!("output:{SINK}").parse().unwrap();
    for d in recorder.devices().unwrap() {
        if d.key != key {
            recorder.set_enabled(&d.key, false).unwrap();
        }
    }
    Rig { recorder, key, module, base }
}

fn start_recorder(base: &std::path::Path) -> Recorder {
    let config = ResolvedConfig {
        sessions_dir: base.join("sessions"),
        sample_format: SampleFormat::F32,
        transcription: Default::default(),
        compression: Default::default(),
        llm: Default::default(),
        summarization: Default::default(),
        log_file: base.join("log.txt"),
        source: None,
    };
    Recorder::start(config, &base.join("devices.json")).unwrap()
}

fn recording(r: &Recorder, key: &DeviceKey) -> bool {
    r.devices().unwrap().iter().any(|d| &d.key == key && d.recording)
}

#[test]
#[ignore = "needs a running PipeWire with pactl"]
fn pause_and_resume_leave_a_gap_and_stop_works_while_paused() {
    let rig = rig("pause");
    let dir = rig.recorder.start_session().unwrap();
    wait_for("capture to start", || recording(&rig.recorder, &rig.key));
    sleep(Duration::from_millis(1200));

    rig.recorder.pause_session().unwrap();
    wait_for("capture to stop", || !recording(&rig.recorder, &rig.key));
    assert!(rig.recorder.session_status().unwrap().unwrap().paused);
    assert!(rig.recorder.pause_session().is_err());
    sleep(Duration::from_millis(1500));
    let frozen = rig.recorder.session_status().unwrap().unwrap().elapsed;
    sleep(Duration::from_millis(300));
    assert_eq!(rig.recorder.session_status().unwrap().unwrap().elapsed, frozen, "elapsed freezes while paused");

    rig.recorder.resume_session().unwrap();
    wait_for("capture to resume", || recording(&rig.recorder, &rig.key));
    sleep(Duration::from_millis(1200));
    rig.recorder.stop_session().unwrap();

    let m = manifest::load(&dir).unwrap();
    let segs: Vec<_> = m.segments.iter().filter(|s| s.device == rig.key.to_string()).collect();
    assert_eq!(segs.len(), 2, "{segs:#?}");
    let gap = segs[1].start_offset_ns - segs[0].end_offset_ns.unwrap();
    assert!(gap >= 1_000_000_000, "gap was {gap}ns");

    let dir2 = rig.recorder.start_session().unwrap();
    rig.recorder.pause_session().unwrap();
    rig.recorder.stop_session().unwrap();
    assert!(manifest::load(&dir2).unwrap().ended_at.is_some());
    pactl(&["unload-module", &rig.module]);
}

#[test]
#[ignore = "needs a running PipeWire with pactl"]
fn toggling_a_device_mid_session_closes_and_reopens_segments() {
    let rig = rig("toggle");
    let dir = rig.recorder.start_session().unwrap();
    wait_for("capture to start", || recording(&rig.recorder, &rig.key));
    sleep(Duration::from_millis(1000));

    rig.recorder.set_enabled(&rig.key, false).unwrap();
    wait_for("capture to stop", || !recording(&rig.recorder, &rig.key));
    sleep(Duration::from_millis(500));
    rig.recorder.set_enabled(&rig.key, true).unwrap();
    wait_for("capture to restart", || recording(&rig.recorder, &rig.key));
    sleep(Duration::from_millis(1000));
    rig.recorder.stop_session().unwrap();

    let m = manifest::load(&dir).unwrap();
    assert_eq!(m.segments.iter().filter(|s| s.device == rig.key.to_string()).count(), 2);
    pactl(&["unload-module", &rig.module]);
}

#[test]
#[ignore = "needs a running PipeWire with pactl"]
fn disabled_state_survives_restart() {
    let rig = rig("persist");
    rig.recorder.set_enabled(&rig.key, false).unwrap();
    let Rig { recorder, key, module, base } = rig;
    drop(recorder);

    let again = start_recorder(&base);
    let status = again.devices().unwrap().into_iter().find(|d| d.key == key).unwrap();
    assert!(!status.enabled);
    pactl(&["unload-module", &module]);
}
