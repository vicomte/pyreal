//! An art pack packed into one file: its manifest, every file its models are read from, and every
//! picture made as the pack opens, ready for drawing, so a packed pack opens without decoding or
//! compressing a picture. [`pack`] packs one; [`PackedFile`] reads one.
//!
//! **The file** is a container in the later layout (`docs/formats/01-dat-container.md`), its
//! blocks [`BLOCK`] bytes as written (any size its header names as read; earlier packs took
//! 16 KiB) and its data subset [`SUBSET`]. Record 1 is its contents; every other record, numbered
//! from 2 in the contents' order, is one of the pack's files, byte for byte, or the pictures made
//! from them, and one identical to an earlier record is not written again but takes its id.
//! Numbers are little-endian, and a string is a 32-bit length and its UTF-8.
//!
//! **Layouts.** In layout 1 ([`PLAIN`]) a record is stored as it is. In layout 2 ([`COMPRESSED`])
//! a record is stored as a zstd frame, or as it is where the frame would be no shorter; the
//! contents are never compressed. Both are read; [`pack`] writes either.
//!
//! **The contents**: `DAPK`, the layout, the manifest format the pack was packed for
//! ([`pyreal_assets::art_pack::FORMAT`]), the pack's name, and its hash: the hash of the folder it
//! was packed from, which names the pack in reports however it is opened. Then a count and, for
//! each record, its kind (0 a file, 1 pictures), its key, its id (0 for an empty file, which has
//! no record), its length, in layout 2 its length as stored, and the SHA-256 of its bytes; then
//! the SHA-256 of all that.
//!
//! **A file** is keyed by its path in the pack: `manifest.toml`, `tree/lod0.gltf`,
//! `tree/tree.bin`. **Pictures** are keyed by how they are made, a `:` between the parts:
//!
//! - a model's picture: `model`, `color` or `data`, the alpha it is cut below or `-`, `views` for a
//!   level of pictures of its model seen from round it or `-`, and last its path
//!   (`model:color:0.5:-:tree/bark.png`), so a path holding a `:` is still told apart;
//! - a ground material's three pictures: `ground` and the paths of its base color, normal,
//!   height, roughness and occlusion, empty where it names none;
//! - a material's three, or two where it keeps the retail picture: `material`, the paths of its
//!   base color (empty for the two), normal, roughness, occlusion and height, its normal scale,
//!   how damp it is and the path of its damp map where it names one;
//! - a grit picture: `grit` and its path;
//! - a material's metal map: `metal` and its path;
//! - a ground's detail grain: `detail` and its path;
//! - a moon's picture: `moon` and its path;
//! - a model's card's two, colour and normal: `card`, the cards' make ([`super::cards::MAKE`]) and
//!   the model's id (`card:1:0x02000001`). A pack holding no card of this make has its cards
//!   drawn as it opens.
//!
//! No path a manifest names holds a `:`. A pictures record holds a count, then each picture: its
//! width and height, whether it holds color (1) or data (0), its blocks (0 for four bytes a
//! texel, 4 for BC4, 5 for BC5, 7 for BC7) and its level count, then each level, largest first,
//! as its length and its bytes.
//!
//! **Refusals.** A file that is not a packed pack, that was packed in another layout or for
//! another manifest format, or whose contents are damaged, is refused as it opens. A record that
//! will not read, or whose bytes are not those it was packed with, refuses the whole pack
//! ([`PackedFile::damage`]). A file or picture the contents do not hold is one the pack could not
//! read or make when it was packed, and is left out as it was from the folder.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use pyreal_assets::art_pack::{ManifestGround, ManifestMaterial, FORMAT};
use pyreal_dat::overlay::record_hash;
use pyreal_dat::{DatFile, DatStorage, DatStorageMut, DatWriter, MemoryFile};
use pyreal_primitives::DataId;

use super::images::{BlockFormat, Image};
use super::{ArtPack, Pictures, Recording};

/// The layout whose records are stored as they are.
pub const PLAIN: u32 = 1;

/// The layout whose records are stored as zstd frames where that is shorter.
pub const COMPRESSED: u32 = 2;

/// The zstd level a compressed pack is written at: the slowest to write, the same to read.
const ZSTD_LEVEL: i32 = 19;

/// The container's block size as written, which its header records: small, so the part-filled
/// last block of each record wastes little.
pub const BLOCK: u32 = 0x400;

/// The data subset a packed pack's header carries: `Pack`.
pub const SUBSET: u32 = u32::from_le_bytes(*b"Pack");

/// The contents' magic: `DAPK`.
const MAGIC: u32 = u32::from_le_bytes(*b"DAPK");

/// The contents' record.
const CONTENTS: DataId = DataId(1);

/// The first record after the contents.
const FIRST: u32 = 2;

/// The largest packed pack: a container's blocks lie below 2 GiB, where the high bit of a link
/// marks a free block. A limit of the format, which the base pack is under like any pack; a pack a
/// player adds is also refused past it ([`added_pack_problem`]).
pub const LARGEST: u64 = 0x8000_0000;

/// Why the file at `path` will not do as a pack a player adds: it is larger than any packed pack
/// can be ([`LARGEST`]). `None` when it is not, or cannot be measured.
#[must_use]
pub fn added_pack_problem(path: &Path) -> Option<String> {
    let bytes = std::fs::metadata(path).ok()?.len();
    (bytes > LARGEST).then(|| {
        format!(
            "{} is {} MB, and an art pack holds at most {} MB",
            path.display(),
            bytes >> 20,
            LARGEST >> 20
        )
    })
}

/// The blocks the writer keeps free beyond what the record it writes needs.
const RESERVE: u64 = 0x34;

/// What a record of a packed pack holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// One of the pack's files, byte for byte.
    File,
    /// Pictures made from the pack's files.
    Pictures,
}

/// One record a packed pack's contents name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// What it holds.
    pub kind: Kind,
    /// A file's path, or how its pictures are made.
    pub key: String,
    /// Its id in the container, shared with an identical record; 0 for an empty file, which has
    /// no record.
    pub id: DataId,
    /// Its length.
    pub size: u32,
    /// Its length as stored: `size` where it is stored as it is, shorter for a zstd frame.
    pub stored: u32,
    /// The SHA-256 of its bytes.
    pub sha256: [u8; 32],
}

