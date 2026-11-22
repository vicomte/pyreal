//! Bodies drawn in place of a setup's parts (`[[skin]]`): a skinned glTF mesh, its skeleton and
//! its clips, posed on the processor.
//!
//! **Spaces.** The rig, its clips and the bind pose stay in glTF's own space (height on its second
//! axis) and a posed vertex is turned onto the world's axes as it is written ([`UPRIGHT`]). A
//! posed body stands in its object's space: the object's frame and scale place it.
//!
//! **Posing.** Each frame a body takes every node's rest transform, lays its clip's sample over
//! the nodes the clip moves, sets the joints that name parts to the parts' own places where the
//! object's parts lead ([`Rig::globals`]), and skins each vertex by its four joints.
//!
//! **Slots** ([`Slot`]): what the object's motion does picks a slot, and an empty slot plays the
//! first of its fallbacks the body has.

use glam::{Mat3, Mat4, Quat, Vec3, Vec4};

use crate::gltf::Material;

/// Turns glTF's axes onto the world's: its second axis up.
pub const UPRIGHT: Mat4 = Mat4::from_cols(
    Vec4::new(1.0, 0.0, 0.0, 0.0),
    Vec4::new(0.0, 0.0, 1.0, 0.0),
    Vec4::new(0.0, -1.0, 0.0, 0.0),
    Vec4::new(0.0, 0.0, 0.0, 1.0),
);

/// One node of a body's glTF: its parent and its rest transform.
#[derive(Debug, Clone, PartialEq)]
pub struct RigNode {
    pub name: String,
    pub parent: Option<u16>,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl RigNode {
    /// Its rest transform as a matrix.
    #[must_use]
    pub fn rest(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// One joint of the skin: its node, its inverse bind matrix and the setup's part it names.
#[derive(Debug, Clone, PartialEq)]
pub struct Joint {
    pub node: u16,
    pub inverse_bind: Mat4,
    /// The setup's part it is drawn in place of (`part_NN`, or `extras.part`).
    pub part: Option<u16>,
}

/// The skeleton: every node, parents before their children, and the skin's joints.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rig {
    pub nodes: Vec<RigNode>,
    pub joints: Vec<Joint>,
}

/// What a channel moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelPath {
    Translation,
    Rotation,
    Scale,
}

/// How a channel's keys are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    /// Each key holds an in-tangent, a value and an out-tangent.
    CubicSpline,
}

/// One channel of a clip: one node's translation, rotation or scale over time.
#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub node: u16,
    pub path: ChannelPath,
    pub interpolation: Interpolation,
    pub times: Vec<f32>,
    /// Four floats a value (a translation or scale's fourth is unused), three values a key for a
    /// cubic spline.
    pub values: Vec<Vec4>,
}

/// One glTF animation.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub name: String,
    /// Seconds: its last key.
    pub duration: f32,
    pub channels: Vec<Channel>,
    /// Metres of the body's own space one round of a walk carries it (`extras.stride`).
    pub stride: Option<f32>,
    /// Seconds from an attack's start to its blow (`extras.hit`).
    pub hit: Option<f32>,
}

/// One primitive of the skinned mesh, in bind space.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinPrimitive {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    /// Four joints a vertex, by place in [`Rig::joints`].
    pub joints: Vec<[u16; 4]>,
    /// Their weights, summing to one.
    pub weights: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub material: Material,
}

/// A body as its glTF holds it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkinModel {
    pub rig: Rig,
    pub primitives: Vec<SkinPrimitive>,
    pub clips: Vec<Clip>,
}

/// A body's place for each motion: the slots a pack names clips for
/// ([`pyreal_assets::art_pack::CLIP_SLOTS`]), in that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slot {
    Idle,
    IdleMelee,
    IdleMissile,
    IdleMagic,
    Walk,
    WalkBack,
    Run,
    SideStep,
    Turn,
    MeleeHigh,
    MeleeMed,
    MeleeLow,
    Special,
    Missile,
    Cast,
    Telegraph,
    Hit,
    HitHeavy,
    Die,
    Dead,
    Sleep,
    Stance,
    Emote,
    Cheer,
    Fall,
    Jump,
}

