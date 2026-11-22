//! What the drawn world is built from: `SceneConfig`, one switch per command-line option.
//!
//! Plain data. The presentation that builds the world reads it, and the application keeps the
//! one it built from so the world can be built again on the next world entry.

use pyreal_primitives::CellId;

use pyreal_world_data::landblock::DEFAULT_LANDBLOCK;

/// What the scene is built from. Every field has a command-line switch; the defaults put a
/// fixed camera over Holtburg, which is what the headless capture regression-tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneConfig {
    /// The landblock the camera starts in, and the centre of the window.
    pub landblock: u16,
    /// An interior cell of [`Self::landblock`] to stand the offline body in, found
    /// through the cell's own `cell_bsp` so that its containment test accepts the point.
    /// `--start-cell`. Ignored when the server says where the
    /// player is, which is every connected run.
    pub start_cell: Option<CellId>,
    /// Give the interior cells' baked objects — a dungeon's tables, chairs,
    /// bookshelves and braziers — their bodies and their triangles. Always on in the client
    /// without a runtime switch; this switch exists for
    /// exactly the reason [`Self::particles`]' does, because the only honest evidence
    /// that the furniture is on screen and in the way is a differential against the same
    /// frame and the same walk without it.
    pub cell_statics: bool,
    /// Load each setup's **parts** and their graphics-object physics BSPs, so that
    /// object collision tests can traverse every part's own tree. Always on in the
    /// client: the presence of a physics BSP in the parts selects that collision branch.
    /// The switch exists for the reason [`Self::cell_statics`]'
    /// does — clearing it loads no parts, so the scan answers no and every object falls to its
    /// spheres, and that is the control the differential needs. `--no-mesh-collision`.
    pub mesh_collision: bool,
    /// Radius of the meshed landblock window: `mid_width = 2r+1` blocks are meshed.
    pub land_radius: u32,
    /// How many rings of blocks grow scenery, buildings and static objects. Terrain is cheap
    /// and re-emitted per frame; objects are baked into world-space batches, so this is the
    /// knob that costs memory.
    pub scenery_radius: u32,
    /// How long the drawn world's `stream` may spend building landblocks in one frame. `None` builds
    /// everything queued before returning, which is what the headless captures and the tests
    /// rely on; the connected client sets a budget so that a teleport or login keeps drawing
    /// the portal tunnel while the window fills in, nearest blocks first, over later frames.
    pub stream_budget: Option<std::time::Duration>,
    /// The `Render.*` preferences and the `[Fidelity]` section.
    ///
    /// It is on the config rather than on the scene because the drawn world's `cfg` is public and
    /// the options page has to be able to change these between frames -- which is exactly the
    /// shape of the client's own poll: it re-reads the rendering preferences every frame
    /// and acts only on what changed.
    pub render: crate::render_prefs::RenderPreferences,
    /// Metres above the block's highest vertex the default camera sits. Only used without a
    /// body; with one the camera follows it.
    pub camera_height: f32,
    /// Put a character in the scene. `--no-character` turns it off, which gives a free camera
    /// over the static scene.
    pub character: bool,
    /// Where in the in-game day the session starts, `[0, 1)`. `None` runs the clock
    /// alone, which is what the client does with `GameTime.TimeZeroDelta`
    /// at its default 0.
    pub time_of_day: Option<f32>,
    /// Where in the Dereth *calendar* the session starts, as the absolute
    /// second a `TimeSync` would have put in. Overrides
    /// [`Self::time_of_day`], because it decides the hour too.
    ///
    /// The day fraction is not the whole environment: the sky calendar hashes
    /// `(current_year * days_per_year + current_day)` into one of twenty day groups,
    /// which carry different time-of-day colour ramps at the same hour. A
    /// headless scene asked for fraction 0.1814 through `time_of_day` lands on year 10 day 1,
    /// group 5 ("Sunny"), whose Foredawn `amb_color` is (177, 177, 255) — green equal to red.
    /// A recorded retail capture was taken at absolute second 303_449_042, which is
    /// year 120 day 223, group 15 ("Rainy"), `amb_color` (191, 134, 255). That difference,
    /// and not the interpolation, is the "night colour cast" measured against it.
    ///
    /// See the sky clock's `set_game_time`.
    pub game_time: Option<f64>,
    /// World fog is enabled by default. Clearing this disables fog and skips
    /// both the world-fog lookup and the update of its rendering properties.
    /// It exists because the only honest evidence that the far
    /// terrain is fogged is the same frame with the fog off.
    pub world_fog: bool,
    /// The three `Camera.*` preferences, applied to the body's camera when it is attached.
    /// See [`crate::camera::CameraPreferences`].
    pub camera: crate::camera::CameraPreferences,
    /// The four `Input.*` mouse-look preferences, applied with them.
    pub mouse_look: crate::actions::camera::MouseLookPreferences,
    /// Run and draw the particle emitters. Always on in the client; the switch
    /// exists because the only honest evidence that particles are on screen is a differential
    /// against the same frame with them off, and that differential has to come from the app's
    /// own update path rather than from a second, differently-built scene.
    pub particles: bool,
    /// Draw a part that carries a cloned material through that material's
    /// alpha, which is `1 - t` for translucency `t`.
    ///
    /// Not the surface's dat-authored `TRANSLUCENT` alpha: this is
    /// the object's runtime override, driven by the chase camera's body fade,
    /// an animation's transparency hook or the server's translucency update.
    ///
    /// Always on in the client, which has no switch. Clearing it ignores the material alpha,
    /// which is byte for byte the frame the same parts produce with no material at all; that
    /// is the control the differential needs.
    pub material_translucency: bool,
    /// Release a block's visible interior cells and their static objects
    /// when it scrolls off the window,
    /// instead of leaving its interior cells, their walls and their furniture resident for the
    /// life of the session.
    ///
    /// Always on in the client: every block pushed off an edge is released unconditionally.
    /// Clearing it keeps the interiors resident — the block's
    /// draw data is dropped by the same `retain` either way, and nothing else is — which is
    /// the control the leak differential needs, and the only honest way to state a leak is
    /// against the same walk in the same binary with the release off.
    pub release_interiors: bool,
    /// Carry the server's `PhysicsState` word to the *parts* —
    /// applying the `NODRAW_PS` and `HIDDEN_PS` reactions to the part array.
    ///
    /// Always on in the client: the three state-bit tests run unconditionally, including on
    /// every create. Clearing it still delivers the state word to the *body*, but
    /// `StateSideEffects` is discarded before it reaches a part — which is the control the
    /// differential needs.
    ///
    /// **This does not hide a `HIDDEN_PS` object**, and that is not an omission. Nothing in
    /// the client's draw path reads `HIDDEN_PS`: it walks the cell's shadow-part list
    /// with no state test, and setting hidden never touches
    /// its own part array. What `HIDDEN_PS` does is force `NODRAW_PS` onto the object's
    /// **children**, and that is what this switch turns on. The hidden-and-no-draw tests cover
    /// both.
    pub object_state_draw: bool,
    /// An indoor viewer whose cell has **not** `seen_outside`
    /// reaches neither landblock-window update,
    /// so the landblock window does not move. See the drawn world's `viewpoint_block`.
    ///
    /// **This is the switch between the two *derivations* of the window's block.** Set, the
    /// window's block is the landblock of the **cell id** supplied by the normal rendering
    /// path; clear, it is `floor(origin / 192)` applied to the viewpoint. A third arm bolted
    /// onto the origin arithmetic rather than replacing it lets a teleport into a dungeon slip
    /// between the two — see the drawn world's `viewpoint_block` and the indoor-draw tests.
    ///
    /// Always on in the client, which has no switch. Clearing it re-centres the window from
    /// the viewpoint's **origin** on every frame, which is the control the differential needs.
    /// That walks the window off a dungeon's own
    /// landblock, because a dungeon's cell frames are its own layout space and 87 % of the
    /// retail dat's interior cells sit outside their block's `[0, 192]²`; the block then
    /// stops being resident and the drawn world's `draw_inside` draws nothing at all.
    pub indoor_viewpoint_gate: bool,
    /// Require `side_cell_count == 8` for a block's
    /// **static objects and buildings**, not only on its scenery.
    ///
    /// Static-object and building initialization both return unless this count is eight.
    /// A change away from full detail destroys all three populations. So a landblock on an
    /// outer LOD ring is bare terrain in the client.
    ///
    /// Always on in the client, which has no switch. Clearing it leaves the guard on
    /// `generate_scenery` alone, and that is the control the LOD object-guard differential is
    /// taken against.
    ///
    /// **It changes counts and a bake, and no pixel.** It is reachable only at
    /// `scenery_radius >= 2` (rings 0 and 1 are both full detail and the shipped
    /// `scenery_radius` is 1), and even there the objects it stops baking were assembled and
    /// never submitted: a coarse ring begins 288 m out and the degrade record's terminator has
    /// already taken them. The test measures that rather than assuming it.
    pub lod_object_guard: bool,
    /// Whether the verdicts of which records two eras' files share as the same object are kept
    /// between runs, in the host's cache folder
    /// ([`crate::object_identity::default_cache_dir`]); `false` works them out in memory each
    /// time the other era's look is first drawn.
    pub object_identity_cache: bool,
    /// How much of each frame the application gives those verdicts, working them out in the
    /// background from start-up and handing them to the scene when they are ready
    /// ([`crate::object_identity::IdentityBuild`]). Until then the objects keep the look they
    /// have, and a look asked for is drawn once they arrive. `None`: the scene works them out
    /// itself, at once, the first time the other era's look is drawn.
    pub object_identity_budget: Option<std::time::Duration>,
}