/// A packed pack's contents: what the pack is and every record it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents {
    /// Its layout: [`PLAIN`] or [`COMPRESSED`].
    pub layout: u32,
    /// The manifest format the pack was packed for.
    pub format: u32,
    /// The pack's name.
    pub name: String,
    /// The hash of the folder it was packed from.
    pub hash: String,
    /// Its records, in the order their ids were given.
    pub records: Vec<Record>,
}

fn put(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_text(out: &mut Vec<u8>, s: &str) {
    put(out, u32::try_from(s.len()).unwrap_or(u32::MAX));
    out.extend_from_slice(s.as_bytes());
}

/// Bytes read from the front.
struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (front, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(front)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn text(&mut self) -> Option<String> {
        let n = usize::try_from(self.u32()?).ok()?;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

impl Contents {
    /// The contents' record.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put(&mut out, MAGIC);
        put(&mut out, self.layout);
        put(&mut out, self.format);
        put_text(&mut out, &self.name);
        put_text(&mut out, &self.hash);
        put(
            &mut out,
            u32::try_from(self.records.len()).unwrap_or(u32::MAX),
        );
        for r in &self.records {
            out.push(match r.kind {
                Kind::File => 0,
                Kind::Pictures => 1,
            });
            put_text(&mut out, &r.key);
            put(&mut out, r.id.raw());
            put(&mut out, r.size);
            if self.layout == COMPRESSED {
                put(&mut out, r.stored);
            }
            out.extend_from_slice(&r.sha256);
        }
        let sum = record_hash(&out);
        out.extend_from_slice(&sum);
        out
    }

    /// The contents' record, read: the reason, as a sentence, when it is not a packed pack's, or
    /// one of another layout or manifest format, or damaged.
    ///
    /// # Errors
    /// As above.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut b = Bytes(bytes);
        if b.u32() != Some(MAGIC) {
            return Err("not a packed art pack".into());
        }
        let layout = match b.u32() {
            Some(n @ (PLAIN | COMPRESSED)) => n,
            Some(n) => {
                return Err(format!(
                    "packed in layout {n}, and this client reads layouts {PLAIN} and \
                     {COMPRESSED}; pack it again"
                ))
            }
            None => return Err("its contents are damaged; pack it again".into()),
        };
        let damaged = || "its contents are damaged; pack it again".to_owned();
        let (body, sum) = bytes.split_at(bytes.len().checked_sub(32).ok_or_else(damaged)?);
        if record_hash(body)[..] != sum[..] {
            return Err(damaged());
        }
        let mut b = Bytes(body.get(8..).ok_or_else(damaged)?);
        let format = b.u32().ok_or_else(damaged)?;
        if i64::from(format) != FORMAT {
            return Err(format!(
                "packed from manifest format {format}, and this client reads format {FORMAT}; \
                 pack it again"
            ));
        }
        let read = |b: &mut Bytes<'_>| -> Option<Self> {
            let name = b.text()?;
            let hash = b.text()?;
            let count = b.u32()?;
            let mut records = Vec::new();
            for _ in 0..count {
                let kind = match b.take(1)?[0] {
                    0 => Kind::File,
                    1 => Kind::Pictures,
                    _ => return None,
                };
                let key = b.text()?;
                let id = DataId(b.u32()?);
                let size = b.u32()?;
                let stored = if layout == COMPRESSED { b.u32()? } else { size };
                let sha256 = b.take(32)?.try_into().ok()?;
                records.push(Record {
                    kind,
                    key,
                    id,
                    size,
                    stored,
                    sha256,
                });
            }
            b.0.is_empty().then_some(Self {
                layout,
                format,
                name,
                hash,
                records,
            })
        };
        read(&mut b).ok_or_else(damaged)
    }
}

/// What a packed pack keeps a model's picture under: how it is made, its path last.
#[must_use]
pub fn model_key(path: &str, srgb: bool, cutoff: Option<f32>, views: bool) -> String {
    format!(
        "model:{}:{}:{}:{path}",
        if srgb { "color" } else { "data" },
        cutoff.map_or_else(|| "-".to_owned(), |c| c.to_string()),
        if views { "views" } else { "-" },
    )
}

/// What a packed pack keeps a ground material's three pictures under: the paths of the pictures
/// they are made from. A flat ground and a steep one made from the same pictures share them.
#[must_use]
pub fn ground_key(entry: &ManifestGround) -> String {
    let path = |p: &Option<String>| p.clone().unwrap_or_default();
    format!(
        "ground:{}:{}:{}:{}:{}",
        entry.base_color,
        path(&entry.normal),
        path(&entry.height),
        path(&entry.roughness),
        path(&entry.occlusion),
    )
}

/// What a packed pack keeps a material's three pictures under, or two where it keeps the retail
/// picture: the paths of the pictures they are made from (no base color for the two), the normal
/// scale its normal picture is bent by and how damp it is.
#[must_use]
pub fn material_key(entry: &ManifestMaterial) -> String {
    let path = |p: &Option<String>| p.clone().unwrap_or_default();
    let base = if entry.keep_retail_picture {
        ""
    } else {
        entry.base_color.as_str()
    };
    let key = format!(
        "material:{base}:{}:{}:{}:{}:{}:{}",
        path(&entry.normal),
        path(&entry.roughness),
        path(&entry.occlusion),
        path(&entry.height),
        entry.normal_scale,
        entry.damp,
    );
    match &entry.damp_map {
        Some(map) => format!("{key}:{map}"),
        None => key,
    }
}

/// What a packed pack keeps a material's metal map under, whichever materials share it.
#[must_use]
pub fn metal_key(path: &str) -> String {
    format!("metal:{path}")
}

/// What a packed pack keeps a grit picture under, whichever materials share it.
#[must_use]
pub fn grit_key(path: &str) -> String {
    format!("grit:{path}")
}

/// What a packed pack keeps a ground's detail grain under, whichever grounds share it.
#[must_use]
pub fn detail_key(path: &str) -> String {
    format!("detail:{path}")
}

/// What a packed pack keeps a moon's picture under.
#[must_use]
pub fn moon_key(path: &str) -> String {
    format!("moon:{path}")
}

/// What a packed pack keeps a model's card under: the cards' make and the model's id.
#[must_use]
pub fn card_key(id: DataId) -> String {
    format!("card:{}:{:#010X}", super::cards::MAKE, id.raw())
}

/// The number a pictures record gives a picture's blocks.
fn blocks_code(format: Option<BlockFormat>) -> u32 {
    match format {
        None => 0,
        Some(BlockFormat::Bc4) => 4,
        Some(BlockFormat::Bc5) => 5,
        Some(BlockFormat::Bc7) => 7,
    }
}

