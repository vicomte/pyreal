//! Canonical preference keys shared by the value store and its consumers.

/// `Camera.AdjustmentSpeed`.
pub const CAMERA_ADJUSTMENT_SPEED: &str = "Camera.AdjustmentSpeed";
/// `Camera.AlignToSlope`.
pub const CAMERA_ALIGN_TO_SLOPE: &str = "Camera.AlignToSlope";
/// `Camera.Stiffness`.
pub const CAMERA_STIFFNESS: &str = "Camera.Stiffness";
/// `Debug.PerformancePanel`.
pub const PERFORMANCE_PANEL: &str = "Debug.PerformancePanel";
/// `Display.DetailedFaces`.
pub const DETAILED_FACES: &str = "Display.DetailedFaces";
/// `Display.FullScreen`.
pub const DISPLAY_FULL_SCREEN: &str = "Display.FullScreen";
/// `Display.GroundColoring`.
pub const GROUND_COLORING: &str = "Display.GroundColoring";
/// `Display.PortalStyle`.
pub const PORTAL_STYLE: &str = "Display.PortalStyle";
/// `Display.RefreshRate`.
pub const DISPLAY_REFRESH_RATE: &str = "Display.RefreshRate";
/// `Display.RenderScale`.
pub const DISPLAY_RENDER_SCALE: &str = "Display.RenderScale";
/// `Display.Resolution`.
pub const DISPLAY_RESOLUTION: &str = "Display.Resolution";
/// `Display.SyncToRefresh`.
pub const DISPLAY_SYNC_TO_REFRESH: &str = "Display.SyncToRefresh";
/// `Display.UIBlur`.
pub const UI_BLUR: &str = "Display.UIBlur";
/// `Input.InvertMouseLookYAxis`.
pub const INVERT_MOUSE_LOOK_Y_AXIS: &str = "Input.InvertMouseLookYAxis";
/// `Input.KeymapFile`.
pub const KEYMAP_FILE: &str = "Input.KeymapFile";
/// `Input.MouseLookSensitivity`.
pub const MOUSE_LOOK_SENSITIVITY: &str = "Input.MouseLookSensitivity";
/// `Input.MouseLookSmoothingAmount`.
pub const MOUSE_LOOK_SMOOTHING_AMOUNT: &str = "Input.MouseLookSmoothingAmount";
/// `Input.UseMouseTurning`.
pub const USE_MOUSE_TURNING: &str = "Input.UseMouseTurning";
/// `International.UseIME`.
pub const USE_IME: &str = "International.UseIME";
/// `Misc.CheckForUpdates`: this client's own, not retail's; `True` is on.
pub const CHECK_FOR_UPDATES: &str = "Misc.CheckForUpdates";
/// `Misc.TooltipDelay`.
pub const TOOLTIP_DELAY: &str = "Misc.TooltipDelay";
/// `Misc.TooltipEnable`.
pub const TOOLTIP_ENABLE: &str = "Misc.TooltipEnable";
/// `Render.AspectRatio`.
pub const ASPECT_RATIO: &str = "Render.AspectRatio";
/// `Render.DegradeDistance`.
pub const DEGRADE_DISTANCE: &str = "Render.DegradeDistance";
/// `Render.FieldOfView`.
pub const FIELD_OF_VIEW: &str = "Render.FieldOfView";
/// `Render.Ground`.
pub const GROUND: &str = "Render.Ground";
/// `Render.LandscapeDrawDistance`.
pub const LANDSCAPE_DRAW_DISTANCE: &str = "Render.LandscapeDrawDistance";
/// `Render.Objects`.
pub const OBJECTS: &str = "Render.Objects";
/// `Render.SceneryDrawDistance`.
pub const SCENERY_DRAW_DISTANCE: &str = "Render.SceneryDrawDistance";
/// `Render.ScreenBrightness`.
pub const SCREEN_BRIGHTNESS: &str = "Render.ScreenBrightness";
/// `Render.Sky`.
pub const SKY: &str = "Render.Sky";
/// `Sound.AmbientSoundDisabled`.
pub const AMBIENT_SOUND_DISABLED: &str = "Sound.AmbientSoundDisabled";
/// `Sound.AmbientSoundVolume`.
pub const AMBIENT_SOUND_VOLUME: &str = "Sound.AmbientSoundVolume";
/// `Sound.InterfaceSoundDisabled`.
pub const INTERFACE_SOUND_DISABLED: &str = "Sound.InterfaceSoundDisabled";
/// `Sound.InterfaceSoundVolume`.
pub const INTERFACE_SOUND_VOLUME: &str = "Sound.InterfaceSoundVolume";
/// `Sound.PlaySoundOnlyWhenActive`.
pub const PLAY_SOUND_ONLY_WHEN_ACTIVE: &str = "Sound.PlaySoundOnlyWhenActive";
/// `Sound.SoundDisabled`.
pub const SOUND_DISABLED: &str = "Sound.SoundDisabled";
/// `Sound.SoundFeatures`.
pub const SOUND_FEATURES: &str = "Sound.SoundFeatures";
/// `Sound.SoundVolume`.
pub const SOUND_VOLUME: &str = "Sound.SoundVolume";
/// `Sound.SoundStyle`: this client's own, not retail's.
pub const SOUND_STYLE: &str = "Sound.SoundStyle";
/// `Sound.WeatherSound`: this client's own, not retail's; `True` is on.
pub const WEATHER_SOUND: &str = "Sound.WeatherSound";
/// `Sound.WeatherSoundVolume`: this client's own, not retail's.
pub const WEATHER_SOUND_VOLUME: &str = "Sound.WeatherSoundVolume";
/// `UI.ChatFontFace`.
pub const CHAT_FONT_FACE: &str = "UI.ChatFontFace";
/// `UI.ChatFontSize`.
pub const CHAT_FONT_SIZE: &str = "UI.ChatFontSize";

