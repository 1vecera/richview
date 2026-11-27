# Report fonts

`build.py` regenerates the font faces that reports embed: the catalogs `crates/richview-core/assets/fonts-{sleek,scientific,brutalist,editorial,hand-drawn}.css`, `crates/richview-core/assets/font-manifest.json` and `crates/richview-core/assets/font-coverage.txt` (one line per family: lowercase name, a tab, the hex code point ranges its faces embed, unioned over weights; the `style.glyph-missing` check reads it). Native builds compile the catalogs in; the build stages them as one `richview-fonts.css` beside each WebAssembly runtime, whose hosts provide it to the core.

```sh
uv run scripts/fonts/build.py           # download (cached in tmp/fonts-cache), verify, rebuild
uv run scripts/fonts/build.py --check   # rebuild in memory and fail unless the committed files match
cargo test -p richview-core --test integration -- fonts::
```

The script pins fontTools 4.65.0 and brotli 1.2.0 in its inline metadata; with those versions a rebuild is byte-identical. It refuses to run with other versions.

`uv run scripts/fonts/optical.py --check` verifies the compact native left-bearing/advance estimates against these bundled WOFF2 outlines, without downloading anything. Run it without `--check` after a font update. The table covers the canonical display and monospace faces; unknown fonts or glyphs remain unestimated. Native diagnostics still say `measured: false`. Browser alignment measures the loaded face separately, caches first-label metrics, and preserves the next tick label's space.

## Sources

Every file is fetched from a pinned commit of the upstream project and verified by SHA-256 before use. The manifest records the repository, release, commit, URL and hash of each font and licence file.

| Family | Upstream | Release | Files |
| --- | --- | --- | --- |
| Space Grotesk | floriankarsten/space-grotesk | 2.0.0 | `SpaceGrotesk[wght].ttf` |
| Geist, Geist Mono | vercel/geist-font | v1.7.2 (Geist 1.800, Geist Mono 1.700) | `Geist[wght].ttf`, `Geist-Italic[wght].ttf`, `GeistMono[wght].ttf` |
| Source Serif 4 | adobe-fonts/source-serif | 4.005R | `SourceSerif4Variable-Roman.ttf`, `-Italic.ttf` |
| Source Sans 3 | adobe-fonts/source-sans | 3.052R | `SourceSans3VF-Upright.ttf` |
| Source Code Pro | adobe-fonts/source-code-pro | 2.042R-u | `SourceCodePro-Regular.ttf` |
| Newsreader | productiontype/Newsreader | 1.003 (no tagged release) | `Newsreader[opsz,wght].ttf`, `Newsreader-Italic[opsz,wght].ttf` |
| Big Shoulders | xotypeco/big_shoulders | 2.002 (newer than tag v1.100) | `BigShoulders[opsz,wght].ttf` |
| Space Mono | googlefonts/spacemono | 1.003 (newer than tag v1.0) | `SpaceMono-Regular.ttf` |
| IBM Plex Mono | IBM/plex | @ibm/plex-mono@2.5.0 | `IBMPlexMono-Regular.ttf`, `IBMPlexMono-SemiBold.ttf` |
| Patrick Hand | google/fonts (`ofl/patrickhand`; the designer publishes it there only) | 1.003 (no tagged release) | `PatrickHand-Regular.ttf` |

## Faces

| CSS family | Faces | Instance |
| --- | --- | --- |
| Space Grotesk | 600 | wght 600 |
| Geist | 400, 400 italic, 600 | wght 400, 600 |
| Geist Mono | 400 | wght 400 |
| RichView Serif | 400, 400 italic, 600 | wght 400, 600; opsz 14 |
| RichView Sans | 400, 600 | wght 400, 600 |
| RichView Code | 400 | static Regular |
| Newsreader | 400, 400 italic, 600 | wght 400, 600; opsz 16 |
| Big Shoulders | 800 | wght 800; opsz 14; tabular digits |
| Space Mono | 400 | static Regular |
| RichView Mono | 400, 600 | static Regular, SemiBold |
| Patrick Hand | 400 | static Regular; tabular digits |