/// How a slot's clip plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Play {
    /// Round and round at its own pace.
    Loop,
    /// Round once for each stride the body travels.
    Stride,
    /// Once, then the slot the motion names next.
    Once,
    /// Once, holding its end.
    Hold,
}

impl Slot {
    pub const ALL: [Self; 26] = [
        Self::Idle,
        Self::IdleMelee,
        Self::IdleMissile,
        Self::IdleMagic,
        Self::Walk,
        Self::WalkBack,
        Self::Run,
        Self::SideStep,
        Self::Turn,
        Self::MeleeHigh,
        Self::MeleeMed,
        Self::MeleeLow,
        Self::Special,
        Self::Missile,
        Self::Cast,
        Self::Telegraph,
        Self::Hit,
        Self::HitHeavy,
        Self::Die,
        Self::Dead,
        Self::Sleep,
        Self::Stance,
        Self::Emote,
        Self::Cheer,
        Self::Fall,
        Self::Jump,
    ];

    /// The slot a manifest names `name`.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        let k = pyreal_assets::art_pack::CLIP_SLOTS
            .iter()
            .position(|s| *s == name)?;
        Self::ALL.get(k).copied()
    }

    /// The slots played in its place when a body has no clip for it, nearest first.
    #[must_use]
    pub fn fallbacks(self) -> &'static [Self] {
        use Slot as S;
        match self {
            S::IdleMelee | S::IdleMissile | S::IdleMagic | S::Stance | S::Emote | S::Cheer => {
                &[S::Idle]
            }
            S::WalkBack | S::Run | S::SideStep => &[S::Walk],
            S::MeleeHigh | S::MeleeLow | S::Special | S::Telegraph => &[S::MeleeMed],
            S::Missile | S::Cast => &[S::MeleeMed],
            S::HitHeavy => &[S::Hit],
            S::Sleep => &[S::Dead],
            S::Jump => &[S::Fall],
            _ => &[],
        }
    }

    /// How its clip plays.
    #[must_use]
    pub fn play(self) -> Play {
        use Slot as S;
        match self {
            S::Walk | S::WalkBack | S::Run | S::SideStep => Play::Stride,
            S::Idle
            | S::IdleMelee
            | S::IdleMissile
            | S::IdleMagic
            | S::Turn
            | S::Sleep
            | S::Dead
            | S::Fall => Play::Loop,
            S::Die => Play::Hold,
            _ => Play::Once,
        }
    }
}

impl Clip {
    /// Lays its sample at `t` seconds over `locals` (each node's translation, rotation, scale).
    pub fn sample(&self, t: f32, locals: &mut [(Vec3, Quat, Vec3)]) {
        for c in &self.channels {
            let Some(l) = locals.get_mut(usize::from(c.node)) else {
                continue;
            };
            let v = c.at(t);
            match c.path {
                ChannelPath::Translation => l.0 = v.truncate(),
                ChannelPath::Rotation => l.1 = Quat::from_vec4(v).normalize(),
                ChannelPath::Scale => l.2 = v.truncate(),
            }
        }
    }
}

