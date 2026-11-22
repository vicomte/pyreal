//! The disk image's Finder window: a `.DS_Store` that sizes it, hides its bars, sets its picture
//! and the icon size, and places the app and the Applications link. It is written here, not by
//! the Finder, so a headless release runner makes the same window as a desktop does.
//!
//! The picture is named by an alias record: the volume's name and creation date, the picture's
//! file and folder ids and its path. Those ids exist only once the image is made, so
//! [`ds_store`] reads them from the mounted, writable image.

use std::path::Path;
use std::time::UNIX_EPOCH;

/// The folder on the image that holds the window's picture; the Finder hides dot folders.
pub const BACKGROUND_DIR: &str = ".background";
/// The picture's name: a TIFF holding the 1x and 2x drawings, named for its content, since the
/// Finder keeps a picture it has shown by its volume and path.
pub fn background_name(tiff: &[u8]) -> String {
    format!("background-{}.tiff", &super::sha256_hex(tiff)[..8])
}
/// The window's settings file.
pub const DS_STORE: &str = ".DS_Store";
/// Every `.DS_Store` written here is this long, so a placeholder can reserve its room.
pub const DS_STORE_LEN: usize = 4 + SPACE;

/// The window's content size in points, the picture's 1x size.
pub const WIDTH: u32 = 640;
/// The content height.
pub const HEIGHT: u32 = 400;
/// The title bar the Finder adds above the content.
const TITLE_BAR: u32 = 33;
/// The icons' size in points.
const ICON_SIZE: f64 = 160.0;
/// The labels' size in points.
const TEXT_SIZE: f64 = 14.0;
/// Where the app's and the link's icon centres sit, in content points.
pub const APP_AT: (u32, u32) = (160, 190);
/// The Applications link's icon centre.
pub const LINK_AT: (u32, u32) = (480, 190);
/// The colour under the picture, red, green and blue from 0 to 1.
const BACKGROUND_COLOUR: (f64, f64, f64) = (1.0, 1.0, 1.0);
/// The image's hidden items, and where they sit: below the window, out of view when a player
/// shows hidden files.
const HIDDEN: [&str; 5] = [
    BACKGROUND_DIR,
    DS_STORE,
    ".fseventsd",
    ".Trashes",
    ".VolumeIcon.icns",
];
const HIDDEN_AT: (u32, u32) = (WIDTH / 2, HEIGHT + 240);
/// Where the window opens on screen.
const WINDOW_AT: (u32, u32) = (200, 120);

/// Seconds from 1904-01-01 (the Mac's epoch) to 1970-01-01.
const MAC_EPOCH_OFFSET: u64 = 2_082_844_800;

/// The buddy allocator's space: the header, the B-tree's master block, the allocator's own
/// block and one leaf node.
const SPACE: usize = 0x2000;
const MASTER_AT: usize = 0x20;
const ROOT_AT: usize = 0x800;
const LEAF_AT: usize = 0x1000;
const LEAF_LEN: usize = 0x1000;

