#!/bin/bash
# capture.sh <name> "<dereth report line>" [--creature SETUP[/MOTION][:AHEAD[:RIGHT]]]... [engine-view flags]...
#
# Draws one place for docs/screenshots at 3840 by 2160 from a report line pasted from the client:
# <name>-retail.jpg, <name>-pyreal.jpg and <name>-compare.jpg, and saves the line, without its
# graphics options, as notes/screenshots/reports/<name>.txt (not tracked). The retail side is
# engine-view on the data files alone with every Pyreal addition off; the Pyreal side is the
# client headless on its default options over the base pack.
#
# --creature puts a body of that setup (and motion table) AHEAD metres in front of the camera (8
# by default) and RIGHT metres to its right (left when negative), facing the way the camera looks
# back; give it more than once for more bodies. Any other flag goes to engine-view
# on both sides (for example --dress CLOTHING,TEMPLATE,SHADE). With a creature or such a flag the
# Pyreal side is engine-view over the base pack, as the client cannot place objects offline.
#
# Needs: the viewer and the client built in the test-release profile (cargo build --profile
# test-release -p pyreal-engine-view -p pyreal-client), the base pack (cargo xtask base-pack
# build), ImageMagick, and PYREAL_TEST_DAT_DIR naming the End of Retail data folder. CARGO_TARGET_DIR
# and BASE_PACK are read when set.
set -euo pipefail