/// The length of the pictures record holding `pictures`.
fn pictures_len(pictures: &[Image]) -> usize {
    4 + pictures
        .iter()
        .map(|p| 20 + p.levels.iter().map(|l| 4 + l.len()).sum::<usize>())
        .sum::<usize>()
}

/// The pictures record holding `pictures`.
fn encode_pictures(pictures: &[Image]) -> Vec<u8> {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(pictures_len(pictures));
    put(&mut out, count(pictures.len()));
    for p in pictures {
        put(&mut out, p.width);
        put(&mut out, p.height);
        put(&mut out, u32::from(p.srgb));
        put(&mut out, blocks_code(p.format));
        put(&mut out, count(p.levels.len()));
        for level in &p.levels {
            put(&mut out, count(level.len()));
            out.extend_from_slice(level);
        }
    }
    out
}

/// The pictures a pictures record holds; `None` for bytes that are not one, or whose levels are
/// not the lengths their sizes and blocks make.
fn decode_pictures(bytes: &[u8]) -> Option<Vec<Image>> {
    let mut b = Bytes(bytes);
    let count = b.u32()?;
    let mut pictures = Vec::new();
    for _ in 0..count {
        let (width, height) = (b.u32()?, b.u32()?);
        let srgb = match b.u32()? {
            0 => false,
            1 => true,
            _ => return None,
        };
        let format = match b.u32()? {
            0 => None,
            4 => Some(BlockFormat::Bc4),
            5 => Some(BlockFormat::Bc5),
            7 => Some(BlockFormat::Bc7),
            _ => return None,
        };
        let levels = b.u32()?;
        // A level's length is a u32, so a picture of more texels than that holds none.
        let texels = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || levels == 0 || levels > 32 || texels > u64::from(u32::MAX) {
            return None;
        }
        let mut made = Vec::new();
        for k in 0..levels {
            let (w, h) = ((width >> k).max(1), (height >> k).max(1));
            let want = match format {
                None => (w as usize) * (h as usize) * 4,
                Some(f) => f.len(w, h),
            };
            let len = usize::try_from(b.u32()?).ok()?;
            if len != want {
                return None;
            }
            made.push(std::sync::Arc::new(b.take(len)?.to_vec()));
        }
        pictures.push(Image {
            width,
            height,
            srgb,
            levels: made,
            format,
        });
    }
    b.0.is_empty().then_some(pictures)
}

/// A packed pack, open: its contents read and checked, its records read as they are asked for.
#[derive(Debug)]
pub struct PackedFile {
    name: String,
    file: DatFile,
    contents: Contents,
    files: HashMap<String, usize>,
    pictures: HashMap<String, usize>,
    damage: Mutex<Option<String>>,
}

impl PackedFile {
    /// The packed pack in the file at `path`.
    ///
    /// # Errors
    /// The file will not open, is not a packed pack, was packed in another layout or for another
    /// manifest format, or its contents are damaged or name a record it does not hold.
    pub fn open(path: &Path) -> Result<Self, String> {
        let name = path.display().to_string();
        let file = DatFile::open(path).map_err(|e| match e {
            pyreal_dat::DatError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                format!("{name}: no such folder or file")
            }
            e => format!("{name}: not a packed art pack ({e})"),
        })?;
        Self::from_file(name, file)
    }

    /// The packed pack kept in `storage`, `name` naming it in messages.
    ///
    /// # Errors
    /// As [`Self::open`].
    pub fn from_storage(name: &str, storage: Box<dyn DatStorage>) -> Result<Self, String> {
        let file = DatFile::from_storage(PathBuf::from(name), storage)
            .map_err(|e| format!("{name}: not a packed art pack ({e})"))?;
        Self::from_file(name.to_owned(), file)
    }

    fn from_file(name: String, file: DatFile) -> Result<Self, String> {
        if file.header().data_subset != SUBSET {
            return Err(format!("{name}: not a packed art pack"));
        }
        let contents = match file.read(CONTENTS) {
            Ok(bytes) => Contents::decode(&bytes).map_err(|why| format!("{name}: {why}"))?,
            Err(pyreal_dat::DatError::NotFound(_)) => {
                return Err(format!("{name}: not a packed art pack"))
            }
            Err(e) => {
                return Err(format!(
                    "{name} is damaged: its contents will not read ({e}); pack it again"
                ))
            }
        };
        let mut files = HashMap::new();
        let mut pictures = HashMap::new();
        for (i, r) in contents.records.iter().enumerate() {
            let held = r.size == 0 || file.entry(r.id).is_some_and(|e| e.size == r.stored);
            if !held {
                return Err(format!(
                    "{name} is damaged: {} is not in it; pack it again",
                    r.key
                ));
            }
            match r.kind {
                Kind::File => files.insert(r.key.clone(), i),
                Kind::Pictures => pictures.insert(r.key.clone(), i),
            };
        }
        Ok(Self {
            name,
            file,
            contents,
            files,
            pictures,
            damage: Mutex::new(None),
        })
    }

    /// The file's name, for messages.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the pack is and every record it holds.
    #[must_use]
    pub fn contents(&self) -> &Contents {
        &self.contents
    }

    /// The hash of the folder the pack was packed from.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.contents.hash
    }

    /// Why the pack is damaged, once a record read from it would not read or was not the bytes
    /// it was packed with: the first such, as a sentence. Nothing of a damaged pack is drawn.
    #[must_use]
    pub fn damage(&self) -> Option<String> {
        self.damage
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The pack's file at `path`.
    ///
    /// # Errors
    /// The pack holds no such file, or it is damaged ([`Self::damage`]).
    pub fn file(&self, path: &str) -> Result<Vec<u8>, String> {
        let at = self.files.get(path).copied();
        self.record(at)?
            .ok_or_else(|| format!("{path}: not in {}", self.name))
    }

    /// The picture kept under `key` ([`model_key`]).
    ///
    /// # Errors
    /// As [`Self::three`].
    pub fn picture(&self, key: &str) -> Result<Image, String> {
        let [picture] = self.pictures(key)?;
        Ok(picture)
    }

    /// The three pictures kept under `key` ([`ground_key`], [`material_key`]).
    ///
    /// # Errors
    /// The pack holds no such pictures, or it is damaged ([`Self::damage`]).
    pub fn three(&self, key: &str) -> Result<[Image; 3], String> {
        self.pictures(key)
    }

    /// The two pictures of a material that keeps the retail picture ([`material_key`]).
    ///
    /// # Errors
    /// The pack holds no such pictures, or it is damaged ([`Self::damage`]).
    pub fn two(&self, key: &str) -> Result<[Image; 2], String> {
        self.pictures(key)
    }

    fn pictures<const N: usize>(&self, key: &str) -> Result<[Image; N], String> {
        let at = self.pictures.get(key).copied();
        let bytes = self
            .record(at)?
            .ok_or_else(|| format!("{key}: not in {}", self.name))?;
        decode_pictures(&bytes)
            .and_then(|p| <[Image; N]>::try_from(p).ok())
            .ok_or_else(|| self.damaged(format!("{key} does not hold {N} pictures")))
    }

    /// The bytes of the record at `at` in the contents, checked against its hash; `None` for no
    /// record.
    fn record(&self, at: Option<usize>) -> Result<Option<Vec<u8>>, String> {
        let Some(r) = at.and_then(|i| self.contents.records.get(i)) else {
            return Ok(None);
        };
        if r.size == 0 {
            return Ok(Some(Vec::new()));
        }
        let stored = self
            .file
            .read(r.id)
            .map_err(|e| self.damaged(format!("{} will not read ({e})", r.key)))?;
        let not_packed = || self.damaged(format!("{} is not what was packed", r.key));
        if stored.len() != r.stored as usize {
            return Err(not_packed());
        }
        let bytes = if r.stored == r.size {
            stored
        } else {
            zstd::bulk::decompress(&stored, r.size as usize).map_err(|_| not_packed())?
        };
        if bytes.len() != r.size as usize || record_hash(&bytes) != r.sha256 {
            return Err(self.damaged(format!("{} is not what was packed", r.key)));
        }
        Ok(Some(bytes))
    }

    /// Note the pack as damaged, `why` the record that is; the sentence a refusal says.
    fn damaged(&self, why: String) -> String {
        let e = format!("{} is damaged: {why}; pack it again", self.name);
        let mut damage = self.damage.lock().unwrap_or_else(PoisonError::into_inner);
        if damage.is_none() {
            *damage = Some(e.clone());
        }
        e
    }
}

