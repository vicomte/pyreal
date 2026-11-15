# Sound

How Pyreal plays the game's sounds.

## Two engines

Settings, Sound, **Sound Style** (`Sound.SoundStyle`) chooses the engine. Both make the same choices
about *what* plays: the sound tables, the animation and physics-script hooks, the server's
`0xF750` sound events, the ambient scheduler with its random draws, and the category gates. They
differ in *how* it is heard.

- **Original** (`pyreal_audio::mixer`) is the retail client to the bit: sixteen voices at
  11025 Hz, where the seventeenth sound is dropped, 12 dB a doubling falloff in whole decibels,
  a ±15 dB pan with front and back alike, ambience re-triggered on a timer, the ambient slider
  applied twice and the interface slider ignored. Its tests pin every one of those
  (`core/audio/src/lib.rs` lists them).
- **Enhanced** (`pyreal_audio::stage`, the default, and what a profile without the key gets) plays
  each wave at its own rate on the general mixer (`pyreal_audio::voices`) at the device's rate:
  - 64 voices. When all are busy the quietest one that is neither an interface sound nor the
    player's own gives way to a louder newcomer; what finds no room is counted.
  - Gain falls 9 dB a doubling past 5 m and fades out by 96 m, the original's last audible
    distance. An equal-power pan follows the bearing; a sound behind is duller and a little
    quieter; far sounds lose their top end to the air.
  - Every frame the voices are re-shaped for the listener, and a sound made by an object follows
    the object (`Emitter`); gains glide across a block.
  - An echo (Freeverb) in a building's rooms and a dungeon's halls (`Room`, from the viewer
    cell's `seen_outside`), wetter with distance.
  - A constant ambient sound whose recording fills at least 80% of its interval loops with a
    crossfaded seam and fades with the terrain's share.
  - Each slider is applied once, live; interface sounds follow the interface slider.

Waves are decoded once at their own rate (`wave::decode_native`); the original's 11 kHz copy is
made from that only while Original is chosen (`wave::retail_sample`).
Decoding happens with the mixer's lock let go, so the device thread never waits on it.

The run report's `sound engine --` line gives the engine, sounds started, dropped, given way, the
most at once, loops and those out of earshot, and how many the original's pool started and
dropped.

## What feeds the sound

Besides the sound tables and the server's events, three inputs come from the world and the engine:

- **The ambient scan's terrain.** `BlockResidency::terrain_neighbourhood` gives the 3×3 blocks'
  terrain round the viewer. Without it no ambient sound (birds, wind, water) plays.
- **The weather's sound.** The scene hands the engine's rain, storm, gusts and indoors to the
  rain and wind synthesis (`pyreal_audio::WeatherBed`) through `Scene::weather_sound`.
- **Thunder.** Each lightning strike the engine draws is heard as one of the six thunder peals one
  to five seconds later.
