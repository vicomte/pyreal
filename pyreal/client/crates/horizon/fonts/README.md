# Horizon's fonts

Horizon draws its text from the glyph pages in `../pieces/` (`font-0.png` to `font-5.png`,
described by `fonts.json`). The pages are rasterised from four typefaces, all under the SIL Open
Font License, version 1.1. Each one's copyright line and licence are in `licences/`, as published
in the [Google Fonts repository](https://github.com/google/fonts/tree/main/ofl), and every release
package's `NOTICE.txt` carries them.

| `fonts.json` family | Typeface | Source | Licence |
|---|---|---|---|
| Body | Noto Sans | [notofonts/latin-greek-cyrillic](https://github.com/notofonts/latin-greek-cyrillic) | [OFL 1.1](licences/NotoSans-OFL.txt) |
| Numerals, BarNumbers | Saira | [Omnibus-Type/Saira](https://github.com/Omnibus-Type/Saira) | [OFL 1.1](licences/Saira-OFL.txt) |
| Heading | Pathway Gothic One | [etunni/pathway-gothic](https://github.com/etunni/pathway-gothic) | [OFL 1.1](licences/PathwayGothicOne-OFL.txt) |
| Banner | Cormorant SC | [CatharsisFonts/Cormorant](https://github.com/CatharsisFonts/Cormorant) | [OFL 1.1](licences/CormorantSC-OFL.txt) |

The typefaces were chosen and rasterised for Dereth's Horizon interface, which names them in
the commit that added it (Dereth `cafb648fa`, 2026-10-08). The pages hold the fonts' glyphs as
pictures; the font files themselves are not shipped.
