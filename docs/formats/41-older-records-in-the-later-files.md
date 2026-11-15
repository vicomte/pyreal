# Older records in the later files

The later data files (`client_portal.dat` and its siblings) kept their container from Throne of
Destiny (June 2005) to the end, but three record types changed layout inside it before the end of
retail: SpellTable, CharGen and Region. Pyreal reads the later files of the end of retail only, so
it reads each of the three in its end-of-retail layout. **Files of the later container from any
other date are not supported**: a reader that met one would refuse the older records rather than
read them.

**Readers:** `Decode::decode` on `SpellTable`, `CharGen` and `Region` in `pyreal-assets`. Pinned by
the public-tier `legacy_tables` tests in `core/assets/tests/cpu/` and, in the `dat` tier, by
`capture_census` (every record of the three sets) and `legacy_layouts` (the contents of each
layout).

## The end-of-retail layouts

- **SpellTable (`0x0E00000E`)**: the spells in the layout of [30](30-spell-tables.md), then the
  spell-set table. The tables of 2005 and 2006 ended after the spells with no set table; such a
  table is refused.
- **CharGen (`0x0E000002`)**: a sex has its scale and its physics, motion and combat tables, and a
  hair style has an alternate setup. Thirteen heritages, twenty-six sexes and 869 hair styles. The
  tables of 2005 to 2010 lacked some of those fields and are refused.
- **Region (`0x13000000`)**: a sky object has **nine words**, a particle-script id between its
  object id and its properties, and the rainy day groups' sky objects name the rain's particle
  script. The land surface is texture merging. The regions before August 2012 had eight-word sky
  objects and are refused.

The files from before Throne of Destiny are read in their own layouts; see
[40](40-before-throne-of-destiny.md).
