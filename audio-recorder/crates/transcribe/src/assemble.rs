use std::collections::HashMap;

use audio_recorder_schema::transcript::{Snippet, TranscriptSpeaker};

use crate::audio::{DeviceAudio, RATE};

const MAX_GAP_S: f64 = 1.5;
const MAX_WORDS: usize = 40;
const ISLAND_MAX_S: f64 = 1.2;
const ISLAND_MAX_WORDS: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub start_s: f64,
    pub end_s: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub start_s: f64,
    pub end_s: f64,
    pub speaker: i32,
}

pub struct DeviceInput<'a> {
    pub device: &'a str,
    pub audio: &'a DeviceAudio,
    pub words: Vec<Word>,
    pub turns: Vec<Turn>,
}

struct Raw {
    device: String,
    local_speaker: Option<i32>,
    snippet: Snippet,
}

pub fn speaker_for(word: &Word, turns: &[Turn]) -> Option<i32> {
    let overlap = |t: &Turn| (word.end_s.min(t.end_s) - word.start_s.max(t.start_s)).max(0.0);
    let best = turns.iter().map(|t| (overlap(t), t)).filter(|(o, _)| *o > 0.0).max_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, t)) = best {
        return Some(t.speaker);
    }
    let mid = (word.start_s + word.end_s) / 2.0;
    let distance = |t: &Turn| if mid < t.start_s { t.start_s - mid } else if mid > t.end_s { mid - t.end_s } else { 0.0 };
    turns.iter().min_by(|a, b| distance(a).total_cmp(&distance(b))).map(|t| t.speaker)
}

fn smooth_islands(words: &[&Word], speakers: &mut [Option<i32>]) {
    let mut i = 0;
    while i < words.len() {
        let mut j = i + 1;
        while j < words.len() && speakers[j] == speakers[i] {
            j += 1;
        }
        let surrounded = i > 0 && j < words.len() && speakers[i - 1].is_some() && speakers[i - 1] == speakers[j];
        if surrounded
            && speakers[i] != speakers[i - 1]
            && j - i <= ISLAND_MAX_WORDS
            && words[j - 1].end_s - words[i].start_s <= ISLAND_MAX_S
            && words[i].start_s - words[i - 1].end_s <= MAX_GAP_S
            && words[j].start_s - words[j - 1].end_s <= MAX_GAP_S
        {
            let outer = speakers[i - 1];
            speakers[i..j].iter_mut().for_each(|s| *s = outer);
        }
        i = j;
    }
}

fn sentence_end(text: &str) -> bool {
    text.trim_end().ends_with(['.', '?', '!'])
}

fn is_noise(text: &str) -> bool {
    let t = text.trim();
    t.is_empty() || (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')'))
}

fn device_snippets(input: &DeviceInput) -> Vec<Raw> {
    let mut out: Vec<Raw> = Vec::new();
    let mut current: Option<(Raw, f64, usize)> = None;
    let to_sample = |s: f64| (s.max(0.0) * RATE as f64) as usize;

    let flush = |current: &mut Option<(Raw, f64, usize)>, out: &mut Vec<Raw>| {
        if let Some((raw, _, _)) = current.take() {
            out.push(raw);
        }
    };

    let words: Vec<&Word> = input.words.iter().filter(|w| !is_noise(&w.text)).collect();
    let mut speakers: Vec<Option<i32>> = words.iter().map(|w| speaker_for(w, &input.turns)).collect();
    smooth_islands(&words, &mut speakers);

    for (word, speaker) in words.into_iter().zip(speakers) {
        let Some((start_ns, file)) = input.audio.locate(to_sample(word.start_s)) else { continue };
        let end_ns = input
            .audio
            .locate(to_sample(word.end_s))
            .map_or(start_ns, |(ns, _)| ns.max(start_ns));

        let split = match &current {
            Some((raw, last_end, count)) => {
                raw.local_speaker != speaker
                    || word.start_s - last_end > MAX_GAP_S
                    || sentence_end(&raw.snippet.text)
                    || raw.snippet.segment != file
                    || *count >= MAX_WORDS
            }
            None => false,
        };
        if split {
            flush(&mut current, &mut out);
        }
        match &mut current {
            Some((raw, last_end, count)) => {
                raw.snippet.text.push_str(&word.text);
                raw.snippet.end_ns = end_ns;
                *last_end = word.end_s;
                *count += 1;
            }
            None => {
                current = Some((
                    Raw {
                        device: input.device.to_string(),
                        local_speaker: speaker,
                        snippet: Snippet {
                            source: input.device.to_string(),
                            speaker: None,
                            segment: file.to_string(),
                            start_ns,
                            end_ns,
                            text: word.text.clone(),
                        },
                    },
                    word.end_s,
                    1,
                ));
            }
        }
    }
    flush(&mut current, &mut out);
    for raw in &mut out {
        raw.snippet.text = raw.snippet.text.trim().to_string();
    }
    out.retain(|r| !r.snippet.text.is_empty());
    out
}

