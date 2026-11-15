//! A setup written as a body to rig against (`[[skin]]`): a glTF skeleton of one joint for each
//! part, `part_00` on, at the setup's resting placement, and the parts' own triangles skinned
//! rigidly to them, in a folder that is an art pack drawing the setup as that body.
//!
//! Drawn as it is, the body follows its parts as the game animates them and looks as the setup
//! does. A modeller replaces the triangles, adds bones under the part joints and clips, and keeps
//! the joints' names.

use std::path::Path;

use glam::{Mat4, Quat, Vec3};
use pyreal_dat::RetailDatStore;
use pyreal_primitives::DataId;
use pyreal_scene::art_pack::gltf::Material;
use pyreal_scene::art_pack::skin::{Joint, Rig, RigNode, SkinModel, SkinPrimitive, UPRIGHT};

use crate::model::{posed, Models};
use crate::tally::ModelId;

/// A part's frame in glTF's axes.
fn rig_frame(f: &pyreal_primitives::Frame) -> Mat4 {
    let q = Quat::from_xyzw(f.rotation.x, f.rotation.y, f.rotation.z, f.rotation.w);
    let q = if q.length_squared() > 0.0 {
        q.normalize()
    } else {
        Quat::IDENTITY
    };
    let world = Mat4::from_rotation_translation(q, Vec3::new(f.origin.x, f.origin.y, f.origin.z));
    UPRIGHT.inverse() * world * UPRIGHT
}

/// A world point or direction in glTF's axes.
fn to_rig(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], v[2], -v[1])
}

/// The body of setup `setup`: a joint for each part at its resting placement, and each part's
/// triangles on its joint, one primitive a part, in a plain grey.
///
/// # Errors
/// The setup will not read, or has no parts.
pub fn body(store: &RetailDatStore, setup: DataId) -> Result<SkinModel, String> {
    let parts = pyreal_client_runtime::models::resolve_parts(store, setup);
    if parts.is_empty() {
        return Err(format!("{:#010X}: no setup with parts", setup.0));
    }
    let mut models = Models::new(store);
    let mut nodes = Vec::with_capacity(parts.len());
    let mut joints = Vec::with_capacity(parts.len());
    let mut primitives = Vec::new();
    for (k, part) in parts.iter().enumerate() {
        let k16 = u16::try_from(k).map_err(|_| "too many parts".to_owned())?;
        let at = rig_frame(&part.frame);
        let (scale, rotation, translation) = at.to_scale_rotation_translation();
        nodes.push(RigNode {
            name: format!("part_{k:02}"),
            parent: None,
            translation,
            rotation,
            scale,
        });
        joints.push(Joint {
            node: k16,
            inverse_bind: at.inverse(),
            part: Some(k16),
        });
        let Some(mesh) = models.mesh(ModelId::Object(part.gfxobj), &[]) else {
            continue;
        };
        let mut p = SkinPrimitive {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            joints: Vec::new(),
            weights: Vec::new(),
            indices: Vec::new(),
            material: Material {
                base_color: [0.55, 0.55, 0.55, 1.0],
                metallic: 0.0,
                roughness: 0.7,
                ..Material::default()
            },
        };
        for g in mesh.groups {
            let g = posed(g, &part.frame, part.scale);
            for t in &g.triangles {
                for v in t {
                    p.indices
                        .push(u32::try_from(p.positions.len()).unwrap_or(0));
                    p.positions.push(to_rig(v.position));
                    p.normals.push(to_rig(v.normal).normalize_or_zero());
                    p.uvs.push(v.uv);
                    p.joints.push([k16, 0, 0, 0]);
                    p.weights.push([1.0, 0.0, 0.0, 0.0]);
                }
            }
        }
        if !p.indices.is_empty() {
            primitives.push(p);
        }
    }
    Ok(SkinModel {
        rig: Rig { nodes, joints },
        primitives,
        clips: Vec::new(),
    })
}

/// The manifest of a pack drawing `setup` as the body in `file`, with `clips`.
#[must_use]
pub fn manifest(setup: DataId, file: &str, clips: &[(&str, &str)]) -> String {
    let clips = if clips.is_empty() {
        String::new()
    } else {
        let list: Vec<String> = clips.iter().map(|(s, n)| format!("{s} = {n:?}")).collect();
        format!("clips = {{ {} }}\n", list.join(", "))
    };
    format!(
        "[pack]\nname = \"Body {id:#010X}\"\nformat = 1\n\n[[skin]]\nsetup = \"{id:#010X}\"\n\
         file = \"{file}\"\n{clips}author = \"pyreal-art\"\ntool = \"pyreal-art skeleton\"\n\
         license = \"CC0-1.0\"\n",
        id = setup.0
    )
}

/// Writes `model` as `body.gltf` and `body.bin` in `out`, and a manifest drawing `setup` as it
/// with `clips`.
///
/// # Errors
/// A file that will not write.
pub fn write(
    out: &Path,
    setup: DataId,
    model: &SkinModel,
    clips: &[(&str, &str)],
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let (json, bin) = pyreal_scene::art_pack::skin::write_gltf(model, "body.bin");
    let put = |name: &str, bytes: &[u8]| {
        let path = out.join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    put("body.gltf", json.as_bytes())?;
    put("body.bin", &bin)?;
    put(
        pyreal_scene::art_pack::MANIFEST_FILE,
        manifest(setup, "body.gltf", clips).as_bytes(),
    )
}