/// What [`pack`] made.
#[derive(Debug)]
pub struct Packed {
    /// The pack as it opened from its files, with the problems it met there: what it left out is
    /// left out of the packed file too.
    pub pack: ArtPack,
    /// The packed file.
    pub bytes: Vec<u8>,
    /// Its records, the contents among them.
    pub records: usize,
}

/// The pack whose manifest is `manifest`, every file it names read through `read` by its path
/// relative to the pack, packed into one file: opened as [`ArtPack::open`] opens it, with every
/// file it read for itself and every picture it made. The same files always give the same bytes:
/// the records are written in their keys' order, with no date, into a container exactly as large
/// as they need, each identical record written once.
///
/// # Errors
/// The pack will not open ([`ArtPack::open`]'s errors), or is too large for one file.
pub fn pack(
    manifest: &str,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    layout: u32,
) -> Result<Packed, String> {
    pack_in(manifest, read, BLOCK, layout)
}

/// [`pack`], into a container of `block`-byte blocks.
fn pack_in(
    manifest: &str,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    block: u32,
    layout: u32,
) -> Result<Packed, String> {
    if ![PLAIN, COMPRESSED].contains(&layout) {
        return Err(format!("no layout {layout}: {PLAIN} or {COMPRESSED}"));
    }
    let mut recording = Recording::default();
    let pack = ArtPack::open_from(manifest, read, Pictures::Made, Some(&mut recording), None)?;
    let Recording { files, pictures } = recording;
    let too_large = |what: &str| format!("{what} is larger than one record holds");
    let mut contents = Contents {
        layout,
        format: u32::try_from(FORMAT).unwrap_or(u32::MAX),
        name: pack.name.clone(),
        hash: pack.hash.clone(),
        records: Vec::with_capacity(files.len() + pictures.len()),
    };
    let listed = files
        .iter()
        .map(|(key, bytes)| (Kind::File, key, Cow::Borrowed(&bytes[..])))
        .chain(
            pictures
                .iter()
                .map(|(key, made)| (Kind::Pictures, key, Cow::Owned(encode_pictures(made)))),
        );
    // Hashed as listed, so a record identical to an earlier one takes its id.
    let mut next = FIRST;
    let mut ids: HashMap<(u32, [u8; 32]), DataId> = HashMap::new();
    let mut first = Vec::with_capacity(contents.records.capacity());
    for (kind, key, bytes) in listed {
        let size = u32::try_from(bytes.len()).map_err(|_| too_large(key))?;
        let sha256 = record_hash(&bytes);
        let mut fresh = false;
        let id = if size == 0 {
            DataId(0)
        } else {
            *ids.entry((size, sha256)).or_insert_with(|| {
                fresh = true;
                next += 1;
                DataId(next - 1)
            })
        };
        first.push(fresh);
        contents.records.push(Record {
            kind,
            key: key.clone(),
            id,
            size,
            stored: size,
            sha256,
        });
    }
    let bytes_of = |r: &Record| -> Cow<'_, [u8]> {
        match r.kind {
            Kind::File => Cow::Borrowed(&files[&r.key]),
            Kind::Pictures => Cow::Owned(encode_pictures(&pictures[&r.key])),
        }
    };
    // Each record written once as it is stored, compressed side by side in layout 2.
    let fresh: Vec<&Record> = contents
        .records
        .iter()
        .zip(&first)
        .filter(|(_, first)| **first)
        .map(|(r, _)| r)
        .collect();
    let stored: Vec<(DataId, Vec<u8>)> = super::images::par_map(fresh, &|r: &Record| {
        let bytes = bytes_of(r);
        let frame = (layout == COMPRESSED)
            .then(|| zstd::bulk::compress(&bytes, ZSTD_LEVEL).ok())
            .flatten()
            .filter(|f| f.len() < bytes.len());
        (r.id, frame.unwrap_or_else(|| bytes.into_owned()))
    });
    let lengths: HashMap<DataId, u32> = stored
        .iter()
        .map(|(id, b)| (*id, u32::try_from(b.len()).unwrap_or(u32::MAX)))
        .collect();
    for r in &mut contents.records {
        if let Some(&n) = lengths.get(&r.id) {
            r.stored = n;
        }
    }
    // The container exactly as large as the records need: a chain each, the contents', the
    // directory's nodes as it grows, and the room the writer keeps in hand.
    let per = u64::from(block - 4);
    let blocks = |len: usize| (len as u64).div_ceil(per);
    let records = (next - FIRST) as usize + 1;
    let nodes = blocks(pyreal_dat::btree::NODE_SIZE) * (1 + 2 * (records as u64).div_ceil(30));
    let total = contents
        .records
        .iter()
        .zip(&first)
        .filter(|(_, first)| **first)
        .map(|(r, _)| blocks(r.stored as usize))
        .sum::<u64>()
        + blocks(contents.encode().len())
        + nodes
        + RESERVE
        + 2;
    let size = 0x400 + u64::from(block) * total;
    if size > LARGEST {
        return Err(format!(
            "the pack takes {} MB packed, and one file holds {} MB",
            size >> 20,
            LARGEST >> 20
        ));
    }
    let size = u32::try_from(size).map_err(|_| too_large("the pack"))?;
    let storage = MemoryFile::default();
    // Its length set once, before the writer fills it.
    storage
        .clone()
        .write_all_at(u64::from(size) - 1, &[0])
        .map_err(|e| e.to_string())?;
    let fail = |e: pyreal_dat::DatError| e.to_string();
    let mut w = DatWriter::create_in(
        PathBuf::from(&pack.name),
        Box::new(storage.clone()),
        block,
        pyreal_dat::container::PORTAL_DATFILE,
        SUBSET,
        size,
    )
    .map_err(fail)?;
    for (id, bytes) in &stored {
        w.save(*id, bytes, 1, 0, 0)
            .map_err(|e| format!("{id:?}: {e}"))?;
    }
    w.save(CONTENTS, &contents.encode(), 1, 0, 0)
        .map_err(fail)?;
    // Grown past what was reckoned, it may not reach past what one file holds either.
    if u64::from(w.header().file_size) > LARGEST {
        return Err(format!(
            "the pack takes {} MB packed, and one file holds {} MB",
            w.header().file_size >> 20,
            LARGEST >> 20
        ));
    }
    drop(w);
    Ok(Packed {
        pack,
        bytes: storage.bytes(),
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pack a player adds is refused past the most a packed pack can be, with its size and the
    /// limit in the message; one at the limit is not.
    #[test]
    fn an_added_pack_past_the_largest_is_refused() {
        let path =
            std::env::temp_dir().join(format!("pyreal-added-pack-{}.dat", std::process::id()));
        let f = std::fs::File::create(&path).expect("made");
        f.set_len(LARGEST).expect("sized");
        assert_eq!(added_pack_problem(&path), None);
        f.set_len(LARGEST + (1 << 20)).expect("sized");
        let problem = added_pack_problem(&path);
        drop(f);
        let _ = std::fs::remove_file(&path);
        let problem = problem.expect("refused");
        assert!(
            problem.contains("2049 MB") && problem.contains("at most 2048 MB"),
            "{problem}"
        );
        assert_eq!(
            added_pack_problem(&path),
            None,
            "a file not there is not measured"
        );
    }

    /// A picture too large to count its bytes is refused, not overflowed.
    #[test]
    fn a_picture_too_large_to_count_is_refused() {
        let mut b = Vec::new();
        for v in [1u32, 0x8000_0000, 0x8000_0000, 1, 0, 1, 0] {
            put(&mut b, v);
        }
        let r = std::panic::catch_unwind(|| decode_pictures(&b).is_none());
        assert_eq!(r.ok(), Some(true));
    }

    fn png_of(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            w.write_image_data(rgba).expect("data");
        }
        bytes
    }

    /// `w` by `h` texels climbing from `from`, so no two pictures made from them are alike.
    fn ramp(w: u32, h: u32, from: [u8; 4]) -> Vec<u8> {
        (0..w * h)
            .flat_map(|k| {
                let k = u8::try_from(k % 64).expect("a step");
                [
                    from[0].wrapping_add(k),
                    from[1].wrapping_add(k / 2),
                    from[2],
                    from[3].saturating_sub(k % 3),
                ]
            })
            .collect()
    }

    /// A model of one triangle whose material takes `picture` as its base color and its normal
    /// both, cut out or opaque (`alpha`), beside an empty buffer it names and never reads from.
    fn triangle(picture: &str, alpha: &str) -> String {
        format!(
            r#"{{"asset": {{"version": "2.0"}}, "scenes": [{{"nodes": [0]}}],
            "nodes": [{{"mesh": 0}}],
            "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}, "material": 0}}]}}],
            "materials": [{{"alphaMode": "{alpha}",
                "pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}}},
                "normalTexture": {{"index": 0}}}}],
            "textures": [{{"source": 0}}], "images": [{{"uri": "{picture}"}}],
            "buffers": [{{"uri": "tri.bin", "byteLength": 36}},
                {{"uri": "empty.bin", "byteLength": 0}}],
            "bufferViews": [{{"buffer": 0, "byteLength": 36}}],
            "accessors": [{{"bufferView": 0, "componentType": 5126, "count": 3,
                "type": "VEC3"}}]}}"#
        )
    }

    /// A pack with a bit of everything: a tree whose pictures are each drawn both as color and as
    /// data, cut out and opaque, with a level of pictures of it seen from round it; a model that
    /// will not read; grounds, a steep one made from the same pictures as a flat one and another
    /// from all but its normal; and materials, two from the same pictures bent by different normal
    /// scales, and one that will not read.
    const MANIFEST: &str = r#"