pub fn assemble(inputs: &[DeviceInput]) -> (Vec<Snippet>, HashMap<String, TranscriptSpeaker>) {
    let mut raws: Vec<Raw> = inputs.iter().flat_map(device_snippets).collect();
    raws.sort_by(|a, b| a.snippet.start_ns.cmp(&b.snippet.start_ns).then_with(|| a.device.cmp(&b.device)));

    let mut ids: HashMap<(String, i32), String> = HashMap::new();
    let mut speakers = HashMap::new();
    let mut snippets = Vec::with_capacity(raws.len());
    for mut raw in raws {
        if let Some(local) = raw.local_speaker {
            let next = ids.len() + 1;
            let id = ids.entry((raw.device.clone(), local)).or_insert_with(|| format!("S{next}")).clone();
            speakers.entry(id.clone()).or_insert_with(|| TranscriptSpeaker {
                source: raw.device.clone(),
                label: format!("Speaker {}", &id[1..]),
            });
            raw.snippet.speaker = Some(id);
        }
        snippets.push(raw.snippet);
    }
    (snippets, speakers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::SegmentSpan;

    fn word(start_s: f64, end_s: f64, text: &str) -> Word {
        Word { start_s, end_s, text: text.into() }
    }

    fn audio(spans: &[(&str, usize, usize, u64)]) -> DeviceAudio {
        DeviceAudio {
            samples: Vec::new(),
            spans: spans
                .iter()
                .map(|(f, start, len, off)| SegmentSpan { file: f.to_string(), start_sample: *start, len: *len, offset_ns: *off })
                .collect(),
        }
    }

    #[test]
    fn speaker_by_overlap_then_nearest() {
        let turns = [Turn { start_s: 0.0, end_s: 2.0, speaker: 0 }, Turn { start_s: 2.0, end_s: 5.0, speaker: 1 }];
        assert_eq!(speaker_for(&word(1.5, 2.5, "x"), &turns), Some(1));
        assert_eq!(speaker_for(&word(1.0, 1.4, "x"), &turns), Some(0));
        assert_eq!(speaker_for(&word(9.0, 9.5, "x"), &turns), Some(1));
        assert_eq!(speaker_for(&word(1.0, 1.4, "x"), &[]), None);
    }

    #[test]
    fn brief_islands_inside_one_speaker_are_absorbed_but_real_turns_are_not() {
        let a = audio(&[("f", 0, 16_000 * 60, 0)]);
        let turns = vec![
            Turn { start_s: 0.0, end_s: 3.0, speaker: 0 },
            Turn { start_s: 3.0, end_s: 3.6, speaker: 1 },
            Turn { start_s: 3.6, end_s: 7.0, speaker: 0 },
            Turn { start_s: 7.0, end_s: 12.0, speaker: 2 },
            Turn { start_s: 12.0, end_s: 20.0, speaker: 0 },
        ];
        let words = vec![
            word(2.4, 2.9, " one"),
            word(3.0, 3.3, " blip"),
            word(3.3, 3.6, " blip2"),
            word(4.0, 4.4, " two"),
            word(8.0, 9.5, " third"),
            word(9.5, 11.5, " speaker"),
            word(12.2, 12.6, " back"),
        ];
        let (snips, speakers) = assemble(&[DeviceInput { device: "input:a", audio: &a, words, turns }]);
        let text: Vec<_> = snips.iter().map(|s| (s.text.as_str(), s.speaker.as_deref())).collect();
        assert_eq!(text, [("one blip blip2 two", Some("S1")), ("third speaker", Some("S2")), ("back", Some("S1"))]);
        assert_eq!(speakers.len(), 2);
    }

    #[test]
    fn splits_on_speaker_change_gap_and_sentence_end() {
        let a = audio(&[("t/a.wav", 0, 16_000 * 60, 0)]);
        let turns = vec![Turn { start_s: 0.0, end_s: 3.0, speaker: 0 }, Turn { start_s: 3.0, end_s: 10.0, speaker: 1 }];
        let words = vec![
            word(0.0, 0.4, " Hello"),
            word(0.4, 0.8, " there"),
            word(1.0, 1.5, " friend."),
            word(1.6, 2.0, " How"),
            word(3.2, 3.6, " Fine"),
            word(6.0, 6.5, " later"),
        ];
        let (snips, speakers) = assemble(&[DeviceInput { device: "input:a", audio: &a, words, turns }]);
        let texts: Vec<_> = snips.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["Hello there friend.", "How", "Fine", "later"]);
        assert_eq!(snips[0].speaker.as_deref(), Some("S1"));
        assert_eq!(snips[2].speaker.as_deref(), Some("S2"));
        assert_eq!(speakers["S2"].label, "Speaker 2");
        assert_eq!(snips[0].start_ns, 0);
        assert_eq!(snips[0].end_ns, 1_500_000_000);
    }

    #[test]
    fn interleaves_devices_and_maps_segment_offsets() {
        let mic = audio(&[("t/mic-0.wav", 0, 16_000 * 10, 2_000_000_000), ("t/mic-1.wav", 16_000 * 10 + 8_000, 16_000 * 10, 100_000_000_000)]);
        let sink = audio(&[("t/sink-0.wav", 0, 16_000 * 30, 0)]);
        let mic_words = vec![word(0.0, 0.5, " first"), word(10.5 + 1.0, 12.0, " third")];
        let sink_words = vec![word(4.0, 4.5, " second")];
        let (snips, speakers) = assemble(&[
            DeviceInput { device: "input:mic", audio: &mic, words: mic_words, turns: vec![Turn { start_s: 0.0, end_s: 30.0, speaker: 0 }] },
            DeviceInput { device: "output:sink", audio: &sink, words: sink_words, turns: vec![Turn { start_s: 0.0, end_s: 30.0, speaker: 0 }] },
        ]);
        let order: Vec<_> = snips.iter().map(|s| (s.source.as_str(), s.text.as_str(), s.start_ns)).collect();
        assert_eq!(
            order,
            [
                ("input:mic", "first", 2_000_000_000),
                ("output:sink", "second", 4_000_000_000),
                ("input:mic", "third", 101_000_000_000),
            ]
        );
        assert_eq!(snips[2].segment, "t/mic-1.wav");
        assert_eq!(snips[0].speaker.as_deref(), Some("S1"));
        assert_eq!(snips[1].speaker.as_deref(), Some("S2"));
        assert_eq!(speakers.len(), 2);
    }

    #[test]
    fn drops_noise_markers_and_undiarized_has_no_speaker() {
        let a = audio(&[("f", 0, 16_000 * 10, 0)]);
        let words = vec![word(0.0, 1.0, " [BLANK_AUDIO]"), word(1.0, 1.5, " hi")];
        let (snips, speakers) = assemble(&[DeviceInput { device: "input:a", audio: &a, words, turns: vec![] }]);
        assert_eq!(snips.len(), 1);
        assert_eq!(snips[0].speaker, None);
        assert!(speakers.is_empty());
    }
}