/// The `.DS_Store` for the image mounted at `mount`, named `volume`, holding `app` and the
/// Applications link `link`, its picture already at [`BACKGROUND_DIR`]/`picture`.
pub fn ds_store(
    volume: &str,
    mount: &Path,
    app: &str,
    link: &str,
    picture: &str,
) -> Result<Vec<u8>, String> {
    let picture = alias(volume, mount, &format!("{BACKGROUND_DIR}/{picture}"))?;
    let bwsp = Plist::Dict(vec![
        ("ContainerShowSidebar", Plist::Bool(false)),
        ("PreviewPaneVisibility", Plist::Bool(false)),
        ("ShowPathbar", Plist::Bool(false)),
        ("ShowSidebar", Plist::Bool(false)),
        ("ShowStatusBar", Plist::Bool(false)),
        ("ShowTabView", Plist::Bool(false)),
        ("ShowToolbar", Plist::Bool(false)),
        ("SidebarWidth", Plist::Int(0)),
        (
            "WindowBounds",
            Plist::Str(format!(
                "{{{{{}, {}}}, {{{}, {}}}}}",
                WINDOW_AT.0,
                WINDOW_AT.1,
                WIDTH,
                HEIGHT + TITLE_BAR
            )),
        ),
    ]);
    let icvp = Plist::Dict(vec![
        ("arrangeBy", Plist::Str("none".to_owned())),
        ("backgroundColorBlue", Plist::Real(BACKGROUND_COLOUR.2)),
        ("backgroundColorGreen", Plist::Real(BACKGROUND_COLOUR.1)),
        ("backgroundColorRed", Plist::Real(BACKGROUND_COLOUR.0)),
        ("backgroundImageAlias", Plist::Data(picture)),
        ("backgroundType", Plist::Int(2)),
        ("gridOffsetX", Plist::Real(0.0)),
        ("gridOffsetY", Plist::Real(0.0)),
        ("gridSpacing", Plist::Real(100.0)),
        ("iconSize", Plist::Real(ICON_SIZE)),
        ("labelOnBottom", Plist::Bool(true)),
        ("showIconPreview", Plist::Bool(true)),
        ("showItemInfo", Plist::Bool(false)),
        ("textSize", Plist::Real(TEXT_SIZE)),
        ("viewOptionsVersion", Plist::Int(1)),
    ]);
    let mut records = vec![
        Record::new(".", b"bwsp", Value::Blob(bwsp.to_bytes())),
        Record::new(".", b"icvp", Value::Blob(icvp.to_bytes())),
        Record::new(".", b"vSrn", Value::Long(1)),
        Record::new(".", b"vstl", Value::Type(*b"icnv")),
        Record::new(app, b"Iloc", Value::Blob(iloc(APP_AT))),
        Record::new(link, b"Iloc", Value::Blob(iloc(LINK_AT))),
    ];
    // Hidden items placed by the Finder land in the grid beside the icons.
    for hidden in HIDDEN {
        records.push(Record::new(hidden, b"Iloc", Value::Blob(iloc(HIDDEN_AT))));
    }
    store(&mut records)
}

/// An icon's place: its centre, then the Finder's fixed trailer.
fn iloc((x, y): (u32, u32)) -> Vec<u8> {
    let mut b = Vec::with_capacity(16);
    b.extend(x.to_be_bytes());
    b.extend(y.to_be_bytes());
    b.extend([0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0]);
    b
}

/// A record's value, by its four-character type.
enum Value {
    Long(u32),
    Type([u8; 4]),
    Blob(Vec<u8>),
}

struct Record {
    name: String,
    code: [u8; 4],
    value: Value,
}

impl Record {
    fn new(name: &str, code: &[u8; 4], value: Value) -> Self {
        Self {
            name: name.to_owned(),
            code: *code,
            value,
        }
    }

    fn write(&self, out: &mut Vec<u8>) {
        let units: Vec<u16> = self.name.encode_utf16().collect();
        out.extend(u32_len(units.len()).to_be_bytes());
        for u in units {
            out.extend(u.to_be_bytes());
        }
        out.extend(self.code);
        match &self.value {
            Value::Long(v) => {
                out.extend(b"long");
                out.extend(v.to_be_bytes());
            }
            Value::Type(t) => {
                out.extend(b"type");
                out.extend(t);
            }
            Value::Blob(b) => {
                out.extend(b"blob");
                out.extend(u32_len(b.len()).to_be_bytes());
                out.extend(b);
            }
        }
    }
}