/// The `[Fidelity]` section: the high-fidelity presentation of the world. None of these is a retail preference. Each box is a
/// switch the profile keeps as `True` or `False`; a number above 1 (from `--set-at`) asks its
/// effect for that quality level, 0 Off, 1 Low, 2 Medium, 3 High, 4 Ultra.
pub mod fidelity {
    /// `Fidelity.Shadows`: the sun's shadows, drawn inside the lighting.
    pub const SHADOWS: &str = "Fidelity.Shadows";
    /// `Fidelity.GlobalIllumination`: bounced light, drawn inside the lighting.
    pub const GLOBAL_ILLUMINATION: &str = "Fidelity.GlobalIllumination";
    /// `Fidelity.AmbientOcclusion`: ambient and contact occlusion.
    pub const AMBIENT_OCCLUSION: &str = "Fidelity.AmbientOcclusion";
    /// `Fidelity.Weather`: rain, snow and wet ground, when the day's own weather brings them.
    pub const WEATHER: &str = "Fidelity.Weather";
    /// `Fidelity.NoAntiAliasing`: the world is drawn in the better lighting without its temporal
    /// anti-aliasing.
    pub const NO_ANTI_ALIASING: &str = "Fidelity.NoAntiAliasing";
    /// `Fidelity.NoWind`: an art pack's trees and plants stand still.
    pub const NO_WIND: &str = "Fidelity.NoWind";
    /// `Fidelity.VSync`: frames are presented on the display's refresh.
    pub const VSYNC: &str = "Fidelity.VSync";
    /// `Fidelity.Hdr`: the frame is presented with the display's extended range.
    pub const HDR: &str = "Fidelity.Hdr";
    /// `Fidelity.DepthMapping`: how much relief surfaces take from a pack, a level: 0 Off,
    /// 1 Low, 2 Medium, 3 High.
    pub const DEPTH_MAPPING: &str = "Fidelity.DepthMapping";
    /// `Fidelity.GodRays`: how strongly the sunlit air shows shafts of light, a level: 0 Off,
    /// 1 Low, 2 Medium, 3 High.
    pub const GOD_RAYS: &str = "Fidelity.GodRays";
    /// `Fidelity.CloudDetail`: how finely the volumetric clouds are drawn, a level stored as
    /// 0 Low, 1 Medium, 2 High, 3 Off.
    pub const CLOUD_DETAIL: &str = "Fidelity.CloudDetail";
    /// `Fidelity.GroundCoverDrawDistance`: how far an art pack's ground cover is drawn, a share of
    /// the distance it is drawn to with nothing changed, percent; 1 leaves it out.
    pub const GROUND_COVER_DRAW_DISTANCE: &str = "Fidelity.GroundCoverDrawDistance";
    /// `Fidelity.NoFlameFlicker`: every flame's light holds steady.
    pub const NO_FLAME_FLICKER: &str = "Fidelity.NoFlameFlicker";
    /// `Fidelity.NoSilkCobwebs`: the dungeons' cobwebs are the dats' cards, not fine silk.
    pub const NO_SILK_COBWEBS: &str = "Fidelity.NoSilkCobwebs";
    /// `Fidelity.NoHighResTextures`: a picture with a high-res level in `client_highres.dat` is
    /// drawn at its original level, from the next start.
    pub const NO_HIGH_RES_TEXTURES: &str = "Fidelity.NoHighResTextures";
    /// `Fidelity.LightDrawDistance`: how far the lamps light the world and show their glow, a
    /// share of the distance they reach with nothing changed, percent.
    pub const LIGHT_DRAW_DISTANCE: &str = "Fidelity.LightDrawDistance";
    /// `Fidelity.ShadowDistance`: how far along the view the sun's shadows reach, metres.
    pub const SHADOW_DISTANCE: &str = "Fidelity.ShadowDistance";
    /// `Fidelity.FogDensity`: how thick the haze over the world is, a share of the world fog's own,
    /// percent; 1 for none.
    pub const FOG_DENSITY: &str = "Fidelity.FogDensity";
    /// `Fidelity.ObjectDetail`: how far out objects keep their detail, a share of the distances
    /// their coarser levels take over at, percent.
    pub const OBJECT_DETAIL: &str = "Fidelity.ObjectDetail";
    /// `Fidelity.GrassHeight`: how tall the grass grows, a share of the art pack's height,
    /// percent.
    pub const GRASS_HEIGHT: &str = "Fidelity.GrassHeight";
    /// `Fidelity.WaterDetail`: how finely the water's surface is drawn, a level stored as 0 Low,
    /// 1 Medium, 2 High.
    pub const WATER_DETAIL: &str = "Fidelity.WaterDetail";
    /// `Fidelity.LandscapeSmoothing`: how strongly the landscape is drawn rounded, a level stored
    /// as [`crate::options::landscape_smoothing`] says; Off, retail's exact triangles.
    pub const LANDSCAPE_SMOOTHING: &str = "Fidelity.LandscapeSmoothing";
    /// `Fidelity.DynamicResolution`: the frame rate a Dynamic render scale moves to hold, frames a
    /// second.
    pub const DYNAMIC_RESOLUTION: &str = "Fidelity.DynamicResolution";
    /// `Fidelity.RenderScale`: the share of the frame's pixels the better lighting draws the world
    /// at, each way, percent, or 1 for Dynamic; the picture is rebuilt at the frame's size.
    pub const RENDER_SCALE: &str = "Fidelity.RenderScale";
    /// Names an older profile or report may hold that no option takes any more, read as nothing:
    /// the scenery follows the landscape's draw distance, the retail look, the art pack choice and
    /// the diagnostic views are gone, and the better lighting, the lamps, the physical sky, the
    /// water, the slope materials and the damp are always drawn.
    pub const RETIRED: &[&str] = &[
        "Fidelity.TreeDrawDistance",
        "Fidelity.Interface",
        "Fidelity.RetailLook",
        "Fidelity.ArtPack",
        "Fidelity.Lamps",
        "Fidelity.Sky",
        "Fidelity.Water",
        "Fidelity.NoSlopeMaterials",
        "Fidelity.NoDamp",
        "Fidelity.Lighting",
        "Fidelity.Debug",
    ];

    /// Every name the profile and `--set-at` may set, the boxes first and the menus after them.
    pub const NAMES: &[&str] = &[
        SHADOWS,
        GLOBAL_ILLUMINATION,
        AMBIENT_OCCLUSION,
        WEATHER,
        NO_ANTI_ALIASING,
        NO_WIND,
        VSYNC,
        HDR,
        DEPTH_MAPPING,
        GOD_RAYS,
        CLOUD_DETAIL,
        GROUND_COVER_DRAW_DISTANCE,
        NO_FLAME_FLICKER,
        NO_SILK_COBWEBS,
        NO_HIGH_RES_TEXTURES,
        LIGHT_DRAW_DISTANCE,
        SHADOW_DISTANCE,
        FOG_DENSITY,
        OBJECT_DETAIL,
        GRASS_HEIGHT,
        WATER_DETAIL,
        LANDSCAPE_SMOOTHING,
        DYNAMIC_RESOLUTION,
        RENDER_SCALE,
    ];
}
