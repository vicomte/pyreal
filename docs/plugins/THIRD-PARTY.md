# Third-party code in the plugin host

The sample plugins and their own credits live with the Dereth project.

**Provenance: design.** The plugin interface is this client's own addition, not the original's.

## VTClassic (loot profiles)

The loot profile reader and matcher (`dereth.loot`, in `core/plugin-host/src/loot.rs`) is ported
from VTClassic, the loot plugin for the retail client's Virindi Tank, under its MIT licence:

```text
This file is Copyright (c) 2009-2010 VirindiPlugins

Permission is hereby granted, free of charge, to any person obtaining a copy
  of this software and associated documentation files (the "Software"), to deal
  in the Software without restriction, including without limitation the rights
  to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
  copies of the Software, and to permit persons to whom the Software is
  furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
  all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
  IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
  FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
  LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
  OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
  THE SOFTWARE.
```

## UtilityBelt (the VTank route format)

`dereth.nav`'s reader of VTank `.nav` route files (`core/plugin-host/src/navfile.rs`) follows the
file layout as UtilityBelt's route reader reads it (UtilityBelt is MIT-licensed); the format is
described in [nav.md](nav.md). No code is copied.

## UtilityBelt's script API (UtilityBelt scripts)

The host's library for scripts written for UtilityBelt's scripting (`dereth.ubscript`, in
`core/plugin-lua/src/ubscript/`) matches the API of UtilityBelt.Scripting and of
UtilityBelt.Service's script modules (`utilitybelt.views`, `imgui`, `acclient`, `filesystem`,
`ubnet`, `http`), under their MIT licence: the names and shapes of its classes, members, events
and actions, the actions' types and default priorities, retries and timeouts. Its enumerations
(`ubscript/enums.lua`) are generated from those of UtilityBelt.Scripting and UtilityBelt.Common,
also MIT. The code is this library's own, written over the plugin interface; no code is
translated. The [description](ubscript.md) says what each part is here.

```text
MIT License

Copyright (c) 2022 utilitybelt

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The ImGui flag values the `imgui` module gives (`ImGuiWindowFlags` and the rest) are Dear ImGui's,
under its MIT licence (Copyright (c) 2014-2024 Omar Cornut).

## VTank metas, meta views and expressions

`dereth.meta`'s import of VTank `.met` files (`core/plugin-host/src/metfile.rs`), its meta views
and `dereth.expr`'s expression language follow the formats as they appear in meta files players
publish; no program's code was read or copied. What is carried is described in
[automation.md](automation.md).

## The script engine

- **Luau** (MIT, Roblox Corporation), built from source by the `luau0-src` crate.
- **mlua** (MIT), the Rust bindings to the VM.