fn u32_len(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The file: a buddy allocator holding a one-leaf B-tree of `records`, sorted as the Finder
/// expects (by name ignoring case, then by code).
fn store(records: &mut [Record]) -> Result<Vec<u8>, String> {
    records.sort_by_key(|r| (r.name.to_lowercase(), r.code));
    let mut leaf = Vec::with_capacity(LEAF_LEN);
    leaf.extend(0u32.to_be_bytes()); // no children: a leaf
    leaf.extend(u32_len(records.len()).to_be_bytes());
    for r in records.iter() {
        r.write(&mut leaf);
    }
    if leaf.len() > LEAF_LEN {
        return Err(format!(
            "the Finder settings take {} bytes, more than one {LEAF_LEN}-byte page",
            leaf.len()
        ));
    }

    // The B-tree's master block: its root node (block 2), depth, records, nodes, page size.
    let mut master = Vec::new();
    for v in [2, 0, u32_len(records.len()), 1, u32_len(LEAF_LEN)] {
        master.extend(v.to_be_bytes());
    }

    // The allocator's block: the block addresses (offset | log2 size), the directory naming
    // the B-tree, and the free lists of the space left over.
    let mut root = Vec::new();
    let blocks = [ROOT_AT | 11, MASTER_AT | 5, LEAF_AT | 12];
    root.extend(u32_len(blocks.len()).to_be_bytes());
    root.extend(0u32.to_be_bytes());
    for i in 0..256 {
        let a = blocks.get(i).copied().unwrap_or(0);
        root.extend(u32_len(a).to_be_bytes());
    }
    root.extend(1u32.to_be_bytes());
    root.push(4);
    root.extend(b"DSDB");
    root.extend(1u32.to_be_bytes());
    for k in 0..32u32 {
        // Free buddies of the four used blocks: 0x40..0x400 below the leaf, 0x2000 up above it.
        if (6..=10).contains(&k) || (13..=30).contains(&k) {
            root.extend(1u32.to_be_bytes());
            root.extend((1u32 << k).to_be_bytes());
        } else {
            root.extend(0u32.to_be_bytes());
        }
    }

    let mut space = vec![0u8; SPACE];
    // The header: its magic, then the allocator block's offset, size and offset again.
    space[..4].copy_from_slice(b"Bud1");
    for (i, v) in [ROOT_AT, 0x800, ROOT_AT].into_iter().enumerate() {
        space[4 + i * 4..8 + i * 4].copy_from_slice(&u32_len(v).to_be_bytes());
    }
    space[MASTER_AT..MASTER_AT + master.len()].copy_from_slice(&master);
    space[ROOT_AT..ROOT_AT + root.len()].copy_from_slice(&root);
    space[LEAF_AT..LEAF_AT + leaf.len()].copy_from_slice(&leaf);
    let mut file = 1u32.to_be_bytes().to_vec();
    file.extend(space);
    Ok(file)
}

/// A file's id on its volume, which an alias names it by.
#[cfg(unix)]
fn inode(m: &std::fs::Metadata) -> Result<u64, String> {
    use std::os::unix::fs::MetadataExt;
    Ok(m.ino())
}

/// A file's id on its volume: only a Mac makes the disk image.
#[cfg(not(unix))]
fn inode(_: &std::fs::Metadata) -> Result<u64, String> {
    Err("the disk image is made on a Mac".into())
}

/// A version 2 alias record for `rel` (a path inside the volume) on the volume `name`
/// mounted at `mount`, as the Finder's picture setting holds it.
fn alias(name: &str, mount: &Path, rel: &str) -> Result<Vec<u8>, String> {
    let meta = |p: &Path| std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()));
    let file = mount.join(rel);
    let folder = file
        .parent()
        .ok_or_else(|| format!("{} has no folder", file.display()))?;
    let (target, parent, volume) = (meta(&file)?, meta(folder)?, meta(mount)?);
    let mac_date = |m: &std::fs::Metadata| -> u32 {
        let unix = m
            .created()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        u32::try_from(unix + MAC_EPOCH_OFFSET).unwrap_or(0)
    };
    let file_name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder_name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder_id = u32::try_from(inode(&parent)?).map_err(|_| "folder id over 32 bits")?;
    let file_id = u32::try_from(inode(&target)?).map_err(|_| "file id over 32 bits")?;

    let mut b = vec![0u8; 8]; // creator code, then the length and version, set below
    b.extend(0u16.to_be_bytes()); // a file
    b.extend(pascal(name, 28));
    b.extend(mac_date(&volume).to_be_bytes());
    b.extend(b"H+");
    b.extend(5u16.to_be_bytes()); // an ejectable disk
    b.extend(folder_id.to_be_bytes());
    b.extend(pascal(&file_name, 64));
    b.extend(file_id.to_be_bytes());
    b.extend(mac_date(&target).to_be_bytes());
    b.extend([0u8; 8]); // type and creator codes
    b.extend((-1i16).to_be_bytes()); // levels up from the alias
    b.extend((-1i16).to_be_bytes()); // levels down to the target
    b.extend([0u8; 4 + 2 + 10]); // volume flags, file system id, reserved

    let mut tag = |t: i16, data: &[u8]| {
        b.extend(t.to_be_bytes());
        b.extend(u16::try_from(data.len()).unwrap_or(0).to_be_bytes());
        b.extend(data);
        if data.len() % 2 == 1 {
            b.push(0);
        }
    };
    let utf16 = |s: &str| {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut d = u16::try_from(units.len())
            .unwrap_or(0)
            .to_be_bytes()
            .to_vec();
        for u in units {
            d.extend(u.to_be_bytes());
        }
        d
    };
    tag(0, folder_name.as_bytes());
    tag(1, &folder_id.to_be_bytes());
    tag(2, format!("{name}:{}", rel.replace('/', ":")).as_bytes());
    tag(14, &utf16(&file_name));
    tag(15, &utf16(name));
    tag(18, format!("/{rel}").as_bytes());
    tag(19, format!("/Volumes/{name}").as_bytes());
    b.extend((-1i16).to_be_bytes());
    b.extend(0u16.to_be_bytes());
    let len = u16::try_from(b.len()).map_err(|_| "alias over 64 KB")?;
    b[4..6].copy_from_slice(&len.to_be_bytes());
    b[6..8].copy_from_slice(&2u16.to_be_bytes());
    Ok(b)
}

