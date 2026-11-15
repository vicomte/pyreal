//! Resolve texture record chains and their palette records without interpreting pixels.

use crate::{Decode, Palette, RenderSurface, RenderTexture, Surface, SurfaceTexture};
use pyreal_dat::{divine_type, ContainerEra, DbType, RetailDatStore};
use pyreal_primitives::DataId;

/// A missing, malformed or non-texture record in a texture chain.
#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("{0}: {1}")]
    Dat(DataId, pyreal_dat::DatError),
    #[error("{0}: {1}")]
    Asset(DataId, crate::AssetError),
    #[error("{0} is not a texture id, or names an empty level chain")]
    NotATexture(DataId),
}

/// The most hops a chain takes from a surface to its pixels (surface, texture, image); a longer
/// chain loops.
const MAX_HOPS: u32 = 4;

/// The record lookup shared by products that display images from the data files.
#[derive(Debug)]
pub struct TextureLookup<'a> {
    store: &'a RetailDatStore,
    keep_high_detail: bool,
}

impl<'a> TextureLookup<'a> {
    /// Select the high-resolution level only when the store has `client_highres.dat` open and
    /// detail is zero; a level in another pixel format than the original's is passed over.
    #[must_use]
    pub fn new(store: &'a RetailDatStore, detail: u32) -> Self {
        Self {
            store,
            keep_high_detail: store.highres_granted() && detail == 0,
        }
    }

    /// The dat set the record `id` comes from, which decides its layout and how a 256-colour
    /// image indexes its palette.
    #[must_use]
    pub fn era_of(&self, id: DataId) -> ContainerEra {
        self.store.era_of(id)
    }

    /// Whether a two-level chain retains its high-resolution level.
    #[must_use]
    pub fn keeps_high_detail(&self) -> bool {
        self.keep_high_detail
    }

    /// Read one palette record without expanding its colours.
    ///
    /// # Errors
    /// A missing or malformed palette record.
    pub fn palette(&self, id: DataId) -> Result<Palette, LookupError> {
        self.decode(DbType::Palette, id)
    }

    /// Follow the id chain to the `RenderSurface` that actually carries pixels.
    ///
    /// # Errors
    /// [`LookupError`] when a hop is missing or will not decode.
    pub fn resolve(&self, id: DataId) -> Result<(DataId, RenderSurface, Vec<u8>), LookupError> {
        self.resolve_within(id, MAX_HOPS)
    }

    fn resolve_within(
        &self,
        id: DataId,
        hops: u32,
    ) -> Result<(DataId, RenderSurface, Vec<u8>), LookupError> {
        let kind = divine_type(id).ok_or(LookupError::NotATexture(id))?;
        let hops = hops.checked_sub(1).ok_or(LookupError::NotATexture(id))?;
        match kind {
            DbType::Surface => {
                let s: Surface = self.decode(DbType::Surface, id)?;
                let next = s.orig_texture_id.ok_or(LookupError::NotATexture(id))?;
                self.resolve_within(next, hops)
            }
            // Before Throne of Destiny an image texture carries its own pixels.
            DbType::SurfaceTexture if self.store.era_of(id) == ContainerEra::Classic => {
                let bytes = self.read(DbType::SurfaceTexture, id)?;
                let rs = RenderSurface::from_classic_texture(id, &bytes)
                    .map_err(|e| LookupError::Asset(id, e))?;
                Ok((id, rs, bytes))
            }
            DbType::SurfaceTexture => {
                let t: SurfaceTexture = self.decode(DbType::SurfaceTexture, id)?;
                // one level is that level; two levels are
                // `[1]` (the original) when high detail should be dropped and `[0]`
                // (the high-res partition) otherwise; any other count is the data error
                // "Cannot get surface DID, no source levels are listed!" and `INVALID_DID`.
                let next = match t.source_levels.as_slice() {
                    [one] => *one,
                    [high, original] if self.keep_high_detail => {
                        // The high-res level only where it keeps the original's pixel format, so a
                        // palette or alpha the original has is never lost; the original where not.
                        let low = self.resolve_within(*original, hops);
                        return match (self.resolve_within(*high, hops), low) {
                            (Ok(h), Ok(l)) => Ok(if h.1.format == l.1.format { h } else { l }),
                            (Ok(h), Err(_)) => Ok(h),
                            (Err(_), low) => low,
                        };
                    }
                    [_, original] => *original,
                    _ => return Err(LookupError::NotATexture(id)),
                };
                self.resolve_within(next, hops)
            }
            DbType::RenderTexture => {
                let t: RenderTexture = self.decode(DbType::RenderTexture, id)?;
                let next = *t
                    .source_levels
                    .first()
                    .ok_or(LookupError::NotATexture(id))?;
                self.resolve_within(next, hops)
            }
            DbType::RenderSurface => {
                let bytes = self.read(DbType::RenderSurface, id)?;
                let rs = RenderSurface::decode_payload_in(self.store.era_of(id), id, &bytes)
                    .map_err(|e| LookupError::Asset(id, e))?;
                Ok((id, rs, bytes))
            }
            _ => Err(LookupError::NotATexture(id)),
        }
    }