impl Channel {
    /// Its value at `t` seconds, held at its ends.
    #[must_use]
    pub fn at(&self, t: f32) -> Vec4 {
        let cubic = self.interpolation == Interpolation::CubicSpline;
        let value = |k: usize| {
            if cubic {
                self.values.get(k * 3 + 1)
            } else {
                self.values.get(k)
            }
            .copied()
            .unwrap_or(Vec4::ZERO)
        };
        let n = self.times.len();
        if n == 0 {
            return Vec4::ZERO;
        }
        if t <= self.times[0] {
            return value(0);
        }
        if t >= self.times[n - 1] {
            return value(n - 1);
        }
        let k = self.times.partition_point(|&x| x <= t).saturating_sub(1);
        let (t0, t1) = (self.times[k], self.times[k + 1]);
        let span = (t1 - t0).max(1e-6);
        let u = ((t - t0) / span).clamp(0.0, 1.0);
        let rotation = self.path == ChannelPath::Rotation;
        match self.interpolation {
            Interpolation::Step => value(k),
            Interpolation::Linear if rotation => {
                let (a, b) = (Quat::from_vec4(value(k)), Quat::from_vec4(value(k + 1)));
                Vec4::from(a.slerp(b, u))
            }
            Interpolation::Linear => value(k).lerp(value(k + 1), u),
            Interpolation::CubicSpline => {
                let out_tangent = self.values.get(k * 3 + 2).copied().unwrap_or(Vec4::ZERO);
                let in_tangent = self.values.get((k + 1) * 3).copied().unwrap_or(Vec4::ZERO);
                let (u2, u3) = (u * u, u * u * u);
                let v = value(k) * (2.0 * u3 - 3.0 * u2 + 1.0)
                    + out_tangent * span * (u3 - 2.0 * u2 + u)
                    + value(k + 1) * (-2.0 * u3 + 3.0 * u2)
                    + in_tangent * span * (u3 - u2);
                if rotation {
                    Vec4::from(Quat::from_vec4(v).normalize())
                } else {
                    v
                }
            }
        }
    }
}

impl Rig {
    /// Every node's rest translation, rotation and scale.
    #[must_use]
    pub fn rest(&self) -> Vec<(Vec3, Quat, Vec3)> {
        self.nodes
            .iter()
            .map(|n| (n.translation, n.rotation, n.scale))
            .collect()
    }

    /// Each node's transform in the body's space (glTF axes) from `locals`; a node `fixed` names
    /// (by node) takes the transform given instead, and its children follow it.
    #[must_use]
    pub fn globals(&self, locals: &[(Vec3, Quat, Vec3)], fixed: &[(u16, Mat4)]) -> Vec<Mat4> {
        let mut out: Vec<Mat4> = Vec::with_capacity(self.nodes.len());
        for (k, n) in self.nodes.iter().enumerate() {
            let own = fixed
                .iter()
                .find(|(i, _)| usize::from(*i) == k)
                .map(|f| f.1);
            let g = own.unwrap_or_else(|| {
                let (t, r, s) =
                    locals
                        .get(k)
                        .copied()
                        .unwrap_or((n.translation, n.rotation, n.scale));
                let local = Mat4::from_scale_rotation_translation(s, r, t);
                n.parent
                    .and_then(|p| out.get(usize::from(p)))
                    .map_or(local, |p| *p * local)
            });
            out.push(g);
        }
        out
    }

    /// Each joint's skinning matrix from the nodes' transforms.
    #[must_use]
    pub fn joint_matrices(&self, globals: &[Mat4]) -> Vec<Mat4> {
        self.joints
            .iter()
            .map(|j| {
                globals
                    .get(usize::from(j.node))
                    .map_or(Mat4::IDENTITY, |g| *g * j.inverse_bind)
            })
            .collect()
    }

    /// The parts its joints name, in order.
    #[must_use]
    pub fn parts(&self) -> Vec<u16> {
        let mut p: Vec<u16> = self.joints.iter().filter_map(|j| j.part).collect();
        p.sort_unstable();
        p.dedup();
        p
    }
}

/// A part's transform in the body's space (world axes) as the transform of its joint's node, in
/// glTF's axes.
#[must_use]
pub fn part_to_rig(part: Mat4) -> Mat4 {
    UPRIGHT.inverse() * part * UPRIGHT
}

/// One posed vertex: world axes, in the object's space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Posed {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
}