`fonts_for_style` embeds every face of each family a style names, so a theme receives its italic and semibold faces without a catalog change. Variable sources are instanced with `fontTools.varLib.instancer`; outlines keep their overlap flags, and hinting is removed. Every face carries the `gasp` record for unhinted fonts (smoothing in both directions at every size).

## Coverage

Every face covers Basic Latin, Latin-1, Latin Extended-A, the hyphens and en and em dashes, curly quotes, the ellipsis, per mille, primes, U+00A0, U+2007, U+2009, U+202F, U+2212, × ÷ ± ≤ ≥ ≈ ≠, ← ↑ → ↓, € and £, plus every character of the flagship, sales and first-report sources (report text and data files). Where an upstream design lacks one of these, the script adds it from the family's own glyphs and metrics, and the manifest lists each addition:

- `alias`: the soft hyphen, U+2010 and U+2011 map to the hyphen; U+202F maps to the thin space. Where a design lacks them, the micro sign maps to the Greek mu it decomposes to, and the prime and double prime to the apostrophe and quotation mark they are typed as (Patrick Hand).
- `space`: a missing thin space is half the word space (the full advance in monospaced faces); a missing figure space has the tabular figure advance.
- `accent`: Ĕ ĕ Ĭ ĭ Ŏ ŏ are composites of the base and the breve the family uses for Ă or ă, placed with its mark anchors.
- `digraph`: in proportional faces, Ĳ ĳ ŉ Ŀ ŀ are composites of their compatibility decompositions.
- `arrow`: Newsreader and Patrick Hand have no arrows; they are drawn from the family's minus sign (same stroke, axis and side bearings) with open heads.
- `tabular`: Big Shoulders and Patrick Hand have no `tnum` feature, and canvas text in charts cannot request one, so their ten digits are tabular in the face itself: every digit gets the widest digit's advance (Big Shoulders 1026 of 2000 units, Patrick Hand 495 of 1000), is centred in it, and loses its kerning pairs (22 Big Shoulders class-kerning entries; Patrick Hand has none). Where a family has a `tnum` substitution the script bakes it into the cmap instead; neither does. The script checks every rebuilt face of these two families for ten equal, centred digits, so `--check` fails otherwise. The Geist Italic tabular seven (610 units) and the Newsreader Italic seven (1300 units) are given the common figure advance and centred, so tabular figures align.

Greek (U+0370 to U+03FF) is optional per family: the subset keeps every Greek code point the upstream cmap has, never adds one and never fails when a family has none; the manifest lists the kept ranges per face in `greek`. Source Serif 4 (RichView Serif), Source Sans 3 (RichView Sans) and Source Code Pro (RichView Code) have a full Greek alphabet with tonos. Geist, Geist Mono, Space Grotesk, Space Mono, IBM Plex Mono (RichView Mono), Big Shoulders and Patrick Hand have only a few mathematical letters (π everywhere; Λ Ω λ μ ω, Δ in Space Grotesk, as listed per face). Newsreader has no Greek. Browsers set missing Greek in the next font of the stack.

Super- and subscript digits are kept per family in the same way (STY-63): the subset keeps whichever of U+2070, U+2074 to U+207B (the superscript digits, plus and minus) and U+2080 to U+208B (the subscript digits, plus and minus) the upstream cmap has, and the manifest lists them per face in `script_digits`; ¹ ² ³ are Latin-1 and required. Every family has all ten superscript and ten subscript digits except Newsreader, which has only ⁰ and ⁴ besides ¹ ² ³, and Patrick Hand, which has no subscript digits. Only Source Sans 3 (RichView Sans) has the plus and minus signs ⁺ ⁻ ₊ ₋.

