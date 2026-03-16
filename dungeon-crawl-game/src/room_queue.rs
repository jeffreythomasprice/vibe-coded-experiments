use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use rand::SeedableRng;
use rand::rngs::StdRng;
use tracing::info;

use crate::generator::LlmClient;
use crate::schema_types::*;

struct Inner {
    rooms: VecDeque<Room>,
    used_names: Vec<String>,
    in_flight: usize,
}

#[derive(Resource, Clone)]
pub struct RoomQueue {
    inner: Arc<Mutex<Inner>>,
}

impl RoomQueue {
    pub fn new(
        config: Config,
        themes: Vec<GeneratorContext>,
        target_size: usize,
        llm: Arc<dyn LlmClient>,
    ) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            rooms: VecDeque::new(),
            used_names: Vec::new(),
            in_flight: 0,
        }));

        let shared = inner.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            let mut rng = StdRng::from_os_rng();

            rt.block_on(async {
                loop {
                    let should_generate = {
                        let lock = shared.lock().unwrap();
                        lock.rooms.len() + lock.in_flight < target_size
                    };

                    if should_generate {
                        let existing_names = {
                            let mut lock = shared.lock().unwrap();
                            lock.in_flight += 1;
                            let mut names = lock.used_names.clone();
                            names.extend(lock.rooms.iter().map(|r| r.name.clone()));
                            names
                        };

                        let magnitude: f64 = rand::Rng::random_range(&mut rng, 0.0..=1.0);
                        match crate::generator::generate_room(
                            magnitude, &mut rng, &themes, &config, &existing_names, llm.as_ref(),
                        )
                        .await
                        {
                            Ok(room) => {
                                info!("Generated room:\n{}", serde_json::to_string_pretty(&room).unwrap_or_default());
                                let mut lock = shared.lock().unwrap();
                                lock.used_names.push(room.name.clone());
                                lock.rooms.push_back(room);
                                lock.in_flight -= 1;
                            }
                            Err(e) => {
                                tracing::error!("Room generation failed: {e}");
                                shared.lock().unwrap().in_flight -= 1;
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            }
                        }
                    } else {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            });
        });

        Self { inner }
    }

    pub fn try_pop(&self) -> Option<Room> {
        self.inner.lock().unwrap().rooms.pop_front()
    }

    pub fn try_find<F: Fn(&Room) -> bool>(&self, predicate: F) -> Option<Room> {
        let mut lock = self.inner.lock().unwrap();
        if let Some(idx) = lock.rooms.iter().position(|r| predicate(r)) {
            lock.rooms.remove(idx)
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().rooms.len()
    }
}