impl SkinModel {
    /// Every vertex of every primitive, in order, skinned by `joints` ([`Rig::joint_matrices`]).
    pub fn pose(&self, joints: &[Mat4], out: &mut Vec<Posed>) {
        out.clear();
        for p in &self.primitives {
            for v in 0..p.positions.len() {
                let (j, w) = (p.joints[v], p.weights[v]);
                let mut m = Mat4::ZERO;
                for k in 0..4 {
                    if w[k] > 0.0 {
                        m += joints
                            .get(usize::from(j[k]))
                            .copied()
                            .unwrap_or(Mat4::IDENTITY)
                            * w[k];
                    }
                }
                let position = UPRIGHT.transform_point3(m.transform_point3(p.positions[v]));
                let turn = Mat3::from_mat4(m);
                let normal = (Mat3::from_mat4(UPRIGHT) * (turn * p.normals[v])).normalize_or_zero();
                out.push(Posed {
                    position,
                    normal,
                    uv: p.uvs[v],
                });
            }
        }
    }

    /// Vertices in all.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.primitives.iter().map(|p| p.positions.len()).sum()
    }

    /// The clip named `name`.
    #[must_use]
    pub fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name == name)
    }
}

/// Writes `model` as glTF 2.0 text and the one buffer it names, `bin_name` beside it: its nodes,
/// one skinned mesh of plain materials (their base colours), its skin and its clips.
#[must_use]
#[allow(clippy::too_many_lines)] // one document, written in order
pub fn write_gltf(model: &SkinModel, bin_name: &str) -> (String, Vec<u8>) {
    use serde_json::json;
    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    // One view and accessor for each array of floats or u16s.
    let mut push =
        |bytes: Vec<u8>, count: usize, kind: &str, component: u32, extra: serde_json::Value| {
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            let offset = bin.len();
            bin.extend_from_slice(&bytes);
            views.push(json!({"buffer": 0, "byteOffset": offset, "byteLength": bytes.len()}));
            let mut a = json!({
                "bufferView": views.len() - 1,
                "componentType": component,
                "count": count,
                "type": kind,
            });
            if let (Some(o), Some(e)) = (a.as_object_mut(), extra.as_object()) {
                for (k, v) in e {
                    o.insert(k.clone(), v.clone());
                }
            }
            accessors.push(a);
            accessors.len() - 1
        };
    let floats =
        |v: &mut dyn Iterator<Item = f32>| -> Vec<u8> { v.flat_map(f32::to_le_bytes).collect() };
    let mut primitives = Vec::new();
    let mut materials = Vec::new();
    for p in &model.primitives {
        let n = p.positions.len();
        let (lo, hi) = p.positions.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), v| (lo.min(*v), hi.max(*v)),
        );
        let pos = push(
            floats(&mut p.positions.iter().flat_map(|v| v.to_array())),
            n,
            "VEC3",
            5126,
            json!({"min": lo.to_array(), "max": hi.to_array()}),
        );
        let nrm = push(
            floats(&mut p.normals.iter().flat_map(|v| v.to_array())),
            n,
            "VEC3",
            5126,
            json!({}),
        );
        let uv = push(
            floats(&mut p.uvs.iter().flatten().copied()),
            n,
            "VEC2",
            5126,
            json!({}),
        );
        let joints = push(
            p.joints
                .iter()
                .flatten()
                .flat_map(|j| j.to_le_bytes())
                .collect(),
            n,
            "VEC4",
            5123,
            json!({}),
        );
        let weights = push(
            floats(&mut p.weights.iter().flatten().copied()),
            n,
            "VEC4",
            5126,
            json!({}),
        );
        let indices = push(
            p.indices.iter().flat_map(|i| i.to_le_bytes()).collect(),
            p.indices.len(),
            "SCALAR",
            5125,
            json!({}),
        );
        materials.push(json!({
            "pbrMetallicRoughness": {
                "baseColorFactor": p.material.base_color,
                "metallicFactor": p.material.metallic,
                "roughnessFactor": p.material.roughness,
            },
            "doubleSided": p.material.double_sided,
        }));
        primitives.push(json!({
            "attributes": {"POSITION": pos, "NORMAL": nrm, "TEXCOORD_0": uv,
                "JOINTS_0": joints, "WEIGHTS_0": weights},
            "indices": indices,
            "material": materials.len() - 1,
        }));
    }
    let ibm = push(
        floats(
            &mut model
                .rig
                .joints
                .iter()
                .flat_map(|j| j.inverse_bind.to_cols_array()),
        ),
        model.rig.joints.len(),
        "MAT4",
        5126,
        json!({}),
    );
    let mut animations = Vec::new();
    for c in &model.clips {
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for ch in &c.channels {
            let (lo, hi) = ch
                .times
                .iter()
                .fold((f32::MAX, f32::MIN), |(lo, hi), t| (lo.min(*t), hi.max(*t)));
            let input = push(
                floats(&mut ch.times.iter().copied()),
                ch.times.len(),
                "SCALAR",
                5126,
                json!({"min": [lo], "max": [hi]}),
            );
            let (kind, width) = match ch.path {
                ChannelPath::Rotation => ("VEC4", 4),
                _ => ("VEC3", 3),
            };
            let output = push(
                floats(
                    &mut ch
                        .values
                        .iter()
                        .flat_map(|v| v.to_array().into_iter().take(width)),
                ),
                ch.values.len(),
                kind,
                5126,
                json!({}),
            );
            samplers.push(json!({
                "input": input,
                "output": output,
                "interpolation": match ch.interpolation {
                    Interpolation::Linear => "LINEAR",
                    Interpolation::Step => "STEP",
                    Interpolation::CubicSpline => "CUBICSPLINE",
                },
            }));
            channels.push(json!({
                "sampler": samplers.len() - 1,
                "target": {"node": ch.node, "path": match ch.path {
                    ChannelPath::Translation => "translation",
                    ChannelPath::Rotation => "rotation",
                    ChannelPath::Scale => "scale",
                }},
            }));
        }
        let mut extras = serde_json::Map::new();
        if let Some(s) = c.stride {
            extras.insert("stride".into(), json!(s));
        }
        if let Some(h) = c.hit {
            extras.insert("hit".into(), json!(h));
        }
        let mut a = json!({"name": c.name, "samplers": samplers, "channels": channels});
        if !extras.is_empty() {
            a["extras"] = serde_json::Value::Object(extras);
        }
        animations.push(a);
    }
    let mesh_node = model.rig.nodes.len();
    let mut nodes: Vec<serde_json::Value> = model
        .rig
        .nodes
        .iter()
        .map(|n| {
            json!({
                "name": n.name,
                "translation": n.translation.to_array(),
                "rotation": n.rotation.to_array(),
                "scale": n.scale.to_array(),
            })
        })
        .collect();
    for (k, n) in model.rig.nodes.iter().enumerate() {
        if let Some(p) = n.parent {
            let children = nodes[usize::from(p)]
                .as_object_mut()
                .map(|o| o.entry("children").or_insert_with(|| json!([])));
            if let Some(serde_json::Value::Array(c)) = children {
                c.push(json!(k));
            }
        }
    }
    for joint in &model.rig.joints {
        if let (Some(part), Some(n)) = (joint.part, nodes.get_mut(usize::from(joint.node))) {
            n["extras"] = json!({"part": part});
        }
    }
    nodes.push(json!({"name": "body", "mesh": 0, "skin": 0}));
    let mut roots: Vec<usize> = model
        .rig
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.parent.is_none())
        .map(|(k, _)| k)
        .collect();
    roots.push(mesh_node);
    let doc = json!({
        "asset": {"version": "2.0", "generator": "pyreal-art"},
        "scene": 0,
        "scenes": [{"nodes": roots}],
        "nodes": nodes,
        "meshes": [{"primitives": primitives}],
        "materials": materials,
        "skins": [{
            "joints": model.rig.joints.iter().map(|j| j.node).collect::<Vec<_>>(),
            "inverseBindMatrices": ibm,
        }],
        "animations": animations,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"uri": bin_name, "byteLength": bin.len()}],
    });
    (serde_json::to_string_pretty(&doc).unwrap_or_default(), bin)
}

