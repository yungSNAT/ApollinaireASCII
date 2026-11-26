# Research and provenance

Research captured on 1 November 2026. The goal was a Rust library, CLI and browser editor whose artwork always remains printable ASCII characters, covering static art, lettering and animation.

## Search scope

The search found 1,164 distinct GitHub repositories from 27 searches. Queries cover English, Chinese, Japanese, Korean, Russian, Portuguese, Spanish and German terms, including `字符画`, `アスキーアート`, `아스키 아트`, `ASCII арт`, `ASCII imagem` and Rust/WASM/editor/animation/FIGlet combinations. Some queries returned no matches. Results were bounded relevance pages, normally the first 30 or 100, then deduplicated.

This is a broad discovery sample, not every ASCII repository on GitHub. GitHub search caps, ranking, private repositories, untagged projects and changing results prevent an exhaustive census. Discovery recorded metadata only; it is not a claim that every candidate's source or licence was audited. Closely related candidates received deeper README, manifest, rendering-path or licence checks.

## Selection criteria

The useful foundation needed character output, Rust compatibility, a practical route to a shared native/WASM core, clear reuse terms, manageable dependencies and code narrow enough to understand. Stars were not a quality score. A large terminal framework can be excellent in its own scope whilst making a poor browser library foundation.

### Sources this repository uses

| Repository | What the evidence showed | Use |
| --- | --- | --- |
| [orhnk/RASCII](https://github.com/orhnk/RASCII) | Rust; MIT; compact resize/luminance/character-ramp converter. Source and licence inspected. | Selected conversion foundation; upstream history retained. |
| [yuanbohan/rs-figlet](https://github.com/yuanbohan/rs-figlet) | Rust Apache-2.0 FIGlet implementation with 4 bundled font styles. Source, manifest and font headers inspected. | Used as the `figlet-rs` dependency until 1 November 2026; replaced by a local renderer that supports user-chosen layouts, vertical smushing and word wrap. |
| [cmatsuoka/figlet](https://github.com/cmatsuoka/figlet) | FIGlet implementation and font format specification. Specification and licence checked. | Format reference; horizontal layout and word wrap ported from `figlet.c`; BSD-3-Clause distribution fonts bundled. |
| [patorjk/figlet.js](https://github.com/patorjk/figlet.js) | JavaScript FIGlet implementation, MIT. | Vertical layout follows its implementation of the FIGfont 2 specification. |

### Other candidates

The other candidates inspected were, in kind:

- A Rust image-to-ASCII tool: a focused converter, not an animation or editor foundation.
- A Python terminal animation framework and a Python effects engine: their language and terminal ownership conflict with a shared Rust/WASM core.
- A TypeScript browser animation editor with mixed licensing: an interaction reference only.
- A Rust terminal media player built on native decoders: its decoder stack is unsuitable for a browser core.
- Lettering and art libraries in Rust, Python and JavaScript, one under GPL-3.0: breadth references.
- A Python editor and C terminal graphics libraries with ANSI, Unicode, CP437 or colour modes: their character and colour modes exceed this project's ASCII contract.
- Image converters in Go, JavaScript and TypeScript, and a canvas-based text effect: focused converters, or output that is not selectable text.

None supplied code or artwork to this repository. Some older or guessed repository paths returned 404; they were not counted as inspected source or used as foundations.

## Text that animates as text

A terminal project's website animates its logo as text frames: selectable text cells changing over time. That behaviour is what ApollinaireASCII follows. No artwork was copied; ApollinaireASCII's ghost is a new procedural scene. Character-shaped pixels drawn in a canvas do not meet this project's selectable-text output requirement.

## Reuse boundary

The starting commit is RASCII `8d33a3670e51298d530ab202090b212501310cdd`. The resize, weighted luminance and ramp-index approach in `src/image_renderer.rs` is adapted from it, with the MIT notice preserved. ASCII validation, alpha compositing, resource bounds, dithering, edges and timed GIF import extend that component.

The frame/canvas model, scenes, animation format, editing, CLI and browser controls are new Rust code. The FIGlet renderer ports figlet 2.2.5 (horizontal layout and word wrap) and figlet.js (vertical layout); bundled fonts come from the FIGlet and TOIlet distributions. Other inspected projects supplied no copied code or artwork. See [third-party notices](../THIRD_PARTY_NOTICES.md) for the retained notices and [dependency notes](DEPENDENCIES.md) for scan results and upstream constraints.

No single inspected repository supplied the full requested combination. The result combines a small licensed Rust foundation with a shared Rust character model.