[ $# -ge 2 ] || { sed -n 2,3p "$0" | cut -c3-; exit 2; }
name="$1"; line="$2"; shift 2
root="$(cd "$(dirname "$0")/../.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
viewer="$target/test-release/engine-view"
client="$target/test-release/pyreal"
base_pack="${BASE_PACK:-$target/base-pack/base-pack.dat}"
dats="${PYREAL_TEST_DAT_DIR:?PYREAL_TEST_DAT_DIR must name the End of Retail data folder}"
docs="$root/docs/screenshots"
size=3840x2160
for f in "$viewer" "$client" "$base_pack"; do
  [ -e "$f" ] || { echo "capture.sh: $f is missing (see the header)" >&2; exit 1; }
done

# The report line as both sides read it: the place, camera, field of view and time, a free
# camera, and its weather (auto drawn as clear); no graphics options, pack or window. A clear
# report's clock moves by whole days (its time of day kept) to the nearest day of the region's
# day group CLEAR_DAY (10 by default, a "Clear" day with next to no cloud), which both the client
# and engine-view then draw.
reports="$root/notes/screenshots/reports"
mkdir -p "$reports"
report="$reports/$name.txt"
python3 - "$line" "${CLEAR_DAY:-10}" > "$report" <<'EOF'
import math, re, struct, sys
hours = ['Darktide', 'Darktide-and-Half', 'Foredawn', 'Foredawn-and-Half', 'Dawnsong',
         'Dawnsong-and-Half', 'Morntide', 'Morntide-and-Half', 'Midsong', 'Midsong-and-Half',
         'Warmtide', 'Warmtide-and-Half', 'Evensong', 'Evensong-and-Half', 'Gloaming',
         'Gloaming-and-Half']
def f32(x):
    return struct.unpack('f', struct.pack('f', x))[0]
def day_group(clock, zero=3600.0, zero_year=10, day=7620.0, days=360, groups=20):
    # End of Retail's calendar: the region's game time and calc_present_day_group.
    y = clock + zero
    years = math.floor(y / (day * days))
    d = math.floor((y - years * day * days) / day)
    v = (((years + zero_year) * days + d) * 0x6A42FDB2 - 0x7541E9AE) & 0xFFFFFFFF
    i = math.floor(f32(f32(f32(float(v)) * f32(2.3283064e-10)) * f32(float(groups))))
    return i if 0 <= i < groups else 0
def clear_day(clock, want):
    for k in sorted(range(-400, 401), key=abs):
        if day_group(clock + k * 7620.0) == want:
            return clock + k * 7620.0
    return clock
out, weather = [], 'clear'
for p in (x.strip() for x in sys.argv[1].strip().split(';')):
    if p.startswith(('dereth report', 'loc ', 'camera ', 'clock ')):
        out.append(p)
    elif p.startswith('fov '):
        out += [p, 'mode free']
    elif p.startswith('time '):
        t = float(p.split()[1])
        out.append(f'time {t:.3f} {hours[int(t * 16) % 16]}, set here')
    elif p.startswith('weather ') and p.split()[1] in ('rain', 'snow', 'storm'):
        weather = p.split()[1]
if not any(p.startswith('camera ') for p in out):
    sys.exit('capture.sh: the report line names no camera')
if weather == 'clear':
    out = [f'clock {clear_day(float(p.split()[1]), int(sys.argv[2])):.6f}' if p.startswith('clock ')
           else p for p in out]
print('; '.join(out + [f'weather {weather}']))
EOF
R="$(cat "$report")"

# What the report says of the camera and the time.
read -r block cx cy yaw t cell < <(python3 - "$R" <<'EOF'
import re, sys
r = sys.argv[1]
m = re.search(r'camera (0x[0-9A-Fa-f]{4}) \[(\S+) (\S+) \S+\] yaw (\S+) pitch \S+ cell (0x[0-9A-Fa-f]{8})', r)
t = re.search(r'time ([0-9.]+)', r)
print(m.group(1), m.group(2), m.group(3), m.group(4), t.group(1) if t else '0.5', m.group(5))
EOF
)

# Flags for both sides: each creature in front of the camera, facing it.
both=()
while [ $# -gt 0 ]; do
  case "$1" in
    --creature)
      spec="$2"; shift 2
      IFS=: read -r setup ahead right <<< "$spec"
      read -r x y facing < <(python3 -c "
import math
h, a, r = -float('$yaw'), float('${ahead:-8}'), float('${right:-0}')
x = float('$cx') + a * math.sin(h) + r * math.cos(h)
y = float('$cy') + a * math.cos(h) - r * math.sin(h)
print(f'{x:.2f} {y:.2f} {(math.degrees(h) + 180) % 360:.1f}')")
      both+=(--object "$setup@$x,$y" --facing "$facing") ;;
    *) both+=("$1"); shift ;;
  esac
done

# Retail: engine-view with no pack, the client's own light, and every Pyreal addition off. A
# report's clock wins over its time unless --time names it. Outdoors no lamp light (the viewer has
# no copy of the original's lamp light on the land); rain and snow as the report says.
weather_flags=()
case "$R" in *"weather rain"*) weather_flags=(--rain);; *"weather storm"*) weather_flags=(--storm);; *"weather snow"*) weather_flags=(--snow);; esac
lamps=(); (( 16#${cell:6:4} < 0x100 )) && lamps=(--no-lamps)
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
"$viewer" --dat-dir "$dats" --report "$R" --size "$size" --frames 30 --time "$t" \
  --retail --no-surfaces --no-shadows --no-ao --no-bounce --no-cover --no-lamp-rays --no-water \
  --clouds 0 --no-volumes --no-webs ${lamps[@]+"${lamps[@]}"} ${weather_flags[@]+"${weather_flags[@]}"} \
  ${both[@]+"${both[@]}"} --out "$work/retail.png" > "$work/retail.log" 2>&1 \
  || { tail -5 "$work/retail.log" >&2; exit 1; }

# Pyreal: the client at its default options over the base pack, the whole frame drawn, no
# character; or, with objects to place, engine-view over the base pack as near those defaults as
# it goes (Object Detail 150%, the far land 12 blocks out).
if [ ${#both[@]} -eq 0 ]; then
  PYREAL_BASE_PACK="$base_pack" PYREAL_SETTINGS_DIR="$work/settings" "$client" --headless --no-ui \
    --no-character --frames 60 --size "$size" --report "$report" \
    --set-at "1:Fidelity.RenderScale=100" --dat-dir "$dats" --capture "$work/pyreal.png" \
    > "$work/pyreal.log" 2>&1 || { tail -5 "$work/pyreal.log" >&2; exit 1; }
else
  "$viewer" --dat-dir "$dats" --base-pack "$base_pack" --report "$R" --size "$size" --frames 30 \
    --time "$t" --lod-bias 1.5 --far 12 ${weather_flags[@]+"${weather_flags[@]}"} "${both[@]}" \
    --out "$work/pyreal.png" > "$work/pyreal.log" 2>&1 || { tail -5 "$work/pyreal.log" >&2; exit 1; }
fi

# JPEG at quality 90 (lower only to stay under 3 MB), and both side by side at half size.
font=/System/Library/Fonts/Supplemental/Arial.ttf
for side in retail pyreal; do
  for q in 90 87 84 80; do
    magick "$work/$side.png" -strip -sampling-factor 4:2:0 -quality $q "$docs/$name-$side.jpg"
    [ "$(stat -f %z "$docs/$name-$side.jpg")" -lt 3000000 ] && break
  done
done
label() { magick "$1" -resize 1920x1080 -font "$font" -pointsize 40 -fill white -stroke black \
  -strokewidth 2 -annotate +30+60 "$2" "$3"; }
label "$work/retail.png" Retail "$work/retail-half.png"
label "$work/pyreal.png" Pyreal "$work/pyreal-half.png"
magick "$work/retail-half.png" "$work/pyreal-half.png" +append -strip -sampling-factor 4:2:0 \
  -quality 88 "$docs/$name-compare.jpg"
echo "wrote $docs/$name-{retail,pyreal,compare}.jpg and $report"