/// `s` as a Pascal string in a field of `size` bytes, cut to fit.
fn pascal(s: &str, size: usize) -> Vec<u8> {
    let mut bytes: Vec<u8> = s.replace(':', "/").into_bytes();
    bytes.truncate(size - 1);
    let mut out = vec![u8::try_from(bytes.len()).unwrap_or(0)];
    out.extend(&bytes);
    out.resize(size, 0);
    out
}

/// The few property list values the Finder's settings use, written as a binary plist.
pub(super) enum Plist {
    Bool(bool),
    Int(u64),
    Real(f64),
    Str(String),
    Data(Vec<u8>),
    Dict(Vec<(&'static str, Plist)>),
}

impl Plist {
    /// The `bplist00` bytes: the objects in depth order, their offsets, then the trailer.
    pub(super) fn to_bytes(&self) -> Vec<u8> {
        let mut objects: Vec<Vec<u8>> = Vec::new();
        self.flatten(&mut objects);
        let mut out = b"bplist00".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());
        for o in &objects {
            offsets.push(out.len());
            out.extend(o);
        }
        let table_at = out.len();
        for o in &offsets {
            out.extend(u32_len(*o).to_be_bytes());
        }
        out.extend([0u8; 6]);
        out.push(4); // offset size
        out.push(2); // object reference size
        out.extend((objects.len() as u64).to_be_bytes());
        out.extend(0u64.to_be_bytes()); // the top object
        out.extend((table_at as u64).to_be_bytes());
        out
    }

    /// Push this value's object, and its keys' and values' after it; returns its index.
    fn flatten(&self, objects: &mut Vec<Vec<u8>>) -> u16 {
        let at = objects.len();
        objects.push(Vec::new());
        let bytes = match self {
            Self::Bool(v) => vec![if *v { 0x09 } else { 0x08 }],
            Self::Int(v) => {
                let mut b = vec![0x13];
                b.extend(v.to_be_bytes());
                b
            }
            Self::Real(v) => {
                let mut b = vec![0x23];
                b.extend(v.to_be_bytes());
                b
            }
            Self::Str(s) => marked(0x50, s.len(), s.as_bytes()),
            Self::Data(d) => marked(0x40, d.len(), d),
            Self::Dict(entries) => {
                let mut keys = Vec::new();
                let mut values = Vec::new();
                for (k, _) in entries {
                    keys.push(Self::Str((*k).to_owned()).flatten(objects));
                }
                for (_, v) in entries {
                    values.push(v.flatten(objects));
                }
                let mut b = marked(0xd0, entries.len(), &[]);
                for r in keys.iter().chain(&values) {
                    b.extend(r.to_be_bytes());
                }
                b
            }
        };
        objects[at] = bytes;
        u16::try_from(at).unwrap_or(u16::MAX)
    }
}

/// An object's marker byte with its count, the count spilled into an int when 15 or more.
fn marked(kind: u8, count: usize, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    if count < 15 {
        b.push(kind | u8::try_from(count).unwrap_or(0));
    } else {
        b.push(kind | 0x0f);
        b.push(0x13);
        b.extend((count as u64).to_be_bytes());
    }
    b.extend(body);
    b
}
