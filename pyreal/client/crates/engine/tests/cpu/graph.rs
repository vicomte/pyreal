//! The frame graph holds together, and a broken one is refused.

use pyreal_engine::graph::{GraphDecl, GraphError, PassDecl, PassKind};
use pyreal_engine::renderer::graph_decl;

/// The engine's own graph: the sky's tables, the ground's depth, its pyramid, cull, the cover,
/// G-buffer, the sun's cascades culled and drawn, the weather's shelter map, the lamps' clusters
/// and shadows, occlusion gathered and blurred, probes, the water's ripples, the clouds, lighting,
/// the water's depth, the water, its depth merged into the world's, the rain's puddles, the ice,
/// translucency (sorted, or order-independent and
/// laid over), the rain and snow, the air's froxels and their light laid over, exposure, bloom,
/// tone map, the interface's marks in the world, the temporal resolve or upscale and the
/// composite with the UI, in that order.
#[test]
fn the_engines_frame_graph_validates() {
    let g = graph_decl();
    assert_eq!(g.validate(), Ok(()));
    let names: Vec<&str> = g.passes.iter().map(|p| p.name).collect();
    assert_eq!(
        names,
        [
            "atmosphere",
            "prepass",
            "hiz",
            "cull",
            "cover",
            "gbuffer",
            "shadow-cull",
            "shadows",
            "shelter",
            "trails",
            "clusters",
            "lamp-shadows",
            "gtao",
            "ao gather",
            "ao blur",
            "probes",
            "ripples",
            "clouds",
            "lighting",
            "water depth",
            "water",
            "water merge",
            "puddles",
            "ice",
            "translucent",
            "translucent objects",
            "oit",
            "weather",
            "air",
            "air apply",
            "exposure",
            "bloom",
            "tonemap",
            "world marks",
            "resolve",
            "composite"
        ]
    );
}

fn decl(passes: Vec<PassDecl>) -> GraphDecl {
    GraphDecl {
        passes,
        imported: vec!["scene"],
        exported: vec!["out"],
    }
}

#[test]
fn a_read_nothing_wrote_is_refused() {
    let g = decl(vec![
        PassDecl::new("a", PassKind::Render)
            .reads(&["depth"])
            .writes(&["out"]),
        PassDecl::new("b", PassKind::Render).writes(&["depth"]),
    ]);
    assert_eq!(
        g.validate(),
        Err(GraphError::Unwritten {
            pass: "a",
            resource: "depth"
        })
    );
}

#[test]
fn duplicate_passes_and_unread_writes_are_refused() {
    let dup = decl(vec![
        PassDecl::new("a", PassKind::Compute)
            .reads(&["scene"])
            .writes(&["out"]),
        PassDecl::new("a", PassKind::Render),
    ]);
    assert_eq!(dup.validate(), Err(GraphError::DuplicatePass("a")));
    let unread = decl(vec![PassDecl::new("a", PassKind::Compute)
        .reads(&["scene"])
        .writes(&["junk", "out"])]);
    assert_eq!(unread.validate(), Err(GraphError::Unread("junk")));
}