/// A small body to try the path with: a column of three boxes (`part_00`, then two bones up its
/// spine, `spine` and `head`), each box `size` metres across, every vertex on one joint; with a
/// clip `Sway` that bends the spine back and forth over two seconds (`extras.stride` one metre),
/// and `Nod` that tips the head once (`extras.hit` half a second in).
#[must_use]
pub fn sample_body(size: f32, color: [f32; 4]) -> SkinModel {
    let h = size;
    let nodes = vec![
        RigNode {
            name: "part_00".into(),
            parent: None,
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        RigNode {
            name: "spine".into(),
            parent: Some(0),
            translation: Vec3::new(0.0, h, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        RigNode {
            name: "head".into(),
            parent: Some(1),
            translation: Vec3::new(0.0, h, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
    ];
    let joints = (0u16..3)
        .map(|k| Joint {
            node: k,
            inverse_bind: Mat4::from_translation(Vec3::new(0.0, -h * f32::from(k), 0.0)),
            part: (k == 0).then_some(0),
        })
        .collect();
    let mut p = SkinPrimitive {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        joints: Vec::new(),
        weights: Vec::new(),
        indices: Vec::new(),
        material: Material {
            base_color: color,
            metallic: 0.0,
            roughness: 0.6,
            ..Material::default()
        },
    };
    // Each box's six faces, four corners each, outward.
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y, Vec3::X),
    ];
    for k in 0u16..3 {
        let centre = Vec3::new(0.0, h * (f32::from(k) + 0.5), 0.0);
        let half = h * 0.4 * (1.0 - 0.15 * f32::from(k));
        for (n, a, b) in faces {
            let base = u32::try_from(p.positions.len()).unwrap_or(0);
            for (u, v) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                p.positions.push(centre + (n + a * u + b * v) * half);
                p.normals.push(n);
                p.uvs.push([u * 0.5 + 0.5, v * 0.5 + 0.5]);
                p.joints.push([k, 0, 0, 0]);
                p.weights.push([1.0, 0.0, 0.0, 0.0]);
            }
            p.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    let bend = |deg: f32| Vec4::from(Quat::from_rotation_z(deg.to_radians()));
    let clips = vec![
        Clip {
            name: "Sway".into(),
            duration: 2.0,
            channels: vec![Channel {
                node: 1,
                path: ChannelPath::Rotation,
                interpolation: Interpolation::Linear,
                times: vec![0.0, 0.5, 1.0, 1.5, 2.0],
                values: vec![bend(0.0), bend(35.0), bend(0.0), bend(-35.0), bend(0.0)],
            }],
            stride: Some(1.0),
            hit: None,
        },
        Clip {
            name: "Nod".into(),
            duration: 1.0,
            channels: vec![Channel {
                node: 2,
                path: ChannelPath::Rotation,
                interpolation: Interpolation::Linear,
                times: vec![0.0, 0.5, 1.0],
                values: vec![
                    Vec4::from(Quat::IDENTITY),
                    Vec4::from(Quat::from_rotation_x(0.9)),
                    Vec4::from(Quat::IDENTITY),
                ],
            }],
            stride: None,
            hit: Some(0.5),
        },
    ];
    SkinModel {
        rig: Rig { nodes, joints },
        primitives: vec![p],
        clips,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_back(model: &SkinModel) -> SkinModel {
        let (json, bin) = write_gltf(model, "body.bin");
        crate::gltf::decode_skin(json.as_bytes(), "b/body.gltf", &mut |p| {
            assert_eq!(p, "b/body.bin");
            Ok(bin.clone())
        })
        .expect("reads")
    }

    /// A body written as glTF reads back the same: its nodes, joints and the parts they name,
    /// its vertices and their joints, and its clips with their stride and blow.
    #[test]
    fn a_written_body_reads_back_as_it_was() {
        let body = sample_body(0.5, [0.8, 0.2, 0.2, 1.0]);
        let back = read_back(&body);
        // The skeleton's three nodes, then the mesh's own.
        assert_eq!(back.rig.nodes.len(), 4);
        assert_eq!(back.rig.nodes[2].parent, Some(1));
        assert_eq!(back.rig.parts(), [0]);
        assert_eq!(back.rig.joints[1].part, None);
        assert_eq!(back.primitives[0].positions, body.primitives[0].positions);
        assert_eq!(back.primitives[0].joints, body.primitives[0].joints);
        assert_eq!(back.clips.len(), 2);
        assert_eq!(back.clips[0].stride, Some(1.0));
        assert_eq!(back.clips[1].hit, Some(0.5));
        assert!((back.clips[0].duration - 2.0).abs() < 1e-6);
    }

    /// At rest a body stands as it was bound, turned upright: glTF's height on the world's third
    /// axis.
    #[test]
    fn at_rest_a_body_stands_as_bound_and_upright() {
        let body = sample_body(0.5, [1.0; 4]);
        let globals = body.rig.globals(&body.rig.rest(), &[]);
        let mut out = Vec::new();
        body.pose(&body.rig.joint_matrices(&globals), &mut out);
        assert_eq!(out.len(), body.vertex_count());
        for (posed, bind) in out.iter().zip(&body.primitives[0].positions) {
            let want = Vec3::new(bind.x, -bind.z, bind.y);
            assert!(posed.position.distance(want) < 1e-5, "{posed:?} {want}");
        }
        let top = out.iter().map(|v| v.position.z).fold(f32::MIN, f32::max);
        assert!(
            top > 1.3,
            "the head stands up the world's third axis: {top}"
        );
    }

    /// A clip bends the bones it moves, and the bones under them follow; a part's joint set from
    /// the part carries the bones on it.
    #[test]
    fn a_clip_bends_its_bones_and_a_part_carries_its_joint() {
        let body = sample_body(0.5, [1.0; 4]);
        let mut locals = body.rig.rest();
        body.clips[0].sample(0.5, &mut locals);
        let globals = body.rig.globals(&locals, &[]);
        let mut out = Vec::new();
        body.pose(&body.rig.joint_matrices(&globals), &mut out);
        let head = |o: &[Posed]| o[48..].iter().map(|v| v.position).sum::<Vec3>() / 24.0;
        // Bent 35 degrees about glTF's z (the world's -y): the head leans along -x.
        assert!(head(&out).x < -0.2, "{}", head(&out));
        // Moving part 0 up a metre lifts the whole column.
        let lifted = part_to_rig(Mat4::from_translation(Vec3::Z));
        let globals = body.rig.globals(&body.rig.rest(), &[(0, lifted)]);
        body.pose(&body.rig.joint_matrices(&globals), &mut out);
        let rest_head = Vec3::new(0.0, 0.0, 0.5 * 2.5);
        assert!(
            head(&out).distance(rest_head + Vec3::Z) < 1e-4,
            "{}",
            head(&out)
        );
    }

    /// Linear keys join smoothly and hold at their ends, and a step holds each key.
    #[test]
    fn keys_join_as_their_interpolation_says() {
        let mut c = Channel {
            node: 0,
            path: ChannelPath::Translation,
            interpolation: Interpolation::Linear,
            times: vec![1.0, 2.0],
            values: vec![Vec4::ZERO, Vec4::new(2.0, 0.0, 0.0, 0.0)],
        };
        assert_eq!(c.at(0.0).x, 0.0);
        assert!((c.at(1.5).x - 1.0).abs() < 1e-6);
        assert_eq!(c.at(9.0).x, 2.0);
        c.interpolation = Interpolation::Step;
        assert_eq!(c.at(1.9).x, 0.0);
    }

    /// An empty slot plays its fallbacks' clips, and each slot names how it plays.
    #[test]
    fn slots_fall_back_and_play_their_way() {
        assert_eq!(Slot::named("run"), Some(Slot::Run));
        assert_eq!(Slot::named("swim"), None);
        assert_eq!(Slot::Run.fallbacks(), [Slot::Walk]);
        assert_eq!(Slot::Run.play(), Play::Stride);
        assert_eq!(Slot::Die.play(), Play::Hold);
        assert_eq!(Slot::MeleeHigh.play(), Play::Once);
        for (k, s) in Slot::ALL.iter().enumerate() {
            assert_eq!(
                Slot::named(pyreal_assets::art_pack::CLIP_SLOTS[k]),
                Some(*s)
            );
        }
    }
}