    /// The palette a loaded surface record names: the palette its picture carries when the
    /// picture is a palette-indexed image whose palette reads, otherwise the palette the record
    /// itself names. A solid-color surface, or a record that names none, has none.
    ///
    /// Loading puts the picture's own palette in place of the record's, so a surface whose
    /// record names no palette over an indexed picture still has one. This is the palette an
    /// object's part is compared against when an appearance description recolors it.
    #[must_use]
    pub fn surface_palette(&self, surface: DataId) -> Option<DataId> {
        let s: Surface = self.decode(DbType::Surface, surface).ok()?;
        if s.surface_type & 6 == 0 {
            return None;
        }
        let named = s.orig_palette_id.filter(|p| p.raw() != 0);
        let picture = s
            .orig_texture_id
            .filter(|t| divine_type(*t) == Some(DbType::SurfaceTexture));
        let own = picture
            .and_then(|t| self.resolve(t).ok())
            .and_then(|(_, rs, _)| rs.default_palette_id)
            .filter(|p| p.raw() != 0 && self.palette(*p).is_ok());
        own.or(named)
    }

    fn read(&self, kind: DbType, id: DataId) -> Result<Vec<u8>, LookupError> {
        self.store
            .read_typed(kind, id)
            .map_err(|e| LookupError::Dat(id, e))
    }

    fn decode<T: Decode>(&self, kind: DbType, id: DataId) -> Result<T, LookupError> {
        let bytes = self.read(kind, id)?;
        T::decode_payload_in(self.store.era_of(id), id, &bytes)
            .map_err(|e| LookupError::Asset(id, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_dat::{DatFile, DatWriter, MemoryFile};
    use std::path::PathBuf;

    /// A store whose portal file holds `records` and whose other files are empty.
    fn store_with(records: &[(u32, Vec<u8>)]) -> RetailDatStore {
        let file = |name: &str, block: u32, set: u32, records: &[(u32, Vec<u8>)]| {
            let storage = MemoryFile::default();
            let mut w = DatWriter::create_in(
                PathBuf::from(name),
                Box::new(storage.clone()),
                block,
                set,
                0,
                0x400 + block * 64,
            )
            .expect("create");
            for (id, bytes) in records {
                w.save(DataId(*id), bytes, 1, 1, 1).expect("save");
            }
            drop(w);
            DatFile::from_storage(PathBuf::from(name), Box::new(storage)).expect("open")
        };
        RetailDatStore::open_with(
            file("client_portal.dat", 0x400, 1, records),
            file("client_cell_1.dat", 0x100, 2, &[]),
            file("client_local_English.dat", 0x400, 3, &[]),
            None,
        )
    }

    /// A surface that names itself as its picture is refused, not followed until the stack ends.
    #[test]
    fn a_texture_chain_that_loops_is_refused() {
        let mut surface = Vec::new();
        for v in [2u32, 0x0800_0001, 0] {
            surface.extend_from_slice(&v.to_le_bytes());
        }
        for f in [0.0f32, 0.0, 1.0] {
            surface.extend_from_slice(&f.to_le_bytes());
        }
        let store = store_with(&[(0x0800_0001, surface)]);
        let lookup = TextureLookup::new(&store, 1);
        assert!(lookup.resolve(DataId(0x0800_0001)).is_err());
    }
}