[pack]
name = "Packed"
format = 1
[[model]]
id = "0x02000258"
foliage = true
author = "a"
tool = "t"
license = "CC0-1.0"
[[model.lod]]
file = "tree/lod0.gltf"
distance = 30
[[model.lod]]
file = "tree/lod1.gltf"
distance = 90
[[model.lod]]
file = "tree/far.gltf"
views = 2
[[model]]
id = "0x020005C9"
author = "a"
tool = "t"
license = "CC0-1.0"
[[model.lod]]
file = "rock/missing.gltf"
[[terrain]]
ground = [1, 9]
base_color = "g/a.png"
height = "g/a_h.png"
normal = "g/a_n.png"
size = 4
author = "a"
tool = "t"
license = "CC0-1.0"
[[terrain]]
ground = "road"
base_color = "g/b.png"
roughness = "g/b_r.png"
size = 8
author = "a"
tool = "t"
license = "CC0-1.0"
[[steep]]
ground = 9
base_color = "g/a.png"
height = "g/a_h.png"
normal = "g/a_n.png"
size = 6
author = "a"
tool = "t"
license = "CC0-1.0"
[[steep]]
ground = 1
base_color = "g/a.png"
height = "g/a_h.png"
size = 6
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x0500287C"
base_color = "m/wall.png"
normal = "m/wall_n.png"
normal_scale = 2.0
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05002752"
base_color = "m/wall.png"
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x0500287D"
base_color = "m/wall.png"
normal = "m/wall_n.png"
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05002753"
base_color = "m/fallback.png"
keep_retail_picture = true
normal = "m/wall_n.png"
height = "m/wall.png"
damp = 0.5
damp_map = "m/wall.png"
grit = "m/wall_n.png"
grit_repeat = [3, 4]
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05001CB4"
base_color = "m/missing.png"
author = "a"
tool = "t"
license = "CC0-1.0"
[[moon]]
object = "0x01001F6A"
surface = "m/wall.png"
lights = "m/wall_n.png"
author = "a"
tool = "t"
license = "CC0-1.0"
"#;

    fn files() -> HashMap<String, Vec<u8>> {
        let mut bin = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 3.0, 0.0]] {
            for c in p {
                bin.extend(c.to_le_bytes());
            }
        }
        let mut f = HashMap::new();
        let mut put = |path: &str, bytes: Vec<u8>| f.insert(path.to_owned(), bytes);
        put("tree/lod0.gltf", triangle("bark.png", "MASK").into_bytes());
        put("tree/lod1.gltf", triangle("bark.png", "MASK").into_bytes());
        put("tree/far.gltf", triangle("far.png", "OPAQUE").into_bytes());
        put("tree/tri.bin", bin);
        put("tree/empty.bin", Vec::new());
        put(
            "tree/bark.png",
            png_of(6, 5, &ramp(6, 5, [90, 60, 30, 255])),
        );
        put(
            "tree/far.png",
            png_of(8, 4, &ramp(8, 4, [20, 120, 40, 200])),
        );
        put("g/a.png", png_of(8, 8, &ramp(8, 8, [100, 140, 20, 255])));
        put("g/a_h.png", png_of(8, 8, &ramp(8, 8, [60, 0, 0, 255])));
        put("g/a_n.png", png_of(8, 8, &ramp(8, 8, [110, 150, 255, 255])));
        put("g/b.png", png_of(4, 4, &ramp(4, 4, [10, 20, 30, 255])));
        put("g/b_r.png", png_of(4, 4, &ramp(4, 4, [180, 0, 0, 255])));
        put("m/wall.png", png_of(8, 8, &ramp(8, 8, [120, 90, 60, 255])));
        put(
            "m/wall_n.png",
            png_of(8, 8, &ramp(8, 8, [160, 128, 230, 255])),
        );
        f
    }

    fn reader(
        files: &HashMap<String, Vec<u8>>,
    ) -> impl FnMut(&str) -> Result<Vec<u8>, String> + use<'_> {
        |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        }
    }

    fn packed() -> Packed {
        let files = files();
        let packed = pack(MANIFEST, &mut reader(&files), PLAIN);
        packed.expect("the pack packs")
    }

    fn open(bytes: &[u8]) -> Result<ArtPack, String> {
        let file = PackedFile::from_storage("test.dat", Box::new(bytes.to_vec()))?;
        ArtPack::from_packed(&file)
    }

    #[test]
    fn a_packed_file_keeps_its_materials_sizes_and_reads_their_pictures_as_they_are_drawn() {
        use crate::MaterialPart;
        let files = files();
        let folder = ArtPack::open(MANIFEST, &mut reader(&files)).expect("the folder opens");
        let path =
            std::env::temp_dir().join(format!("pyreal-lazy-materials-{}.dat", std::process::id()));
        std::fs::write(&path, &packed().bytes).expect("written");
        let opened = ArtPack::open_packed(&path);
        let _ = std::fs::remove_file(&path);
        let opened = opened.expect("the packed file opens");
        assert!(opened.material_source.is_some());
        assert_eq!(opened.materials.len(), folder.materials.len());
        for (i, (held, kept)) in opened.materials.iter().zip(&folder.materials).enumerate() {
            // Only the sizes are held; each picture reads back as made.
            assert!(held.normal.levels.is_empty() && held.surface.levels.is_empty());
            assert_eq!(
                (held.normal.width, held.surface.height),
                (kept.normal.width, kept.surface.height)
            );
            for part in [
                MaterialPart::Albedo,
                MaterialPart::Normal,
                MaterialPart::Surface,
            ] {
                assert_eq!(
                    opened.material_picture(i, part),
                    folder.material_picture(i, part),
                    "material {i} {part:?}"
                );
            }
        }
    }

    #[test]
    fn a_compressed_pack_is_smaller_and_opens_to_what_the_plain_one_opens_to() {
        let files = files();
        let plain = packed();
        let compressed =
            pack(MANIFEST, &mut reader(&files), COMPRESSED).expect("the pack packs compressed");
        assert!(compressed.bytes.len() < plain.bytes.len());
        let file = PackedFile::from_storage("test.dat", Box::new(compressed.bytes.clone()))
            .expect("opens");
        assert_eq!(file.contents().layout, COMPRESSED);
        assert!(file.contents().records.iter().any(|r| r.stored < r.size));
        let (a, b) = (
            open(&plain.bytes).expect("opens"),
            open(&compressed.bytes).expect("opens"),
        );
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.images, b.images);
        assert_eq!(a.models, b.models);
        assert_eq!(a.materials, b.materials);
        assert_eq!(a.grounds, b.grounds);
    }

    #[test]
    fn a_packed_pack_opens_to_the_models_and_pictures_its_folder_opens_to() {
        let files = files();
        let folder = ArtPack::open(MANIFEST, &mut reader(&files)).expect("the folder opens");
        // The pack exercises what it is meant to: a picture drawn as color and as data is two
        // pictures, and the far level's two more; four grounds and four materials, one keeping
        // the retail picture, whose fallback colour is not in the folder; and what will not read
        // left out.
        assert_eq!(folder.images.len(), 4);
        assert!(folder.images.iter().all(|i| i.format.is_some()));
        assert!(folder.images[2].srgb && !folder.images[3].srgb);
        assert_eq!(folder.grounds.len(), 4);
        assert_eq!(folder.materials.len(), 4);
        assert_eq!(folder.problems.len(), 2, "{:?}", folder.problems);
        let packed = packed();
        let file =
            PackedFile::from_storage("test.dat", Box::new(packed.bytes.clone())).expect("opens");
        // The packed file holds no picture's file, only the pictures made from them: nothing is
        // decoded or compressed as it opens.
        let records = &file.contents().records;
        assert!(records
            .iter()
            .filter(|r| r.kind == Kind::File)
            .all(|r| !r.key.ends_with(".png")));
        assert!(records.iter().any(|r| r.kind == Kind::Pictures));
        // Two levels' identical files are written once, under one id.
        let id = |key: &str| records.iter().find(|r| r.key == key).map(|r| r.id);
        assert!(id("tree/lod0.gltf").is_some());
        assert_eq!(id("tree/lod0.gltf"), id("tree/lod1.gltf"));
        let ids: std::collections::BTreeSet<_> = records
            .iter()
            .filter(|r| r.size > 0)
            .map(|r| r.id)
            .collect();
        assert_eq!(packed.records, ids.len() + 1);
        assert!(ids.len() < records.iter().filter(|r| r.size > 0).count());
        let opened = ArtPack::from_packed(&file).expect("the packed pack opens");
        assert_eq!(opened.name, folder.name);
        assert_eq!(
            opened.hash, folder.hash,
            "named as the folder it was packed from"
        );
        assert_eq!(opened.models, folder.models);
        assert_eq!(opened.images, folder.images);
        assert_eq!(opened.grounds, folder.grounds);
        assert_eq!(opened.materials, folder.materials);
        // A grit picture is kept once, and its material names it.
        assert_eq!(folder.grits.len(), 1);
        assert_eq!(opened.grits, folder.grits);
        let gritty = folder.materials.iter().find(|m| m.grit.is_some());
        let gritty = gritty.expect("one has grit");
        assert_eq!(gritty.grit_repeat, [3.0, 4.0]);
        // It keeps the retail picture: its two pictures are packed, its fallback colour is not.
        assert!(gritty.albedo.is_none());
        let kept = opened.materials.iter().find(|m| m.grit.is_some());
        assert!(kept.is_some_and(|m| m.albedo.is_none() && m.surface == gritty.surface));
        // A moon's pictures are made as colour, and read back as packed.
        assert_eq!(folder.moons.len(), 1);
        assert!(folder.moons[0].surface.srgb && folder.moons[0].lights.is_some());
        assert_eq!(opened.moons, folder.moons);
        // What the folder left out, the packed pack leaves out too.
        assert_eq!(opened.problems.len(), 2, "{:?}", opened.problems);
        assert!(
            opened.problems[0].contains("0x020005C9"),
            "{:?}",
            opened.problems
        );
        assert!(
            opened.problems[1].contains("0x05001CB4"),
            "{:?}",
            opened.problems
        );
        assert_eq!(packed.pack.models, folder.models);
    }

    /// A kind of ground cover packs the files and pictures its levels name, and opens from the
    /// packed file as from its folder, with how much the wind moves it.
    #[test]
    fn ground_cover_packs_the_files_its_levels_name() {
        let manifest = r#"
[pack]
name = "Meadow"
format = 1
[[ground_cover]]
ground = [1, 9]
density = 0.5
wind = 1.5
author = "a"
tool = "t"
license = "CC0-1.0"
[[ground_cover.lod]]
file = "grass/lod0.gltf"
distance = 30
"#;
        let mut files = files();
        let bin = files["tree/tri.bin"].clone();
        files.insert(
            "grass/lod0.gltf".to_owned(),
            triangle("blade.png", "MASK").into_bytes(),
        );
        files.insert("grass/tri.bin".to_owned(), bin);
        files.insert("grass/empty.bin".to_owned(), Vec::new());
        files.insert(
            "grass/blade.png".to_owned(),
            png_of(4, 4, &ramp(4, 4, [60, 140, 40, 255])),
        );
        let folder = ArtPack::open(manifest, &mut reader(&files)).expect("the folder opens");
        assert_eq!(folder.ground_cover.len(), 1);
        assert!(folder.problems.is_empty(), "{:?}", folder.problems);
        let packed = pack(manifest, &mut reader(&files), PLAIN).expect("the pack packs");
        let file = PackedFile::from_storage("test.dat", Box::new(packed.bytes)).expect("opens");
        let keys: Vec<&str> = file
            .contents()
            .records
            .iter()
            .map(|r| r.key.as_str())
            .collect();
        assert!(keys.contains(&"grass/lod0.gltf"), "{keys:?}");
        assert!(keys.contains(&"grass/tri.bin"), "{keys:?}");
        assert!(
            keys.iter().any(|k| k.ends_with("grass/blade.png")),
            "{keys:?}"
        );
        let opened = ArtPack::from_packed(&file).expect("the packed pack opens");
        assert_eq!(opened.ground_cover, folder.ground_cover);
        assert_eq!(opened.models, folder.models);
        assert_eq!(opened.models[0].wind, 1.5);
    }

    /// A material's metal map is packed once under its own key, beside its material's pictures,
    /// and leaves every other material's record as it was.
    #[test]
    fn a_metal_map_packs_under_its_own_key() {
        let manifest = |metal: &str| {
            format!(
                r#"
[pack]
name = "Iron"
format = 1
[[material]]
texture = "0x05002620"
base_color = "m/fallback.png"
keep_retail_picture = true
normal = "m/wall_n.png"
{metal}author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05002621"
base_color = "m/fallback.png"
keep_retail_picture = true
normal = "m/wall.png"
author = "a"
tool = "t"
license = "CC0-1.0"
"#
            )
        };
        let banded = manifest("metal_map = \"m/metal.png\"\nbrighten = 2\n");
        let mut files = files();
        files.insert(
            "m/metal.png".to_owned(),
            png_of(8, 8, &ramp(8, 8, [0, 0, 0, 255])),
        );
        let folder = ArtPack::open(&banded, &mut reader(&files)).expect("the folder opens");
        assert!(folder.problems.is_empty(), "{:?}", folder.problems);
        let metal = folder.materials[0].metal.as_ref().expect("a metal map");
        assert_eq!(metal.format, Some(BlockFormat::Bc4));
        assert_eq!(folder.materials[0].matter.metal, Some(2.0));
        assert!(folder.materials[1].metal.is_none());
        let packed = pack(&banded, &mut reader(&files), PLAIN).expect("the pack packs");
        let file = PackedFile::from_storage("test.dat", Box::new(packed.bytes)).expect("opens");
        let records = &file.contents().records;
        assert!(records.iter().any(|r| r.key == metal_key("m/metal.png")));
        let opened = ArtPack::from_packed(&file).expect("the packed pack opens");
        assert_eq!(opened.materials, folder.materials);
        assert_eq!(
            opened.material_picture(0, crate::MaterialPart::Metal),
            Some(metal.clone())
        );
        // Without the metal map the plain material's pictures pack into the same record.
        let plain = pack(&manifest(""), &mut reader(&files), PLAIN).expect("the pack packs");
        let plain = PackedFile::from_storage("plain.dat", Box::new(plain.bytes)).expect("opens");
        let hash =
            |records: &[Record], key: &str| records.iter().find(|r| r.key == key).map(|r| r.sha256);
        let key = material_key(
            &pyreal_assets::art_pack::ArtPackManifest::parse(&banded)
                .expect("reads")
                .materials[1],
        );
        assert!(hash(records, &key).is_some());
        assert_eq!(hash(records, &key), hash(&plain.contents().records, &key));
    }

    #[test]
    fn a_folder_or_its_packed_file_opens_by_its_path() {
        let root =
            std::env::temp_dir().join(format!("pyreal-art-pack-path-{}", std::process::id()));
        let dir = root.join("pack");
        for (path, bytes) in files() {
            let at = dir.join(&path);
            std::fs::create_dir_all(at.parent().expect("a folder")).expect("the folder");
            std::fs::write(at, bytes).expect("a file");
        }
        std::fs::write(dir.join("manifest.toml"), MANIFEST).expect("the manifest");
        let file = root.join("pack.dat");
        std::fs::write(&file, packed().bytes).expect("the packed file");
        let from_folder = ArtPack::open_path(&dir);
        let from_file = ArtPack::open_path(&file);
        let _ = std::fs::remove_dir_all(&root);
        let (from_folder, from_file) = (
            from_folder.expect("the folder opens"),
            from_file.expect("the packed file opens"),
        );
        assert_eq!(from_file.models, from_folder.models);
        assert_eq!(from_file.images, from_folder.images);
        assert_eq!(from_file.hash, from_folder.hash);
    }

    /// A pack written with the 16 KiB blocks of earlier packs opens as one written now.
    #[test]
    fn a_pack_of_earlier_larger_blocks_still_opens() {
        let files = files();
        let earlier = pack_in(MANIFEST, &mut reader(&files), 0x4000, PLAIN).expect("packs");
        let now = packed();
        let header = |bytes: &[u8]| {
            let file = DatFile::from_storage("t".into(), Box::new(bytes.to_vec())).expect("opens");
            file.header().block_size
        };
        assert_eq!(
            (header(&earlier.bytes), header(&now.bytes)),
            (0x4000, BLOCK)
        );
        assert!(now.bytes.len() < earlier.bytes.len());
        let (earlier, now) = (
            open(&earlier.bytes).expect("the earlier pack opens"),
            open(&now.bytes).expect("the pack opens"),
        );
        assert_eq!(earlier.models, now.models);
        assert_eq!(earlier.images, now.images);
        assert_eq!(earlier.grounds, now.grounds);
        assert_eq!(earlier.materials, now.materials);
    }

    #[test]
    fn the_same_files_always_pack_into_the_same_bytes() {
        let first = packed();
        let second = packed();
        assert!(
            first.bytes == second.bytes,
            "packed twice, the bytes differ"
        );
        // Exactly as large as its records need: no more than the writer's own room left over.
        let file = DatFile::from_storage("t".into(), Box::new(first.bytes)).expect("opens");
        assert!(u64::from(file.header().free_count) <= RESERVE + 2 + 16);
    }

    /// `bytes` with the byte `at` bytes into the record `id`'s first block changed.
    fn changed(bytes: &[u8], id: DataId, at: usize) -> Vec<u8> {
        let file = DatFile::from_storage("t".into(), Box::new(bytes.to_vec())).expect("opens");
        let entry = file.entry(id).expect("the record");
        let mut out = bytes.to_vec();
        out[entry.offset as usize + 4 + at] ^= 0x5A;
        out
    }

    #[test]
    fn a_damaged_or_mismatched_packed_file_is_refused_whole() {
        let packed = packed();
        let file =
            PackedFile::from_storage("test.dat", Box::new(packed.bytes.clone())).expect("opens");
        let id_of = |kind: Kind, key: &str| {
            file.contents()
                .records
                .iter()
                .find(|r| r.kind == kind && r.key.contains(key))
                .expect("the record")
                .id
        };
        // One texel of one picture changed: the whole pack is refused, not the one model.
        let picture = id_of(Kind::Pictures, "far.png");
        let e = open(&changed(&packed.bytes, picture, 40)).expect_err("a damaged picture");
        assert!(e.contains("is damaged") && e.contains("far.png"), "{e}");
        // One of a material's pictures changed: the pack is refused, though it would draw
        // without the material.
        let material = id_of(Kind::Pictures, "material:m/wall.png::");
        let e = open(&changed(&packed.bytes, material, 40)).expect_err("a damaged material");
        assert!(e.contains("is damaged") && e.contains("m/wall.png"), "{e}");
        // A model's file changed.
        let model = id_of(Kind::File, "lod0.gltf");
        let e = open(&changed(&packed.bytes, model, 10)).expect_err("a damaged model");
        assert!(e.contains("is damaged") && e.contains("lod0.gltf"), "{e}");
        // Its contents changed.
        let e = open(&changed(&packed.bytes, CONTENTS, 30)).expect_err("damaged contents");
        assert!(e.contains("damaged"), "{e}");
        // Cut short, inside its last record.
        let last = DatFile::from_storage("t".into(), Box::new(packed.bytes.clone()))
            .expect("opens")
            .entry(CONTENTS)
            .expect("the contents")
            .offset as usize;
        let e = open(&packed.bytes[..last + 8]).expect_err("cut short");
        assert!(e.contains("damaged"), "{e}");
        // Not a packed pack at all.
        let e = open(b"not a container").expect_err("not a container");
        assert!(e.contains("not a packed art pack"), "{e}");
        // Another layout, and another manifest format.
        let contents = file.contents().encode();
        let mut later = contents.clone();
        later[4..8].copy_from_slice(&3u32.to_le_bytes());
        let e = Contents::decode(&later).expect_err("another layout");
        assert!(e.contains("layout 3"), "{e}");
        let mut other = contents[..contents.len() - 32].to_vec();
        other[8..12].copy_from_slice(&2u32.to_le_bytes());
        let sum = record_hash(&other);
        other.extend_from_slice(&sum);
        let e = Contents::decode(&other).expect_err("another manifest format");
        assert!(e.contains("manifest format 2"), "{e}");
        // The undamaged file still opens.
        assert!(open(&packed.bytes).is_ok());
    }
}
