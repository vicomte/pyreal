//! Draws a place with the engine, headless to a PNG or in a window with a free camera, and
//! reports each pass's GPU time and what was drawn.
//!
//! **Depends on** `pyreal-engine` (everything it draws), `pyreal-dat` (opening the dats),
//! `pyreal-primitives` (the ids of objects a report names), `pyreal-world-data` (the region and
//! the animation assets), `pyreal-animation` (motion commands), and `pyreal-chargen` with
//! `pyreal-assets` (a character dressed as creation dresses one). **Used by** nothing; people run
//! it to work on the engine.
//!
//! **Must never** be depended on by the client: it is a development viewer.
//!
//! ```text
//! engine-view --dat-dir <dir> [--later-dat-dir <dir>] [--overlay <dir>] [--block 0xA9B4 --at X,Y[,Z] --heading DEG --pitch DEG]
//!             [--report "<report line>" | --library <file> --name <place>]
//!             [--objects SETUP,BLOCK,X,Y;...]
//!             [--time 0.5] [--size 1280x720] [--radius 4] [--fov DEG] [--exposure 1.0]
//!             [--ev STOPS] [--retail] [--no-bloom] [--rain] [--storm] [--snow] [--blizzard] [--no-shadows] [--shadow-distance M] [--no-lamps] [--no-ao] [--clouds 0-3]
//!             [--no-bounce] [--no-lamp-rays] [--god-rays 0-3] [--no-water] [--water-detail 0-2] [--no-cover] [--cover-reach 1.0] [--tone neutral|aces|agx]
//!             [--no-surfaces] [--no-relief] [--relief 0-3] [--no-height-blend] [--no-triplanar] [--no-detail]
//!             [--no-macro]
//!             [--day-group N] [--clock SECONDS] [--frames 8] [--out place.png] [--window]
//!             [--walk] [--no-cut] [--no-hiz] [--lod-bias 1.0] [--cards-from 1.0] [--far BLOCKS] [--fog-end METRES]
//!             [--object SETUP[/MOTION_TABLE][*SCALE][@X,Y[,Z]]] [--character male|female[@X,Y[,Z]]]
//!             [--stance HandCombat] [--motion WalkForward] [--time-in-anim SECONDS] [--facing DEG]
//!             [--dress CLOTHING,TEMPLATE,SHADE[,PALETTE]]... [--wear CLOTHING,TEMPLATE,SHADE]... [--translucency T]
//!             [--no-aa] [--upscaler metalfx|temporal|spatial|plain] [--render-scale 1.0]
//!             [--budget MS] [--ui overlay.png] [--select X,Y] [--hover X,Y]
//!             [--no-particles] [--no-flames] [--oit] [--script SCRIPT@X,Y[,Z]] [--body-script SCRIPT@SECONDS] [--no-webs] [--no-volumes] [--retail-portals] [--tunnel SECONDS] [--retail-beacons] [--retail-falls] [--retail-fountains] [--no-wind] [--stride VX,VY]
//!             [--turn DEG] [--orbit DEG] [--save-every N] [--set-time FRAME:TIME] [--moon-phase REZ,ALB]
//! ```
//!
//! Scripted objects run their default scripts, and their emitters' particles are drawn;
//! `--script` plays a script (a spell's effect) at a block-local place, `--body-script` on the
//! asked-for bodies that far into the clip; `--oit` draws the
//! translucent surfaces and particles order-independently rather than sorted.
//!
//! The world is drawn at `--render-scale` of the output (or at what holds `--budget` ms of GPU
//! time a frame) and rebuilt at the output's size by the first upscaler that runs from
//! `--upscaler` on; `--ui` lays a picture over the output as the UI. `--select` and `--hover`
//! pick the object under an output pixel after the first frame and highlight it, a body with its
//! target ring.
//!
//! The place is a landblock and a block-local position (the height defaults to the ground plus
//! two metres), a compass heading and a pitch, or a report line's camera (the format of
//! `pyreal/client/tests/gpu/report-library.txt`), with the cell it names: a room or a dungeon's
//! cell puts the eye inside, where the portal walk starts. A library place's objects (its `doors`
//! or `setup,block,x,y` entries, the ones a server would make) are placed on the ground. The dats are a folder of end-of-retail files or
//! of the older `portal.dat` and `cell.dat`. In a window, WASD and Q/E move, the right mouse
//! button looks, Shift is faster, and a left click selects what is under the cursor.
//!
//! Bodies are animated: a library place's setups, each `--object` (a setup, with the motion table
//! it moves by when not its own and the scale its weenie gives it) and each `--character` (an Aluvian dressed as character creation
//! dresses one) stand where they are put, block-local in the view's block, or in a row in front
//! of the camera, facing it (or `--facing`, a compass heading), in `--stance` doing `--motion`,
//! `--time-in-anim` seconds into it; `--dress` (repeated, each worn over the last), `--wear` (the
//! items it wears, which cover its own dress and leave the setup's own parts elsewhere) and
//! `--translucency` describe them as a server describes a creature.

mod bodies;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use pyreal_dat::RetailDatStore;
use pyreal_engine::body::Body;
use pyreal_engine::cells::OUTSIDE;
use pyreal_engine::renderer::{Calibration, Exposure, ToneCurve};
use pyreal_engine::world::{ground_height, load_area, load_far, load_far_trees, Area, AssetLoader};
use pyreal_engine::{Camera, Engine, Gpu, PoolSizes, SkyLight};
use pyreal_primitives::DataId;

/// A block-local place: x, y and the height (the ground's when not given).
type Place = (f32, f32, Option<f32>);

#[derive(Debug, Clone)]
struct Args {
    dat_dir: PathBuf,
    /// End of retail's files beside an older world's `dat_dir`, as the client opens them.
    later_dat_dir: Option<PathBuf>,
    /// A world's overlay folder laid over the dats.
    overlay: Option<PathBuf>,
    block: u16,
    at: (f32, f32, Option<f32>),
    heading_deg: f32,
    pitch_deg: f32,
    time: f32,
    size: (u32, u32),
    radius: i32,
    fov_deg: f32,
    /// A fixed exposure, in place of the eye's adaptation.
    exposure: Option<f32>,
    /// Stops over the auto-exposure's key.
    ev: f32,
    /// The client's own light, for comparing brightness.
    retail: bool,
    no_bloom: bool,
    /// Rain falling.
    rain: bool,
    /// Rain in a storm.
    storm: bool,
    /// Rain or snow by the land under it, and the same in a storm (a blizzard).
    snow: bool,
    blizzard: bool,
    no_shadows: bool,
    /// How far the sun's shadows reach, metres; the renderer's own when `None`.
    shadow_distance: Option<f32>,
    /// Retail's water picture rather than the water pass.
    no_water: bool,
    /// Water Detail's stored level (0 Low, 1 Medium, 2 High).
    water_detail: u32,
    /// No ground cover; and the share of its reach.
    no_cover: bool,
    /// The surfaces' depth (5h), each part on or off.
    surfaces: pyreal_engine::renderer::Surfaces,
    cover_reach: f32,
    no_lamps: bool,
    no_ao: bool,
    /// Cloud Detail: 0 the dats' flat deck, 1 to 3 the volumetric clouds.
    clouds: u32,
    no_bounce: bool,
    /// The nearest lamps' shadows left untraced.
    no_lamp_rays: bool,
    /// The God Rays level, 0 (off) to 3.
    god_rays: u32,
    tone: ToneCurve,
    frames: usize,
    out: Option<PathBuf>,
    window: bool,
    /// A report line's yaw, in the client's radians, which wins over the heading.
    yaw: Option<f32>,
    /// A report's horizontal field of view, which wins over the vertical one unless `--fov` names it.
    hfov_deg: Option<f32>,
    /// The game clock in seconds, which names the time of day and the day group.
    clock: Option<f64>,
    time_given: Option<f32>,
    group_given: Option<usize>,
    /// The dat cell a report's camera stood in.
    cell: Option<u32>,
    /// Objects to place: setup, block and block-local x and y.
    objects: Vec<(u32, u16, f32, f32)>,
    /// Print every cell the portal walk saw, with its screen rectangle.
    walk: bool,
    /// Leave the ground whole inside rooms.
    no_cut: bool,
    /// Cull without the ground's depth pyramid.
    no_hiz: bool,
    /// Draws the original level of a picture `client_highres.dat` has a sharper one of.
    no_high_res: bool,
    /// Ground Coloring: Throne of Destiny's colours rather than Classic.
    tod_ground: bool,
    /// How far each degrade level is kept, as a multiple of the dats' distances (0: never degrade).
    lod_bias: f32,
    /// Where the pack's levels of pictures take over (`AssetLoader::cards_from`); zero for never.
    cards_from: Option<f32>,
    /// Blocks out the far terrain reaches; zero for none.
    far: i32,
    /// Where the fog is whole, in place of the region's (to look at distant ground).
    fog_end: Option<f32>,
    /// Animated bodies to place.
    bodies: Vec<bodies::Spec>,
    stance: Option<pyreal_animation::MotionCommand>,
    motion: Option<pyreal_animation::MotionCommand>,
    /// Seconds into the motion the bodies are drawn at.
    time_in_anim: f64,
    /// The bodies' compass heading, in place of facing the camera.
    facing_deg: Option<f32>,
    /// The asked-for setups' dress, as a server describes a creature, each layer over the last.
    dress: Vec<bodies::Dress>,
    /// The asked-for setups' worn items, as a server dresses a creature in its equipment.
    worn: Vec<bodies::Dress>,
    /// The asked-for bodies' physics-description translucency (a shadow's 0.5).
    translucency: Option<f32>,
    /// The art pack drawn over the dats.
    art_pack: Option<PathBuf>,
    /// The base pack the art pack is drawn over, as the client draws its built-in one.
    base_pack: Option<PathBuf>,
    /// The temporal anti-aliasing off.
    no_aa: bool,
    /// No particles; translucency drawn order-independently.
    no_particles: bool,
    /// Objects' fire drawn as their retail particles, not drawn flames.
    no_flames: bool,
    /// Cobwebs drawn as the dats' cards, not silk.
    no_webs: bool,
    /// Smoke, mist and creatures' liquid, vapour, glow, frost and lightning drawn as their
    /// particles' cards, as retail draws them.
    no_volumes: bool,
    /// Portals drawn as retail draws their swirl, not as a vortex drawing in dust.
    retail_portals: bool,
    /// The lighthouses' beacons drawn as retail's added pictures, not a magical white light.
    retail_beacons: bool,
    /// Retail's waterfall cards, not the falling sheets.
    retail_falls: bool,
    /// Retail's fountains: their basins' picture and their bubbles.
    retail_fountains: bool,
    /// No wind: the grass, shrubs and trees stand still.
    no_wind: bool,
    /// Every body walked at this velocity, metres a second east and north.
    stride: Option<(f32, f32)>,
    /// The camera turned this many degrees a frame, clockwise.
    turn: f32,
    /// The camera carried round the first body this many degrees a frame, anticlockwise, turning
    /// as it goes so the body stays where it was in view.
    orbit: f32,
    /// Seconds into portal space to draw in place of the view, in the first portal's colour.
    tunnel: Option<f32>,
    /// How much of Rez'arel's and Alb'arel's faces are lit, 0 new to 1 full.
    moon_phase: Option<(f32, f32)>,
    /// Every this many frames written beside `--out`, numbered.
    save_every: usize,
    /// A frame at which the time of day is set, as `@tod` sets it, and the time.
    set_time: Option<(usize, f32)>,
    oit: bool,
    /// Scripts played where they are put (a spell's effect): script and block-local place.
    scripts: Vec<(u32, Place)>,
    /// Scripts played on the asked-for bodies (a spell's cast): script and the clip's second.
    body_scripts: Vec<(u32, f64)>,
    /// The upscaler tried first.
    upscaler: Option<pyreal_engine::temporal::Upscaler>,
    /// The share of the output's size the world is drawn at.
    render_scale: f32,
    /// A GPU frame budget, ms, held by the dynamic resolution.
    budget: Option<f64>,
    /// A picture laid over the output as the UI.
    ui: Option<PathBuf>,
    /// Output pixels whose object is selected, and hovered, after the first frame.
    select: Option<(u32, u32)>,
    hover: Option<(u32, u32)>,
}