Newsreader and Patrick Hand borrow the digits they lack from the face of another bundled family with the same style and weight (`DIGIT_LENDERS`): Newsreader from Source Serif 4 (RichView Serif), Patrick Hand from Source Sans 3 (RichView Sans). Every family but Source Sans 3 borrows the four signs from Source Sans 3 (`SIGN_LENDER`), upright at the family's weight, since Source Sans 3 Italic is not bundled.
- A borrowed face is cut from the lender's outlines to the borrowed characters, with no layout features. It is named `RichView Serif Digits`, `RichView Sans Signs`, or `RichView Sans Digits` for Patrick Hand's subscripts with the signs. A family has one borrowed face per lender, so Newsreader has two.
- Digits are borrowed as they are. Each sign is moved up or down so that its minus centres on the script digits the family draws, own or lent: ⁺ ⁻ on the superior digits, ₊ ₋ on the inferiors. A monospaced family gives each sign its cell, with the outline centred in it. Borrowed unmoved, the minus sat up to 0.08 em off the superiors' centre (Geist, Space Grotesk, Source Serif 4, Big Shoulders and the monospaced faces) and up to 0.17 em below the inferiors' centre (Big Shoulders, IBM Plex Mono). The script checks every lent minus to within one unit, every plus to 1/200 em and every cell, so `--check` fails otherwise. The manifest records each face's move in `signs_moved_em`.
- The catalog lists it right after the family's face, under the family's CSS name and with a `unicode-range` of the borrowed characters, so browsers draw those from it and everything else from the family's face.
- A set the family lacks any of (the superscript digits, the subscript digits, the four signs) is borrowed whole, so a number never mixes two designs. Newsreader's own ¹ ² ³ ⁰ ⁴ top out at its cap height, while Source Serif 4's superiors rise above it, so Newsreader takes all ten superscript and all ten subscript digits; Patrick Hand keeps its own superscripts and takes the subscript digits with the signs.
- The manifest marks a borrowed face with `borrowed_from`, `lends` and `unicode_range`, and `font-coverage.txt` counts its characters for its family. A comment after its rule names what it lends and the lender, and a report that embeds it carries the lender's licence notice.

Every bundled family therefore draws ⁰ to ⁹, ₀ to ₉ and ⁺ ⁻ ₊ ₋, so 10⁻³ in a report and 1.2 × 10⁻⁶ on a chart axis never fall back, and a raw `<sub>` or `<sup>` suggests m² or CO₂ in every style. A borrowed digits face is 1.5 to 1.7 KB of WOFF2 and a signs face 0.8 KB.

Four archaic letters without a decomposition stay absent where the upstream design omits them: ĸ (Geist, Geist Mono), ſ (Geist, Geist Mono, Newsreader, Big Shoulders) and Ŧ ŧ (Source Serif 4). Monospaced faces do not compose digraphs, because two letters cannot share one cell; designs that have these characters draw a narrow I and J or a dot inside the L. Geist Mono therefore lacks Ĳ ĳ ŉ Ŀ ŀ and Space Mono lacks Ŀ ŀ. Browsers set all of these in the next font of the stack.

## OpenType features

The faces keep `kern`, `liga`, `tnum`, `lnum`, `pnum`, `onum` and `case` where the family has them, plus `locl` and `rvrn`. `ccmp`, `mark` and `mkmk` are not kept because no combining mark is encoded. `smcp` and `c2sc` are not kept: no stylesheet or style token requests small caps, and they cost 4–6 KB per face. Kerning adjustments smaller than 1/100 em are removed; a glyph-pair exception is removed only when the class kerning it overrides differs from it by less than that.

## Licences

All families are under the SIL Open Font License 1.1. The notices in `crates/richview-core/assets/font-licenses/` are embedded in every report and packaged with the binaries, together with the showcase notices in `assets/showcase/fonts/licenses/`; the files the two folders share are identical. Each notice begins with the copyright line of the pinned upstream licence, which names any Reserved Font Name, and `tests/fonts.rs` checks it against the manifest; the Source Serif 4, Source Sans 3, Source Code Pro, IBM Plex Mono and Geist Mono notices are the texts of the pinned upstream licence files, with LF line endings. Each face also keeps its upstream copyright (name ID 0), version, vendor and designer records, with the OFL description and URL.

Subset and instanced faces are Modified Versions under the OFL. Adobe reserves the font name "Source" (Source Serif 4, Source Sans 3, Source Code Pro) and IBM reserves "Plex" (IBM Plex Mono). The faces built from these families therefore use RichView Serif, RichView Sans, RichView Code and RichView Mono as both internal names and CSS family names. The other families declare no Reserved Font Name. The script reads the reservations from the pinned licence files and fails if an internal name or CSS family would use one. Original family names remain in source provenance and licence notices. The CSS aliases select the same font payloads; they do not change glyphs or metrics. Standalone landing and showcase fonts have their own declarations and notices.
