//! Player faces' detail made off the frame: a picture's face and edge maps
//! ([`crate::face_detail::face_map`], [`crate::face_detail::edge_map`]) and a near head's shape
//! ([`crate::face_detail::shape_head`]) worked out on a thread of their own, so a face first seen
//! never holds the frame up. The world draws the plain face meanwhile and takes the detail as it
//! comes ([`crate::world::AssetLoader::tick_faces`]).

use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, Sender};

use pyreal_assets::GfxObj;
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

use crate::asset::mesh::{gfxobj_mesh, MeshData};
use crate::asset::texture::TexturePixels;

/// What the thread is asked for.
enum Job {
    /// A face or hair picture's two maps.
    Maps(DataId),
    /// A player head model (worn as head key `1`) shaded inside what covers it where `2` names
    /// the skin it keeps, then shaped in full.
    Head(DataId, u64, Vec<DataId>),
}

/// What the thread hands back.
#[allow(missing_debug_implementations)]
pub enum Made {
    /// Picture `0`'s face map and edge map (`None` where it has none).
    Maps(DataId, Option<TexturePixels>, Option<TexturePixels>),
    /// Head key `1` (model `0`) shaped in full (`None` where it will not load).
    Head(DataId, u64, Option<MeshData>),
}

/// The thread, started at the first request, and what it has been asked for and not yet
/// handed back.
#[derive(Default)]
#[allow(missing_debug_implementations)]
pub struct FaceWork {
    line: Option<(Sender<Job>, Receiver<Made>)>,
    maps_asked: HashSet<u32>,
    heads_asked: HashSet<u64>,
}

impl FaceWork {
    fn send(&mut self, store: &RetailDatStore, job: Job) -> bool {
        if self.line.is_none() {
            let (jobs, rx) = channel::<Job>();
            let (tx, made) = channel::<Made>();
            let store = store.clone();
            let started = std::thread::Builder::new()
                .name("faces".into())
                .spawn(move || {
                    while let Ok(job) = rx.recv() {
                        let out = match job {
                            Job::Maps(id) => Made::Maps(
                                id,
                                crate::face_detail::face_map(&store, id),
                                crate::face_detail::edge_map(&store, id),
                            ),
                            Job::Head(id, key, keep) => {
                                Made::Head(id, key, head(&store, id, &keep))
                            }
                        };
                        if tx.send(out).is_err() {
                            break;
                        }
                    }
                });
            if started.is_err() {
                return false;
            }
            self.line = Some((jobs, made));
        }
        self.line
            .as_ref()
            .is_some_and(|(jobs, _)| jobs.send(job).is_ok())
    }

    /// Asks for picture `id`'s maps, once until they are handed back.
    pub fn ask_maps(&mut self, store: &RetailDatStore, id: DataId) {
        if !self.maps_asked.contains(&id.0) && self.send(store, Job::Maps(id)) {
            self.maps_asked.insert(id.0);
        }
    }

    /// Asks for head model `id`, worn as head key `key` keeping skin `keep`, shaped in full, once
    /// until it is handed back.
    pub fn ask_head(&mut self, store: &RetailDatStore, id: DataId, key: u64, keep: &[DataId]) {
        if !self.heads_asked.contains(&key) && self.send(store, Job::Head(id, key, keep.to_vec())) {
            self.heads_asked.insert(key);
        }
    }

    /// What the thread has made since the last call, up to `most`.
    pub fn take(&mut self, most: usize) -> Vec<Made> {
        let Some((_, made)) = self.line.as_ref() else {
            return Vec::new();
        };
        let out: Vec<Made> = made.try_iter().take(most).collect();
        for m in &out {
            match m {
                Made::Maps(id, ..) => self.maps_asked.remove(&id.0),
                Made::Head(_, key, _) => self.heads_asked.remove(key),
            };
        }
        out
    }
}

/// Head model `id`'s mesh shaded inside what covers the skin `keep` names (none when empty), then
/// shaped in full, as the loader would make it.
fn head(store: &RetailDatStore, id: DataId, keep: &[DataId]) -> Option<MeshData> {
    let bytes = store.read_typed(DbType::GfxObj, id).ok()?;
    let obj =
        <GfxObj as pyreal_assets::Decode>::decode_payload_in(store.era_of(id), id, &bytes).ok()?;
    let mut mesh = gfxobj_mesh(&obj);
    if mesh.indices.is_empty() {
        return None;
    }
    if !keep.is_empty() {
        crate::asset::mesh::shade_inside(&mut mesh, keep, crate::asset::mesh::SHADE_REACH);
    }
    Some(crate::face_detail::shape_head(store, id, &mesh))
}
