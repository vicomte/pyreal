//! The client's front end over the runtime: the Horizon interface, the cursor, the clipboard
//! bridge, the device input and the renderer it draws through, generic over the platform under
//! it.
//!
//! **Depends on** the runtime and what it is built from (`pyreal-client-runtime`,
//! `pyreal-client-model`, `pyreal-client-contract`, `pyreal-client-net`, `pyreal-protocol`, `pyreal-rules`,
//! `pyreal-primitives`, `pyreal-chargen`, `pyreal-dat`, `pyreal-assets`, `pyreal-physics`, `pyreal-animation`,
//! `pyreal-audio`, `pyreal-world-data`, `pyreal-presentation`), the drawn world (`pyreal-scene`, with
//! `pyreal-world-render`), the cursor's pictures (`pyreal-codecs`), the device input (`pyreal-input`), the interface
//! (`pyreal-horizon`, with the game's words and tables it reads, `pyreal-ui-logic`) and the plugin
//! host it seats the windows of (`pyreal-plugin-host`, `pyreal-plugin-api`, `pyreal-plugin-ui`, with
//! the widget kit they are drawn with, `pyreal-ui-kit`). **Used by** the desktop client
//! (`pyreal-client`).
//!
//! **Must never** reach a platform itself: no window system, no operating system call and no sound
//! device (`cargo xtask seams`, `seam: application halves`). Everything platform-specific arrives
//! through [`platform::host::Host`], which the application is generic over: the window and its
//! clock, the time zone, the URL launch, the sound output, the clipboard, the cursor images and
//! the pointer's hold during a camera drag.
//!
//! The frame loop is the runtime's. What this crate owns is what plugs into it: the front end and
//! the application with it ([`app::ClientShell`], [`app::App`]), the HUD's model and the
//! interface's receivers beside it ([`hud`]), the renderer it draws through ([`gpu`],
//! [`present`]), the cursor ([`cursor`]) and the pointer held during a camera drag
//! ([`pointer`](mod@pointer)), the clipboard ([`clipboard`]) and the device input ([`input`],
//! [`pump`]). Runtime and scene types are named through their owning crates.

pub mod app;
pub mod front_end;

// The host-clipboard seam.
pub mod clipboard;

pub mod cursor;

// The host's pad, read once a frame.
pub mod gamepad;

/// The renderer: the scene's.
pub mod gpu;
pub mod hud;
/// The device input: the input manager with the client's registrations and the keymap store, and
/// the hand-off of what it produces to the runtime as actions.
pub mod input;

// The pointer during a camera drag: hidden, held still, and shown again where the drag began.
pub mod pointer;

// What the shell needs from the platform under it (`platform::host::Host`), and the window
// events and key identities every platform shares.
pub mod platform;
/// The presentation: the runtime's device seam and what the interface adds to it.
pub mod present;

/// The message pump: host events in, Win32 messages out, the window procedure in the middle.
pub mod pump;
/// The `Render.*` preference owners, over the scene's.
pub mod render_prefs;