impl Default for SceneConfig {
    fn default() -> Self {
        Self {
            landblock: DEFAULT_LANDBLOCK,
            start_cell: None,
            cell_statics: true,
            mesh_collision: true,
            // Quality preset 1 (the presets select radii 3, 5, 8, 11,
            // 15). The landscape constructor default is 5 = 121 blocks, which is 2.5x the texture
            // merges and 2.5x the draws for a slice that only has to be looked at.
            //
            // **This is the bench default, not the client's.** The client
            // takes its window from `Render.LandscapeDrawDistance`, whose registered default
            // is the Medium preset's — see [`crate::config::Config::land_radius`]. The 3 here is kept
            // because a test wants the cheapest window that still has rings in it, and every
            // suite in the tree that names a radius names its own; a test that wants the
            // client's answer asks `App::config()` for it, as a dozen of them do.
            land_radius: 3,
            scenery_radius: 1,
            stream_budget: None,
            render: crate::render_prefs::RenderPreferences::default(),
            camera_height: 45.0,
            character: true,
            time_of_day: None,
            game_time: None,
            // World fog is enabled by default.
            world_fog: true,
            camera: crate::camera::CameraPreferences::default(),
            mouse_look: crate::actions::camera::MouseLookPreferences::default(),
            particles: true,
            material_translucency: true,
            release_interiors: true,
            object_state_draw: true,
            indoor_viewpoint_gate: true,
            lod_object_guard: true,
            object_identity_cache: false,
            object_identity_budget: None,
        }
    }
}