impl Args {
    /// The vertical field of view in degrees: a report's horizontal one at this size's aspect,
    /// as the client draws it, else `--fov`.
    // Presentation only: the camera's angle, not anything the game computes.
    #[allow(clippy::disallowed_methods, clippy::cast_precision_loss)]
    fn vertical_fov(&self) -> f32 {
        let aspect = self.size.0 as f32 / self.size.1.max(1) as f32;
        self.hfov_deg.map_or(self.fov_deg, |h| {
            (2.0 * ((h.to_radians() * 0.5).tan() / aspect).atan()).to_degrees()
        })
    }
}

impl Default for Args {
    fn default() -> Self {
        Self {
            dat_dir: PathBuf::new(),
            later_dat_dir: None,
            overlay: None,
            block: 0xA9B4,
            at: (96.0, 96.0, None),
            heading_deg: 0.0,
            pitch_deg: -10.0,
            time: 0.5,
            size: (1280, 720),
            radius: 4,
            fov_deg: 60.0,
            exposure: None,
            ev: 0.0,
            retail: false,
            no_bloom: false,
            rain: false,
            storm: false,
            snow: false,
            blizzard: false,
            no_shadows: false,
            shadow_distance: None,
            no_water: false,
            water_detail: 1,
            no_cover: false,
            surfaces: pyreal_engine::renderer::Surfaces::ALL,
            cover_reach: 1.0,
            no_lamps: false,
            no_ao: false,
            clouds: 2,
            no_bounce: false,
            no_lamp_rays: false,
            god_rays: 0,
            tone: ToneCurve::Neutral,
            frames: 8,
            out: None,
            window: false,
            yaw: None,
            hfov_deg: None,
            clock: None,
            time_given: None,
            group_given: None,
            cell: None,
            objects: Vec::new(),
            walk: false,
            no_cut: false,
            no_hiz: false,
            no_high_res: false,
            tod_ground: false,
            lod_bias: 1.0,
            cards_from: None,
            far: 24,
            fog_end: None,
            bodies: Vec::new(),
            stance: None,
            motion: None,
            time_in_anim: 0.5,
            facing_deg: None,
            dress: Vec::new(),
            worn: Vec::new(),
            translucency: None,
            art_pack: None,
            base_pack: None,
            no_aa: false,
            no_particles: false,
            no_flames: false,
            no_webs: false,
            no_volumes: false,
            retail_portals: false,
            retail_beacons: false,
            retail_falls: false,
            retail_fountains: false,
            no_wind: false,
            stride: None,
            turn: 0.0,
            orbit: 0.0,
            tunnel: None,
            moon_phase: None,
            save_every: 0,
            set_time: None,
            oit: false,
            scripts: Vec::new(),
            body_scripts: Vec::new(),
            upscaler: None,
            render_scale: 1.0,
            budget: None,
            ui: None,
            select: None,
            hover: None,
        }
    }
}

fn hex_u16(s: &str) -> Result<u16, String> {
    let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    let t = &t[..t.len().min(4)];
    u16::from_str_radix(t, 16).map_err(|e| format!("{s}: {e}"))
}

fn pixel(s: &str) -> Result<(u32, u32), String> {
    let (x, y) = s.split_once(',').ok_or(format!("{s}: give X,Y"))?;
    let n = |v: &str| v.trim().parse::<u32>().map_err(|e| format!("{v}: {e}"));
    Ok((n(x)?, n(y)?))
}

fn hex_u32(s: &str) -> Result<u32, String> {
    let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(t, 16).map_err(|e| format!("{s}: {e}"))
}

/// `setup,block,x,y` entries, `;` between them.
fn objects(spec: &str) -> Result<Vec<(u32, u16, f32, f32)>, String> {
    spec.split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(|e| {
            let f: Vec<&str> = e.split(',').collect();
            if f.len() != 4 {
                return Err(format!("{e}: an object is setup,block,x,y"));
            }
            Ok((hex_u32(f[0])?, hex_u16(f[1])?, num(f[2])?, num(f[3])?))
        })
        .collect()
}

fn num(s: &str) -> Result<f32, String> {
    s.trim().parse::<f32>().map_err(|e| format!("{s}: {e}"))
}

/// The camera, time and field of view of a report line.
fn apply_report(a: &mut Args, line: &str) -> Result<(), String> {
    let cam = line
        .split(';')
        .map(str::trim)
        .find(|f| f.starts_with("camera "))
        .ok_or("the report line has no camera")?;
    let mut words = cam.split_whitespace().skip(1);
    a.block = hex_u16(words.next().ok_or("no block")?)?;
    let rest: Vec<&str> = words.collect();
    let joined = rest.join(" ");
    let open = joined.find('[').ok_or("no position")?;
    let close = joined.find(']').ok_or("no position")?;
    let p: Vec<f32> = joined[open + 1..close]
        .split_whitespace()
        .map(num)
        .collect::<Result<_, _>>()?;
    if p.len() != 3 {
        return Err("the position is not three numbers".into());
    }
    a.at = (p[0], p[1], Some(p[2]));
    let after: Vec<&str> = joined[close + 1..].split_whitespace().collect();
    for w in after.windows(2) {
        match w[0] {
            "yaw" => a.yaw = Some(num(w[1])?),
            "pitch" => a.pitch_deg = num(w[1])?.to_degrees(),
            "cell" => a.cell = Some(hex_u32(w[1])?),
            _ => {}
        }
    }
    for field in line.split(';').map(str::trim) {
        let w: Vec<&str> = field.split_whitespace().collect();
        match w.as_slice() {
            ["time", t, ..] => a.time = num(t)?,
            ["clock", c] => a.clock = Some(c.parse().map_err(|_| "the report's clock")?),
            ["fov", h, ..] => a.hfov_deg = Some(num(h)?),
            _ => {}
        }
    }
    Ok(())
}

/// What `--help` prints.
const USAGE: &str =
    "engine-view --dat-dir <dir> [--later-dat-dir <dir>] [--overlay <dir>] [--block 0xA9B4 --at X,Y[,Z] --heading DEG --pitch DEG]
            [--report \"<report line>\" | --library <file> --name <place>]
            [--objects SETUP,BLOCK,X,Y;...] [--art-pack <file>] [--base-pack <file>]
            [--object SETUP[/MOTION_TABLE][*SCALE][@X,Y[,Z]]] [--character male|female[@X,Y[,Z]]]
            [--stance HandCombat] [--motion WalkForward] [--time-in-anim SECONDS] [--facing DEG]
            [--dress CLOTHING,TEMPLATE,SHADE[,PALETTE]]... [--wear CLOTHING,TEMPLATE,SHADE]... [--translucency T]
            [--time 0.5] [--size 1280x720] [--radius 4] [--fov DEG] [--exposure 1.0]
            [--ev STOPS] [--retail] [--no-bloom] [--rain] [--storm] [--snow] [--blizzard] [--no-shadows] [--shadow-distance M] [--no-lamps] [--no-ao] [--clouds 0-3]
            [--no-bounce] [--no-lamp-rays] [--god-rays 0-3] [--no-water] [--water-detail 0-2] [--no-cover] [--cover-reach 1.0] [--tone neutral|aces|agx] [--day-group N] [--clock SECONDS] [--frames 8]
            [--no-surfaces] [--no-relief] [--relief 0-3] [--no-height-blend] [--no-triplanar] [--no-detail] [--no-macro]
            [--out place.png] [--window] [--walk] [--no-cut] [--no-hiz] [--no-high-res] [--ground-coloring classic|tod] [--lod-bias 1.0] [--cards-from 1.0]
            [--far BLOCKS] [--fog-end METRES]
            [--no-aa] [--upscaler metalfx|temporal|spatial|plain] [--render-scale 1.0] [--budget MS]
            [--ui overlay.png] [--select X,Y] [--hover X,Y] [--no-particles] [--no-flames] [--oit]
            [--script SCRIPT@X,Y[,Z]] [--body-script SCRIPT@SECONDS] [--no-webs] [--no-volumes] [--retail-portals] [--tunnel SECONDS] [--retail-beacons] [--retail-falls] [--no-wind] [--stride VX,VY]
            [--turn DEG] [--orbit DEG] [--save-every N] [--set-time FRAME:TIME] [--moon-phase REZ,ALB]";

fn parse() -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    let mut library: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let (mut time, mut fov) = (None, None);
    while let Some(arg) = it.next() {
        let mut val = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--help" | "-h" => return Err(USAGE.to_owned()),
            "--dat-dir" => a.dat_dir = PathBuf::from(val()?),
            "--later-dat-dir" => a.later_dat_dir = Some(PathBuf::from(val()?)),
            "--overlay" => a.overlay = Some(PathBuf::from(val()?)),
            "--block" => a.block = hex_u16(&val()?)?,
            "--at" => {
                let v = val()?;
                let p: Vec<&str> = v.split(',').collect();
                if p.len() < 2 {
                    return Err("--at takes X,Y or X,Y,Z".into());
                }
                a.at = (
                    num(p[0])?,
                    num(p[1])?,
                    p.get(2).map(|z| num(z)).transpose()?,
                );
            }
            "--heading" => a.heading_deg = num(&val()?)?,
            "--pitch" => a.pitch_deg = num(&val()?)?,
            "--time" => time = Some(num(&val()?)?),
            "--size" => {
                let v = val()?;
                let (w, h) = v.split_once('x').ok_or("--size takes WxH")?;
                a.size = (
                    w.parse().map_err(|_| "--size width")?,
                    h.parse().map_err(|_| "--size height")?,
                );
            }
            "--radius" => a.radius = val()?.parse().map_err(|_| "--radius")?,
            "--fov" => fov = Some(num(&val()?)?),
            "--exposure" => a.exposure = Some(num(&val()?)?),
            "--ev" => a.ev = num(&val()?)?,
            "--retail" => a.retail = true,
            "--no-bloom" => a.no_bloom = true,
            "--rain" => a.rain = true,
            "--storm" => a.storm = true,
            "--snow" => a.snow = true,
            "--blizzard" => a.blizzard = true,
            "--no-shadows" => a.no_shadows = true,
            "--shadow-distance" => a.shadow_distance = Some(num(&val()?)?),
            "--no-water" => a.no_water = true,
            "--water-detail" => {
                a.water_detail = val()?
                    .parse()
                    .map_err(|_| "--water-detail takes 0, 1 or 2".to_owned())?;
            }
            "--no-cover" => a.no_cover = true,
            "--no-surfaces" => a.surfaces = pyreal_engine::renderer::Surfaces::NONE,
            "--no-relief" => a.surfaces.relief = 0,
            "--relief" => {
                a.surfaces.relief = val()?
                    .parse()
                    .ok()
                    .filter(|l| *l <= pyreal_engine::renderer::RELIEF_HIGH)
                    .ok_or("--relief takes 0, 1, 2 or 3")?;
            }
            "--no-height-blend" => a.surfaces.height_blend = false,
            "--no-triplanar" => a.surfaces.triplanar = false,
            "--no-detail" => a.surfaces.detail = false,
            "--no-macro" => a.surfaces.macro_variation = false,
            "--cover-reach" => a.cover_reach = num(&val()?)?,
            "--no-lamps" => a.no_lamps = true,
            "--no-ao" => a.no_ao = true,
            "--clouds" => a.clouds = val()?.parse().map_err(|_| "--clouds")?,
            "--no-bounce" => a.no_bounce = true,
            "--no-lamp-rays" => a.no_lamp_rays = true,
            "--god-rays" => a.god_rays = val()?.parse().map_err(|_| "--god-rays")?,
            "--tone" => {
                a.tone = match val()?.as_str() {
                    "neutral" => ToneCurve::Neutral,
                    "aces" => ToneCurve::Aces,
                    "agx" => ToneCurve::Agx,
                    other => return Err(format!("--tone takes neutral, aces or agx, not {other}")),
                }
            }
            "--day-group" => a.group_given = Some(val()?.parse().map_err(|_| "--day-group")?),
            "--clock" => a.clock = Some(val()?.parse().map_err(|_| "--clock")?),
            "--frames" => a.frames = val()?.parse().map_err(|_| "--frames")?,
            "--out" => a.out = Some(PathBuf::from(val()?)),
            "--window" => a.window = true,
            "--walk" => a.walk = true,
            "--no-cut" => a.no_cut = true,
            "--no-hiz" => a.no_hiz = true,
            "--no-high-res" => a.no_high_res = true,
            "--ground-coloring" => {
                a.tod_ground = match val()?.to_ascii_lowercase().as_str() {
                    "classic" => false,
                    "tod" | "throne-of-destiny" => true,
                    other => return Err(format!("--ground-coloring {other}: classic or tod")),
                }
            }
            "--lod-bias" => a.lod_bias = num(&val()?)?,
            "--cards-from" => a.cards_from = Some(num(&val()?)?),
            "--far" => a.far = val()?.parse().map_err(|_| "--far")?,
            "--fog-end" => a.fog_end = Some(num(&val()?)?),
            "--report" => apply_report(&mut a, &val()?)?,
            "--library" => library = Some(PathBuf::from(val()?)),
            "--name" => name = Some(val()?),
            "--objects" => a.objects.extend(objects(&val()?)?),
            "--object" => a.bodies.push(bodies::object(&val()?)?),
            "--character" => a.bodies.push(bodies::character(&val()?)?),
            "--stance" => a.stance = Some(bodies::command(&val()?)?),
            "--motion" => a.motion = Some(bodies::command(&val()?)?),
            "--time-in-anim" => {
                a.time_in_anim = val()?.parse().map_err(|_| "--time-in-anim")?;
            }
            "--facing" => a.facing_deg = Some(num(&val()?)?),
            "--dress" => a.dress.push(bodies::dress(&val()?)?),
            "--wear" => a.worn.push(bodies::dress(&val()?)?),
            "--translucency" => a.translucency = Some(num(&val()?)?),
            "--art-pack" => a.art_pack = Some(PathBuf::from(val()?)),
            "--base-pack" => a.base_pack = Some(PathBuf::from(val()?)),
            "--no-aa" => a.no_aa = true,
            "--no-particles" => a.no_particles = true,
            "--no-flames" => a.no_flames = true,
            "--no-webs" => a.no_webs = true,
            "--no-volumes" => a.no_volumes = true,
            "--retail-portals" => a.retail_portals = true,
            "--retail-beacons" => a.retail_beacons = true,
            "--retail-falls" => a.retail_falls = true,
            "--retail-fountains" => a.retail_fountains = true,
            "--no-wind" => a.no_wind = true,
            "--stride" => {
                let v = val()?;
                let (x, y) = v.split_once(',').ok_or("--stride takes VX,VY")?;
                a.stride = Some((num(x)?, num(y)?));
            }
            "--turn" => a.turn = num(&val()?)?,
            "--orbit" => a.orbit = num(&val()?)?,
            "--tunnel" => a.tunnel = Some(num(&val()?)?),
            "--moon-phase" => {
                let v = val()?;
                let (r, l) = v.split_once(',').ok_or("--moon-phase takes REZ,ALB")?;
                a.moon_phase = Some((num(r)?, num(l)?));
            }
            "--save-every" => a.save_every = val()?.parse().map_err(|_| "--save-every")?,
            "--set-time" => {
                let v = val()?;
                let (f, t) = v.split_once(':').ok_or("--set-time takes FRAME:TIME")?;
                a.set_time = Some((f.parse().map_err(|_| "--set-time")?, num(t)?));
            }
            "--oit" => a.oit = true,
            "--script" => {
                let v = val()?;
                let (id, at) = v.split_once('@').ok_or("--script takes SCRIPT@X,Y[,Z]")?;
                let p: Vec<f32> = at
                    .split(',')
                    .map(|x| x.trim().parse::<f32>().map_err(|e| format!("{x}: {e}")))
                    .collect::<Result<_, _>>()?;
                let place = match p.as_slice() {
                    [x, y] => (*x, *y, None),
                    [x, y, z] => (*x, *y, Some(*z)),
                    _ => return Err("--script takes SCRIPT@X,Y[,Z]".into()),
                };
                a.scripts.push((hex_u32(id)?, place));
            }
            "--body-script" => {
                let v = val()?;
                let (id, at) = v
                    .split_once('@')
                    .ok_or("--body-script takes SCRIPT@SECONDS")?;
                a.body_scripts.push((hex_u32(id)?, f64::from(num(at)?)));
            }
            "--upscaler" => {
                let v = val()?;
                a.upscaler = Some(pyreal_engine::temporal::Upscaler::named(&v).ok_or(format!(
                    "--upscaler takes metalfx, temporal, spatial or plain, not {v}"
                ))?);
            }
            "--render-scale" => a.render_scale = num(&val()?)?,
            "--budget" => a.budget = Some(f64::from(num(&val()?)?)),
            "--ui" => a.ui = Some(PathBuf::from(val()?)),
            "--select" => a.select = Some(pixel(&val()?)?),
            "--hover" => a.hover = Some(pixel(&val()?)?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if let (Some(lib), Some(name)) = (library, name) {
        let text = std::fs::read_to_string(&lib).map_err(|e| format!("{}: {e}", lib.display()))?;
        let mut lines = text.lines();
        let heading = lines
            .by_ref()
            .find(|l| l.split('|').next().map(str::trim) == Some(name.as_str()))
            .ok_or(format!("no place named {name}"))?;
        let wanted = heading.split('|').nth(2).map_or("", str::trim);
        let spec = if wanted == "doors" {
            text.lines()
                .find_map(|l| l.strip_prefix("@doors "))
                .unwrap_or("")
        } else {
            wanted
        };
        a.objects.extend(objects(spec)?);
        let report = lines
            .find(|l| l.starts_with("dereth report"))
            .ok_or(format!("{name} has no report line"))?;
        apply_report(&mut a, report)?;
    }
    // What the command line names wins over the report.
    a.time_given = time;
    if let Some(t) = time {
        a.time = t;
    }
    if let Some(f) = fov {
        a.fov_deg = f;
        a.hfov_deg = None;
    }
    if a.dat_dir.as_os_str().is_empty() {
        return Err("--dat-dir is required".into());
    }
    if a.out.is_none() && !a.window {
        return Err("give --out, --window or both".into());
    }
    Ok(a)
}

fn open_store(
    dir: &std::path::Path,
    later: Option<&std::path::Path>,
) -> Result<RetailDatStore, String> {
    if !pyreal_dat::holds_modern_dats(dir) {
        return match later {
            Some(later) => RetailDatStore::open_classic_with_modern(dir, later),
            None => RetailDatStore::open_classic_dir(dir),
        }
        .map_err(|e| format!("{}: {e}", dir.display()));
    }
    let store = RetailDatStore::open_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // As the client opens it: whenever it is installed.
    store
        .grant_highres()
        .map_err(|e| format!("client_highres.dat in {}: {e}", dir.display()))?;
    Ok(store)
}

/// Everything a view needs once the dats are open.
struct View {
    store: Arc<RetailDatStore>,
    region: pyreal_engine::world::Region,
    camera: Camera,
    light: SkyLight,
}

fn view(a: &Args) -> Result<View, String> {
    let mut store = open_store(&a.dat_dir, a.later_dat_dir.as_deref())?;
    if let Some(dir) = &a.overlay {
        let overlay = pyreal_dat::overlay::OverlayDir::new(dir).map_err(|e| e.to_string())?;
        store = store
            .with_overlay(&overlay, None)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let store = Arc::new(store);
    let region = pyreal_world_data::landblock::load_region(&store).map_err(|e| e.to_string())?;
    let (x, y, z) = a.at;
    let z = match z {
        Some(z) => z,
        None => ground_height(&store, &region, a.block, x, y).unwrap_or(0.0) + 2.0,
    };
    let camera = Camera {
        position: Vec3::new(x, y, z),
        yaw: a.yaw.unwrap_or(-a.heading_deg.to_radians()),
        pitch: a.pitch_deg.to_radians(),
        fov_y: a.vertical_fov().to_radians(),
        near: 0.02,
        #[allow(clippy::cast_precision_loss)]
        far: (a.radius.max(a.far) as f32 + 0.5) * 192.0,
        cell: a.cell,
    };
    let (time, group) = match a.clock {
        Some(c) => {
            let (t, g) = pyreal_engine::sky::calendar(&region, c);
            (a.time_given.unwrap_or(t), a.group_given.unwrap_or(g))
        }
        None => (a.time, a.group_given.unwrap_or(0)),
    };
    let mut light = SkyLight::at(&region, group, time);
    if let Some(end) = a.fog_end {
        light.fog_end = end;
    }
    println!(
        "camera {:#06X} {:.2}, yaw {:.3}, pitch {:.3}, vertical field of view {:.1} degrees",
        a.block,
        camera.position,
        camera.yaw,
        camera.pitch,
        camera.fov_y.to_degrees()
    );
    println!(
        "time {time:.3} of a {:.0} s day, day group {group} ({:?}): sun toward {:.2} at {:.2}, ambient {:.2}, fog {:.2} from {:.0} to {:.0} m",
        region.game_time.day_length, light.weather, light.sun_dir, light.sun, light.ambient, light.fog, light.fog_start, light.fog_end
    );
    Ok(View {
        store,
        region,
        camera,
        light,
    })
}

/// The frames a headless run draws its bodies through, a sixtieth of a second apart.
const FRAME_STEP: f64 = 1.0 / 60.0;

fn load(engine: &mut Engine, v: &View, a: &Args) -> (AssetLoader, Vec<Body>) {
    let t = Instant::now();
    if !a.no_particles {
        engine.particles.assets = Some(Arc::new(
            pyreal_world_data::anim_assets::DatAnimAssets::new(Arc::clone(&v.store)),
        ));
    }
    let opened = match (&a.art_pack, &a.base_pack) {
        (Some(path), Some(base)) => Some((
            path,
            pyreal_engine::pack::ArtPack::open_path(base)
                .and_then(|b| pyreal_engine::pack::EnginePack::open_over(path, Some(&b))),
        )),
        (Some(path), None) | (None, Some(path)) => {
            Some((path, pyreal_engine::pack::EnginePack::open(path)))
        }
        (None, None) => None,
    };
    let mut loader = match opened {
        Some((path, opened)) => match opened {
            Ok(pack) => {
                let p = &pack.pack;
                println!(
                    "art pack {} ({}), opened in {:.0} ms: {} models, {} materials, {} grounds, {} pictures, {} lights, {} moons, {} kinds of cover; {} problems",
                    p.name,
                    p.hash,
                    t.elapsed().as_secs_f64() * 1e3,
                    p.models.len(),
                    p.materials.len(),
                    p.grounds.len(),
                    p.images.len(),
                    p.lights.len(),
                    p.moons.len(),
                    p.ground_cover.len(),
                    p.problems.len()
                );
                AssetLoader::with_pack(pack)
            }
            Err(e) => {
                println!("art pack {}: {e}; drawing the dats alone", path.display());
                AssetLoader::default()
            }
        },
        None => AssetLoader::default(),
    };
    if let Some(c) = a.cards_from {
        loader.cards_from = c;
    }
    let area = Area {
        center: a.block,
        radius: a.radius,
    };
    load_area(engine, &mut loader, &v.store, &v.region, area);
    let heavens = pyreal_engine::heavens::Heavens::load(engine, &mut loader, &v.store, &v.region);
    let (cards, decks, other, pack_moons) = heavens.counts();
    let now = heavens.at(v.light.group, v.light.time, a.clock.unwrap_or(0.0));
    println!(
        "heavens: {cards} cards, {decks} decks, {other} other sky objects, {pack_moons} pack moons; now moons toward {:.2?}{}",
        now.moons.iter().map(|m| m.dir).collect::<Vec<_>>(),
        if now.deck.is_some() { " and a cloud deck" } else { "" }
    );
    engine.heavens = Some(heavens);
    if a.far > a.radius {
        // The finest far level reaches past the near square's corners.
        #[allow(clippy::cast_precision_loss)]
        let first = (a.radius as f32 + 0.5) * 192.0 * 1.5 + 384.0;
        load_far(engine, &mut loader, &v.store, &v.region, area, a.far, first);
        let rings = pyreal_engine::world::far_tree_rings(a.radius, a.far);
        let t = std::time::Instant::now();
        let n = load_far_trees(engine, &mut loader, &v.store, &v.region, area, rings, first);
        if n > 0 {
            println!(
                "far trees: {n} cards on {rings} blocks past the loaded ones, in {:.0} ms",
                t.elapsed().as_secs_f64() * 1e3
            );
        }
    }
    // The bodies asked for, then a library place's objects: setups stand as bodies (as they rest,
    // doing nothing), anything else as it rests.
    let mut specs: Vec<(bodies::Spec, Vec3, bool)> = Vec::new();
    // Bodies with no place stand in a row in front of the camera.
    let row = a.bodies.iter().filter(|b| b.at.is_none()).count();
    let (fwd, right) = {
        let f = v.camera.forward();
        let f = Vec3::new(f.x, f.y, 0.0).normalize_or(Vec3::Y);
        (f, Vec3::new(f.y, -f.x, 0.0))
    };
    let mut k = 0usize;
    for spec in &a.bodies {
        let at = match spec.at {
            Some((x, y, z)) => {
                let z = z.unwrap_or_else(|| {
                    ground_height(&v.store, &v.region, a.block, x, y).unwrap_or(0.0)
                });
                Vec3::new(x, y, z)
            }
            None => {
                #[allow(clippy::cast_precision_loss)]
                let side = (k as f32 - (row as f32 - 1.0) * 0.5) * 2.5;
                k += 1;
                let p = v.camera.position + fwd * 7.0 + right * side;
                Vec3::new(p.x, p.y, render_ground(v, a, p).unwrap_or(p.z - 2.0))
            }
        };
        // On the drawn ground, as the client stands a creature.
        let drawn = spec.at.is_none_or(|(_, _, z)| z.is_none());
        let z = engine
            .land
            .as_ref()
            .and_then(|l| l.drawn_height(at.truncate()));
        let at = match z {
            Some(z) if drawn => Vec3::new(at.x, at.y, z),
            _ => at,
        };
        specs.push((*spec, at, true));
    }
    // A library place's objects: setups stand as bodies, anything else as it rests.
    for &(setup, block, x, y) in &a.objects {
        let (bx, by) = (i32::from(block >> 8), i32::from(block & 0xFF));
        let z = ground_height(&v.store, &v.region, block, x, y).unwrap_or(0.0);
        let at = area.block_origin(bx, by) + Vec3::new(x, y, z);
        if setup >> 24 == 0x02 {
            let spec = bodies::Spec {
                kind: bodies::Kind::Setup(setup, None),
                at: None,
                scale: 1.0,
            };
            specs.push((spec, at, false));
            continue;
        }
        let cell = engine.cells.cell_at(at + Vec3::Z * 0.5).unwrap_or(OUTSIDE);
        loader.place_object_in(
            engine,
            &v.store,
            DataId(setup),
            glam::Affine3A::from_translation(at),
            false,
            cell,
        );
    }
    let mut made = Vec::new();
    let mut made_setups = Vec::new();
    let named = a.bodies.len();
    for (spec, at, asked) in specs {
        let to_eye = v.camera.position - at;
        let yaw = match a.facing_deg {
            _ if !asked => 0.0,
            Some(h) => -h.to_radians(),
            None => pyreal_primitives::num::math::atan2f(-to_eye.x, to_eye.y),
        };
        let q = glam::Quat::from_rotation_z(yaw);
        let frame = pyreal_primitives::Frame::new(
            pyreal_primitives::Vec3::new(at.x, at.y, at.z),
            pyreal_primitives::Quat {
                w: q.w,
                x: q.x,
                y: q.y,
                z: q.z,
            },
        );
        let (stance, motion) = if asked {
            (a.stance, a.motion)
        } else {
            (None, None)
        };
        let dress = if asked { &a.dress[..] } else { &[] };
        let worn = if asked { &a.worn[..] } else { &[] };
        match bodies::make(&v.store, spec, frame, stance, motion, dress, worn) {
            Ok(mut b) => {
                if let Some(t) = a.translucency.filter(|_| asked) {
                    b.driver.set_described_translucency(t);
                }
                // Drawn through the headless frames, the last one at the time asked for.
                #[allow(clippy::cast_precision_loss)]
                let lead = (a.frames.max(1) - 1) as f64 * FRAME_STEP;
                bodies::run(&mut b, (a.time_in_anim - lead).max(0.0));
                made_setups.push(match spec.kind {
                    bodies::Kind::Setup(s, _) => Some(DataId(s)),
                    bodies::Kind::Character(_) => None,
                });
                made.push(b);
            }
            Err(e) => println!("body: {e}"),
        }
    }
    bodies::place_all(engine, &mut loader, &v.store, &mut made, named, a.block);
    // A character has no setup lamps; a creature's are placed each frame ([`bodies::gather`]).
    for (b, s) in made.iter_mut().zip(&made_setups) {
        if s.is_none() {
            b.setup = DataId(0);
        }
    }
    bodies::stand(engine, &made);
    for &(script, (x, y, z)) in &a.scripts {
        let z = z.unwrap_or_else(|| {
            ground_height(&v.store, &v.region, a.block, x, y).unwrap_or(0.0) + 1.0
        });
        let at = Vec3::new(x, y, z);
        let sprites = engine
            .particles
            .assets
            .as_deref()
            .map_or_else(Vec::new, |s| {
                pyreal_engine::particles::script_sprites(s, DataId(script))
            });
        for g in sprites {
            pyreal_engine::particles::prepare_sprite(engine, &mut loader, &v.store, g);
        }
        let cell = engine.cells.cell_at(at + Vec3::Z * 0.5).unwrap_or(OUTSIDE);
        let frame = pyreal_primitives::Frame::new(
            pyreal_primitives::Vec3::new(at.x, at.y, at.z),
            pyreal_primitives::Quat::IDENTITY,
        );
        if !engine.particles.play(DataId(script), frame, cell) {
            println!("script {script:#010X}: will not play");
        }
    }
    let _ = engine.upload();
    engine.gpu.wait();
    println!(
        "loaded {} blocks in {:.0} ms: {:?}",
        loader.stats.blocks,
        t.elapsed().as_secs_f64() * 1e3,
        loader.stats
    );
    (loader, made)
}

/// The ground's height under render-space point `p`, in the view's area.
fn render_ground(v: &View, a: &Args, p: Vec3) -> Option<f32> {
    let (cx, cy) = (i32::from(a.block >> 8), i32::from(a.block & 0xFF));
    let floor = pyreal_primitives::num::floor_to_i32;
    let (dx, dy) = (floor(p.x / 192.0), floor(p.y / 192.0));
    let block = u16::try_from(((cx + dx) << 8) | (cy + dy)).ok()?;
    #[allow(clippy::cast_precision_loss)]
    let (x, y) = (p.x - dx as f32 * 192.0, p.y - dy as f32 * 192.0);
    ground_height(&v.store, &v.region, block, x, y)
}

fn report(engine: &mut Engine, timings: &[(String, Vec<f64>)], spans: &[f64]) {
    let c = engine.scene.counts();
    let cull = engine.renderer.read_cull_stats(&engine.gpu);
    let w = &engine.renderer.weather;
    if w.falling() || w.cover > 0.0 || !engine.renderer.trails.prints.is_empty() {
        println!(
            "weather: {:?}, falling {:.2} (rain {:.2}, snow {:.2}), snowy land {:.2}, fresh snow {:.2}; {} prints, {} in the trail map",
            w.wanted,
            w.precip,
            w.rain,
            w.snow,
            w.snow_share,
            w.cover,
            engine.renderer.trails.prints.len(),
            engine.renderer.trails.drawn_count(),
        );
    }
    let vis = &engine.visibility;
    println!(
        "portal walk: eye in {}, {} of {} rooms seen, outside {}",
        vis.eye_cell
            .and_then(|s| engine.cells.cell(s))
            .map_or_else(|| "the outdoors".to_owned(), |c| format!("{:#010X}", c.id)),
        engine.cells.rooms_seen(vis),
        engine.cells.cells().len(),
        if vis.outside() { "seen" } else { "not seen" }
    );
    println!("pass timings (GPU, median ms):");
    let mut total = 0.0;
    for (name, v) in timings {
        let mut s = v.clone();
        s.sort_by(f64::total_cmp);
        let m = s.get(s.len() / 2).copied().unwrap_or(0.0);
        total += m;
        println!("  {name:<12} {m:>7.3}");
    }
    println!("  {:<12} {total:>7.3}", "total");
    let mut s = spans.to_vec();
    s.sort_by(f64::total_cmp);
    println!(
        "  {:<12} {:>7.3}  (first pass's start to last pass's end; the passes overlap)",
        "frame",
        s.get(s.len() / 2).copied().unwrap_or(0.0)
    );
    let e = engine.renderer.post.read_state(&engine.gpu);
    println!(
        "exposure {:.4} (target {:.4}), the frame's mean light {:.3e} scene units",
        e.exposure,
        e.target,
        pyreal_primitives::num::math::powf(2.0, e.mean_log2)
    );
    println!(
        "translucent draws, sorted far to near: {}",
        engine.renderer.sorted_draws
    );
    let kinds =
        |k: pyreal_engine::lamps::LampKind| engine.lamps.iter().filter(|l| l.kind == k).count();
    println!(
        "lamps: {} placed ({} authored, {} flames, {} the pack's, {} glows), {} lit this frame; {} instances in the rays' top level",
        engine.lamps.len(),
        kinds(pyreal_engine::lamps::LampKind::Authored),
        kinds(pyreal_engine::lamps::LampKind::Flame),
        kinds(pyreal_engine::lamps::LampKind::Pack),
        kinds(pyreal_engine::lamps::LampKind::Glow),
        engine.renderer.lamps_lit,
        engine.renderer.rays.instances
    );
    if let Some(f) = &engine.far {
        let (nodes, tris) = f.drawn();
        println!("far terrain: {nodes} nodes, {tris} triangles");
    }
    if let Some(c) = &engine.renderer.cover {
        let counts = c.read_counts(&engine.gpu);
        let mut levels = [0u32; pyreal_engine::cover::LEVELS];
        let mut triangles = 0u64;
        for (kind, n) in c.kinds.iter().zip(&counts) {
            for l in 0..levels.len() {
                levels[l] += n[l];
                let per: u64 = kind.draws[l].iter().map(|d| u64::from(d.1 / 3)).sum();
                triangles += per * u64::from(n[l]);
            }
        }
        println!(
            "cover: {} kinds in {} draws; plants drawn {} (levels {:?}), {} triangles",
            c.kinds.len(),
            c.draws.len(),
            levels.iter().sum::<u32>(),
            levels,
            triangles
        );
        let (near, far) = c.read_clumps(&engine.gpu);
        let corners = |b| u64::from(pyreal_engine::cover::clump_corners(b) / 3);
        println!(
            "grass: {near} near clumps, {far} far, {} triangles",
            u64::from(near) * corners(pyreal_engine::cover::NEAR_BLADES)
                + u64::from(far) * corners(pyreal_engine::cover::FAR_BLADES)
        );
        let (flowers, stones) = c.read_scatter(&engine.gpu);
        println!("scatter: {flowers} flowers, {stones} stones");
    }
    if let Some(land) = &engine.land {
        let looks = pyreal_engine::water::default_looks();
        let bodies: Vec<String> = land
            .bodies
            .iter()
            .filter(|b| b.vertices > 1)
            .map(|b| {
                format!(
                    "{} ({} vertices, {:.0} m)",
                    looks.get(b.look).map_or("?", |l| l.name),
                    b.vertices,
                    b.reach
                )
            })
            .collect();
        let (patches, triangles) = engine.renderer.water.drawn();
        println!(
            "water: {patches} patches, {triangles} triangles; bodies {}",
            bodies.join(", ")
        );
    }
    println!(
        "instances {} total, {} visible ({} at a coarser level), {} hidden by the ground; buckets {}; meshes {}; triangles {} total, {} visible",
        c.instances,
        cull.visible_instances,
        cull.degraded,
        cull.occluded,
        c.buckets,
        c.meshes,
        c.triangles,
        cull.visible_triangles
    );
    println!(
        "materials {}; textures {} ({:.1} MiB); vertices {} ; indices {}; visible list {}",
        engine.materials.material_count(),
        engine.materials.texture_count(),
        mib(engine.materials.texture_bytes()),
        c.vertices_used,
        c.indices_used,
        c.visible_entries
    );
    let pools: Vec<String> = engine
        .scene
        .pool_bytes()
        .iter()
        .map(|(n, b)| format!("{n} {:.0} MiB", mib(*b)))
        .collect();
    println!("pools: {}", pools.join(", "));
}

#[allow(clippy::cast_precision_loss)]
fn mib(b: u64) -> f64 {
    b as f64 / f64::from(1u32 << 20)
}

fn headless(a: &Args) -> Result<(), String> {
    let v = view(a)?;
    let gpu = Gpu::headless().map_err(|e| e.to_string())?;
    println!("adapter: {}", gpu.adapter.get_info().name);
    let mut engine = Engine::new(
        gpu,
        PoolSizes::default(),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
    .map_err(|e| e.to_string())?;
    apply_light(&mut engine, a);
    engine.renderer.weather.snowy_types = pyreal_engine::weather::snowy_types(&v.region);
    engine.renderer.weather.soft_types = pyreal_engine::weather::soft_types(&v.region);
    // Headless frames are a sixtieth of a second apart, whatever they take to draw.
    #[allow(clippy::cast_possible_truncation)]
    {
        engine.renderer.frame_seconds = Some(FRAME_STEP as f32);
    }
    engine.renderer.cut_ground = !a.no_cut;
    engine.renderer.webs.on = !a.no_webs;
    engine.particles.smoke &= !a.no_volumes;
    engine.particles.portals &= !a.retail_portals;
    engine.particles.beacons &= !a.retail_beacons;
    engine.particles.falls &= !a.retail_falls;
    engine.renderer.fountains_on = !a.retail_fountains;
    if a.no_wind {
        engine.renderer.foliage_wind = 0.0;
    }
    engine.renderer.hiz = !a.no_hiz;
    engine.renderer.lod_bias = a.lod_bias;
    let (mut loader, mut bodies) = load(&mut engine, &v, a);
    let statics = engine.lamps.len();
    engine.renderer.cover_reach = a.cover_reach;
    if let Some(path) = &a.ui {
        let view = load_ui(&engine, path, a.size)?;
        engine.renderer.ui = Some(pyreal_engine::temporal::Ui { view, gamma: false });
    }
    let mut timings: Vec<(String, Vec<f64>)> = Vec::new();
    let mut spans = Vec::new();
    let mut cpu = Vec::new();
    let mut pixels = Vec::new();
    let mut camera = v.camera;
    let mut light = v.light;
    for frame in 0..a.frames.max(1) {
        if let Some((at, time)) = a.set_time.filter(|&(at, _)| at == frame) {
            light = SkyLight::at(&v.region, light.group, time);
            println!("frame {at}: the time of day set to {time:.3}");
        }
        if frame > 0 {
            if let Some((x, y)) = a.stride {
                #[allow(clippy::cast_possible_truncation)]
                let step = FRAME_STEP as f32;
                for b in &mut bodies {
                    b.frame.origin.x += x * step;
                    b.frame.origin.y += y * step;
                    // On the drawn ground, as the client stands a walker.
                    let at = glam::Vec2::new(b.frame.origin.x, b.frame.origin.y);
                    if let Some(z) = engine.land.as_ref().and_then(|l| l.drawn_height(at)) {
                        b.frame.origin.z = z;
                    }
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let t = frame as f64 * FRAME_STEP;
            for &(script, at) in &a.body_scripts {
                if at <= t && at > t - FRAME_STEP {
                    for b in bodies.iter_mut().take(a.bodies.len()) {
                        let _ = b
                            .driver
                            .play_script_internal(pyreal_primitives::DataId(script));
                    }
                }
            }
            bodies::advance(&mut engine, &mut loader, &v.store, &mut bodies, FRAME_STEP);
            engine.renderer.clock += FRAME_STEP;
            camera.yaw -= a.turn.to_radians();
            // Presentation only: where the camera stands, not anything the game computes.
            #[allow(clippy::disallowed_methods)]
            if let Some(b) = bodies.first().filter(|_| a.orbit != 0.0) {
                let o = b.frame.origin;
                let (sin, cos) = a.orbit.to_radians().sin_cos();
                let p = camera.position - glam::Vec3::new(o.x, o.y, camera.position.z);
                camera.position.x = o.x + p.x * cos - p.y * sin;
                camera.position.y = o.y + p.x * sin + p.y * cos;
                camera.yaw += a.orbit.to_radians();
            }
        }
        if frame == 1 {
            select(&mut engine, a, &bodies);
        }
        bodies::gather(
            &mut engine,
            &mut loader,
            &v.store,
            &bodies,
            statics,
            camera.position,
        );
        // The output and its read-back as `render_to_rgba`, with the render call timed alone.
        let target = engine.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output"),
            size: wgpu::Extent3d {
                width: a.size.0,
                height: a.size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: engine.renderer.output_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let started = Instant::now();
        engine
            .render(&camera, &light, &view, a.size)
            .map_err(|e| e.to_string())?;
        cpu.push(started.elapsed().as_secs_f64() * 1e3);
        pixels = pyreal_engine::read_texture(&engine.gpu, &target, 4);
        if let (Some(out), true) = (&a.out, a.save_every > 0 && frame % a.save_every.max(1) == 0) {
            write_png(
                &out.with_extension(format!("{frame:03}.png")),
                a.size,
                &pixels,
            )?;
        }
        for t in engine.renderer.collect_timings(&engine.gpu, true) {
            match timings.iter_mut().find(|(n, _)| n == t.name) {
                Some((_, list)) => list.push(t.ms),
                None => timings.push((t.name.to_owned(), vec![t.ms])),
            }
        }
        spans.push(engine.renderer.frame_span_ms());
    }
    report(&mut engine, &timings, &spans);
    // The first frame loads and uploads the area; the rest are the steady cost.
    let mut steady = cpu.get(1..).unwrap_or(&[]).to_vec();
    steady.sort_by(f64::total_cmp);
    println!(
        "cpu frame (render call, ms): first {:.2}, median {:.3}, min {:.3}, max {:.3} over {}",
        cpu.first().copied().unwrap_or(0.0),
        steady.get(steady.len() / 2).copied().unwrap_or(0.0),
        steady.first().copied().unwrap_or(0.0),
        steady.last().copied().unwrap_or(0.0),
        steady.len()
    );
    println!(
        "particles: {} hosts, {} live, {} drawn in {} draws; {:.2} ms on the CPU",
        engine.particles.hosts.len(),
        engine.particles.live,
        engine.renderer.particles_drawn,
        engine.renderer.particle_draws,
        engine.particles.cpu_ms
    );
    println!(
        "drawn at {}x{} for {}x{} by {:?}",
        engine.renderer.internal.0,
        engine.renderer.internal.1,
        a.size.0,
        a.size.1,
        engine.renderer.temporal.used
    );
    if let Ok(at) = std::env::var("ENGINE_VIEW_PIXEL") {
        let xy: Vec<u32> = at.split(',').filter_map(|v| v.parse().ok()).collect();
        if let (Some(t), [x, y]) = (engine.renderer.ids_texture(), xy.as_slice()) {
            let ids = pyreal_engine::read_texture(&engine.gpu, t, 4);
            let i = ((y * a.size.0 + x) * 4) as usize;
            println!(
                "pixel {x},{y}: material {} (ground's {})",
                u32::from_le_bytes([ids[i], ids[i + 1], ids[i + 2], ids[i + 3]]),
                engine.renderer.terrain_material
            );
            if let Some(nt) = engine.renderer.normal_texture() {
                let ns = pyreal_engine::read_texture(&engine.gpu, nt, 4);
                let h = |o: usize| {
                    let b = u16::from_le_bytes([ns[i + o], ns[i + o + 1]]);
                    let e = i32::from((b >> 10) & 31);
                    let m = f32::from(b & 1023);
                    let v = if e == 0 {
                        m / 1024.0 * 2f32.powi(-14)
                    } else {
                        (1.0 + m / 1024.0) * 2f32.powi(e - 15)
                    };
                    if b & 0x8000 != 0 {
                        -v
                    } else {
                        v
                    }
                };
                println!("  normal oct {} {}", h(0), h(2));
            }
            if let Some(dt) = engine.renderer.depth_texture() {
                let ds = pyreal_engine::read_texture(&engine.gpu, dt, 4);
                let d = f32::from_le_bytes([ds[i], ds[i + 1], ds[i + 2], ds[i + 3]]);
                #[allow(clippy::cast_precision_loss)]
                let (w, h) = (a.size.0 as f32, a.size.1 as f32);
                #[allow(clippy::cast_precision_loss)]
                let ndc = glam::Vec4::new(
                    (*x as f32 + 0.5) / w * 2.0 - 1.0,
                    1.0 - (*y as f32 + 0.5) / h * 2.0,
                    d,
                    1.0,
                );
                let inv = v.camera.view_projection(w / h).inverse();
                let p = inv * ndc;
                let p = p.truncate() / p.w;
                println!(
                    "  depth {d}, world {p:?}, water {:?}",
                    engine.land.as_ref().and_then(|l| l.water_at(p.truncate()))
                );
            }
        }
    }
    if !bodies.is_empty() {
        println!(
            "{} bodies, {:.2} s into their motion",
            bodies.len(),
            bodies.first().map_or(0.0, Body::clock)
        );
    }
    if a.walk {
        let held = engine
            .cells
            .cell_at(v.camera.position)
            .and_then(|s| engine.cells.cell(s))
            .map_or_else(|| "no room".to_owned(), |c| format!("{:#010X}", c.id));
        println!("the eye's point is in {held}");
        for (slot, r) in engine.visibility.rects.iter().enumerate() {
            if pyreal_engine::cells::is_empty(r) {
                continue;
            }
            let name = u32::try_from(slot)
                .ok()
                .and_then(|s| engine.cells.cell(s))
                .map_or_else(|| "outside".to_owned(), |c| format!("{:#010X}", c.id));
            println!(
                "  seen {name}: [{:.3} {:.3} {:.3} {:.3}]",
                r[0], r[1], r[2], r[3]
            );
        }
    }
    if let Some(t) = a.tunnel {
        pixels = tunnel(&engine, a, t);
    }
    if let Some(out) = &a.out {
        write_png(out, a.size, &pixels)?;
        println!("wrote {}", out.display());
    }
    Ok(())
}

/// Portal space `seconds` in, in the colour of the first placed portal as the engine drew it.
fn tunnel(engine: &Engine, a: &Args, seconds: f32) -> Vec<u8> {
    use pyreal_engine::portal::{Leg, Palette, PortalFrame, PortalSpace};
    let colour = a.bodies.iter().find_map(|b| match b.kind {
        bodies::Kind::Setup(setup, _) => engine.particles.portal_colour(setup),
        bodies::Kind::Character(_) => None,
    });
    println!("tunnel colour {colour:?}");
    let target = engine.gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("tunnel"),
        size: wgpu::Extent3d {
            width: a.size.0,
            height: a.size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: engine.renderer.output_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut space = PortalSpace::new(&engine.gpu);
    let f = PortalFrame {
        leg: Leg::Tunnel,
        phase: seconds,
        travelled: seconds,
        seed: 7,
        palette: colour.map_or(Palette::PORTAL, |c| Palette::PORTAL.toward(c)),
        from_world: false,
    };
    space.render(
        &engine.gpu,
        &f,
        None,
        None,
        &view,
        engine.renderer.output_format,
        a.size,
    );
    pyreal_engine::read_texture(&engine.gpu, &target, 4)
}

/// The light's options: its calibration, exposure, bloom and tone curve.
/// Highlights the objects under `--select` and `--hover`, a body's with its target ring.
fn select(engine: &mut Engine, a: &Args, bodies: &[Body]) {
    let gpu = &engine.gpu;
    let picked = |at: Option<(u32, u32)>| at.and_then(|p| engine.renderer.pick(gpu, p));
    let selected = picked(a.select);
    let hovered = picked(a.hover);
    let ring = selected.and_then(|o| {
        let b = bodies.iter().find(|b| b.object == o)?;
        let p = b.frame.origin;
        let r = b
            .driver
            .part_array
            .setup
            .as_ref()
            .map_or(0.6, |s| s.radius.max(0.3) * 1.15);
        Some((glam::Vec3::new(p.x, p.y, p.z), r))
    });
    println!("selected {selected:?} (ring {ring:?}), hovered {hovered:?}");
    engine.renderer.selection = pyreal_engine::renderer::Selection {
        selected,
        hovered,
        ring,
        colour: 0,
    };
}

/// A PNG as the UI's picture, the output's size, premultiplied as it is laid over.
fn load_ui(
    engine: &Engine,
    path: &std::path::Path,
    size: (u32, u32),
) -> Result<wgpu::TextureView, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(
        png::Transformations::EXPAND | png::Transformations::ALPHA | png::Transformations::STRIP_16,
    );
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if (info.width, info.height) != size {
        return Err(format!(
            "--ui is {}x{}, the output {}x{}",
            info.width, info.height, size.0, size.1
        ));
    }
    let channels = info.line_size / info.width.max(1) as usize;
    let mut rgba: Vec<u8> = buf[..info.buffer_size()]
        .chunks_exact(channels)
        .flat_map(|c| match channels {
            2 => [c[0], c[0], c[0], c[1]],
            4 => [c[0], c[1], c[2], c[3]],
            _ => [c[0], c[1], c[2], 255],
        })
        .collect();
    // Premultiplied, in the picture's own (sRGB) encoding; the view decodes it.
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = u16::from(p[3]);
        for c in &mut p[..3] {
            *c = u8::try_from(u16::from(*c) * a / 255).unwrap_or(255);
        }
    }
    let t = engine.gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ui"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    engine.gpu.queue.write_texture(
        t.as_image_copy(),
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size.0 * 4),
            rows_per_image: Some(size.1),
        },
        t.size(),
    );
    Ok(t.create_view(&Default::default()))
}

fn apply_light(engine: &mut Engine, a: &Args) {
    engine.flames.on = !a.no_flames;
    let r = &mut engine.renderer;
    r.calibration = if a.retail {
        Calibration::Retail
    } else {
        Calibration::Physical
    };
    r.exposure = a
        .exposure
        .map_or(Exposure::Auto { compensation: a.ev }, Exposure::Manual);
    if a.no_bloom {
        r.bloom = 0.0;
    }
    if a.rain || a.storm {
        r.weather.falls(pyreal_engine::weather::Fall::Rain, a.storm);
    }
    if a.snow || a.blizzard {
        // The day's own weather: snow over snowy land, rain elsewhere.
        r.weather
            .falls(pyreal_engine::weather::Fall::ByLand, a.blizzard);
    }
    if let Some((rez, alb)) = a.moon_phase {
        // Rez'arel's and Alb'arel's graphics objects.
        r.atmosphere.moon_phases = [(0x0100_1F67, rez), (0x0100_1F6A, alb)];
    }
    r.shadows = !a.no_shadows;
    if let Some(d) = a.shadow_distance {
        r.shadow_distance = d;
    }
    r.water_on = !a.no_water;
    r.water.finest = pyreal_engine::water::finest_level(a.water_detail);
    r.cover_on = !a.no_cover;
    r.surfaces = a.surfaces;
    r.lamps = !a.no_lamps;
    r.ambient_occlusion = !a.no_ao;
    r.atmosphere.cloud_detail = a.clouds.min(3);
    r.bounce = !a.no_bounce;
    r.lamp_rays = !a.no_lamp_rays;
    r.air.level = a.god_rays.min(3);
    r.tone = a.tone;
    r.anti_aliasing = !a.no_aa;
    r.oit = a.oit;
    if let Some(u) = a.upscaler {
        r.upscaler = u;
    }
    r.render_scale = a.render_scale;
    r.dynamic = a
        .budget
        .map(pyreal_engine::temporal::DynamicResolution::new);
    if let Some(c) = a.clock {
        r.clock = c;
    }
}

fn write_png(path: &std::path::Path, (w, h): (u32, u32), rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}

mod window {
    //! The window: a surface the engine draws to each frame, and a free camera.

    use super::{bodies, load, report, view, Arc, Args, Body, Instant};
    use pyreal_engine::{Engine, Gpu, PoolSizes};
    use winit::application::ApplicationHandler;
    use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{Window, WindowId};

    struct Running {
        window: Arc<Window>,
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        engine: Engine,
        loader: super::AssetLoader,
        view: super::View,
        bodies: Vec<Body>,
        /// The lamps that stand still, before the bodies'.
        statics: usize,
        held: Vec<KeyCode>,
        looking: bool,
        last: Instant,
        printed: Instant,
        timings: Vec<(String, Vec<f64>)>,
        /// Where the cursor is, window pixels.
        cursor: (f64, f64),
    }

    pub struct App {
        pub args: Args,
        running: Option<Running>,
        pub error: Option<String>,
    }

    impl App {
        pub fn new(args: Args) -> Self {
            Self {
                args,
                running: None,
                error: None,
            }
        }

        fn start(&mut self, el: &ActiveEventLoop) -> Result<Running, String> {
            let a = &self.args;
            let attrs = Window::default_attributes()
                .with_title("engine-view")
                .with_inner_size(winit::dpi::LogicalSize::new(a.size.0, a.size.1));
            let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
            let surface = instance
                .create_surface(window.clone())
                .map_err(|e| e.to_string())?;
            let gpu = Gpu::new(instance, Some(&surface)).map_err(|e| e.to_string())?;
            let caps = surface.get_capabilities(&gpu.adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(wgpu::TextureFormat::is_srgb)
                .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
            let size = window.inner_size();
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: wgpu::PresentMode::AutoVsync,
                desired_maximum_frame_latency: 2,
                alpha_mode: caps
                    .alpha_modes
                    .first()
                    .copied()
                    .unwrap_or(wgpu::CompositeAlphaMode::Auto),
                view_formats: vec![],
                color_space: wgpu::SurfaceColorSpace::Auto,
            };
            surface.configure(&gpu.device, &config);
            let mut engine =
                Engine::new(gpu, PoolSizes::default(), format).map_err(|e| e.to_string())?;
            super::apply_light(&mut engine, a);
            let v = view(a)?;
            engine.renderer.weather.snowy_types = pyreal_engine::weather::snowy_types(&v.region);
            engine.renderer.weather.soft_types = pyreal_engine::weather::soft_types(&v.region);
            let (loader, bodies) = load(&mut engine, &v, a);
            let statics = engine.lamps.len();
            Ok(Running {
                window,
                surface,
                config,
                engine,
                loader,
                view: v,
                bodies,
                statics,
                held: Vec::new(),
                looking: false,
                last: Instant::now(),
                printed: Instant::now(),
                timings: Vec::new(),
                cursor: (0.0, 0.0),
            })
        }
    }

    impl Running {
        /// Selects the object under the cursor (a body with its target ring), or nothing.
        fn select(&mut self) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let at = (self.cursor.0.max(0.0) as u32, self.cursor.1.max(0.0) as u32);
            let picked = self.engine.renderer.pick(&self.engine.gpu, at);
            let ring = picked.and_then(|o| {
                let b = self.bodies.iter().find(|b| b.object == o)?;
                let p = b.frame.origin;
                let r = b
                    .driver
                    .part_array
                    .setup
                    .as_ref()
                    .map_or(0.6, |s| s.radius.max(0.3) * 1.15);
                Some((glam::Vec3::new(p.x, p.y, p.z), r))
            });
            println!("selected {picked:?}");
            self.engine.renderer.selection.selected = picked;
            self.engine.renderer.selection.ring = ring;
        }

        fn frame(&mut self) {
            let dt = self.last.elapsed().as_secs_f32().min(0.1);
            self.last = Instant::now();
            bodies::advance(
                &mut self.engine,
                &mut self.loader,
                &self.view.store,
                &mut self.bodies,
                f64::from(dt),
            );
            bodies::gather(
                &mut self.engine,
                &mut self.loader,
                &self.view.store,
                &self.bodies,
                self.statics,
                self.view.camera.position,
            );
            self.engine.renderer.clock += f64::from(dt);
            let c = &mut self.view.camera;
            let speed = if self.held.contains(&KeyCode::ShiftLeft) {
                60.0
            } else {
                12.0
            } * dt;
            let (f, r) = (c.forward(), c.right());
            for k in &self.held {
                match k {
                    KeyCode::KeyW => c.position += f * speed,
                    KeyCode::KeyS => c.position -= f * speed,
                    KeyCode::KeyD => c.position += r * speed,
                    KeyCode::KeyA => c.position -= r * speed,
                    KeyCode::KeyE => c.position.z += speed,
                    KeyCode::KeyQ => c.position.z -= speed,
                    _ => {}
                }
            }
            let out = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                other => {
                    if self.printed.elapsed().as_secs() >= 3 {
                        self.printed = Instant::now();
                        println!("no frame to draw to: {other:?}");
                    }
                    self.surface
                        .configure(&self.engine.gpu.device, &self.config);
                    return;
                }
            };
            let target = out.texture.create_view(&Default::default());
            let size = (self.config.width, self.config.height);
            if let Err(e) = self
                .engine
                .render(&self.view.camera, &self.view.light, &target, size)
            {
                eprintln!("{e}");
            }
            self.window.pre_present_notify();
            self.engine.gpu.queue.present(out);
            for t in self
                .engine
                .renderer
                .collect_timings(&self.engine.gpu, false)
            {
                match self.timings.iter_mut().find(|(n, _)| n == t.name) {
                    Some((_, list)) => list.push(t.ms),
                    None => self.timings.push((t.name.to_owned(), vec![t.ms])),
                }
            }
            if self.printed.elapsed().as_secs() >= 3 {
                self.printed = Instant::now();
                report(&mut self.engine, &self.timings, &[]);
                self.timings.clear();
            }
        }
    }

    impl ApplicationHandler for App {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            if self.running.is_some() {
                return;
            }
            match self.start(el) {
                Ok(r) => {
                    r.window.request_redraw();
                    self.running = Some(r);
                }
                Err(e) => {
                    self.error = Some(e);
                    el.exit();
                }
            }
        }

        fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
            let Some(r) = self.running.as_mut() else {
                return;
            };
            match event {
                WindowEvent::CloseRequested => el.exit(),
                WindowEvent::Resized(s) => {
                    r.config.width = s.width.max(1);
                    r.config.height = s.height.max(1);
                    r.surface.configure(&r.engine.gpu.device, &r.config);
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        if code == KeyCode::Escape {
                            el.exit();
                        }
                        r.held.retain(|k| *k != code);
                        if event.state == ElementState::Pressed {
                            r.held.push(code);
                        }
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    if button == MouseButton::Right {
                        r.looking = state == ElementState::Pressed;
                    }
                    if button == MouseButton::Left && state == ElementState::Pressed {
                        r.select();
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    r.cursor = (position.x, position.y);
                }
                WindowEvent::RedrawRequested => {
                    r.frame();
                    r.window.request_redraw();
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
            if let Some(r) = self.running.as_ref() {
                r.window.request_redraw();
            }
        }

        fn device_event(
            &mut self,
            _el: &ActiveEventLoop,
            _id: winit::event::DeviceId,
            event: DeviceEvent,
        ) {
            let Some(r) = self.running.as_mut() else {
                return;
            };
            if let DeviceEvent::MouseMotion { delta } = event {
                if r.looking {
                    #[allow(clippy::cast_possible_truncation)]
                    let (dx, dy) = (delta.0 as f32, delta.1 as f32);
                    let c = &mut r.view.camera;
                    c.yaw -= dx * 0.003;
                    c.pitch = (c.pitch - dy * 0.003).clamp(-1.5, 1.5);
                }
            }
        }
    }

    pub fn run(args: Args) -> Result<(), String> {
        let el = EventLoop::new().map_err(|e| e.to_string())?;
        let mut app = App::new(args);
        el.run_app(&mut app).map_err(|e| e.to_string())?;
        app.error.map_or(Ok(()), Err)
    }
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("engine-view: {e}");
            return ExitCode::from(2);
        }
    };
    pyreal_engine::asset::texture::set_high_res_textures(!args.no_high_res);
    pyreal_engine::ground_tint::set_coloring(if args.tod_ground {
        pyreal_engine::ground_tint::GroundColoring::ThroneOfDestiny
    } else {
        pyreal_engine::ground_tint::GroundColoring::Classic
    });
    let result = if args.out.is_some() {
        headless(&args)
    } else {
        Ok(())
    }
    .and_then(|()| {
        if args.window {
            window::run(args)
        } else {
            Ok(())
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("engine-view: {e}");
            ExitCode::FAILURE
        }
    }
}
