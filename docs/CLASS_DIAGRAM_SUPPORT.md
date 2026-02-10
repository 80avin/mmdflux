# Class Diagram Support Matrix

This document defines current class diagram parity scope for `mmdflux`.
For deep Mermaid delta analysis, see `.gumbo/research/0043-class-diagram-mermaid-delta/synthesis.md`.

## Supported

| Feature | Status | Notes |
| --- | --- | --- |
| Class declarations (`class X`) | Supported | Includes implicit class creation from parsed relations. |
| Members (`Class: member`, `class X { ... }`) | Supported | Mermaid member text is preserved. |
| Stereotypes (`<<interface>>`) | Supported | Inline, statement, and body forms. |
| Core relation operators | Supported | `<|--`, `--|>`, `<|..`, `..|>`, `*--`, `--*`, `o--`, `--o`, `-->`, `<--`, `..>`, `<..`, `--`, `..`. |
| Lollipop relations (`--()`, `()--`) | Supported | Rendered with circle endpoint marker. |
| Two-way endpoint markers | Supported | Symmetric endpoint markers (for example `<|--|>`, `o--o`) are rendered on both ends. |
| Namespace blocks (`namespace X { ... }`) | Supported | Rendered as deterministic grouped containers (subgraph borders) in text and SVG. |
| Direction (`direction LR/RL/BT/TB`) | Supported | Mapped to class layout direction. |

## Partial Support

| Feature | Status | Notes |
| --- | --- | --- |
| Cardinality (`\"1\" --> \"*\"`) | Parse-only | Parsed into class relation metadata; cardinality text is not rendered yet. |
| Class display labels (`class A[\"Label\"]`) | Parse-only | Syntax is accepted and stored; rendered class header still uses class ID. |
| Mixed two-way marker kinds (for example `o--|>`) | Partial | Marker presence is tracked at both ends; marker-kind pairing is not fully preserved for mixed endpoint kinds. |

## Unsupported Metadata Policy

The following Mermaid class diagram metadata is currently ignored in class rendering:

- Notes (`note`, `note for`)
- Interactivity (`click`, `link`, `callback`)
- Styling (`style`, `classDef`, `cssClass`, `:::`)

Policy:

- These statements are parse-tolerated and skipped.
- They do not affect text/SVG/MMDS class output.
- They currently do not emit warnings by default.

## Intentional Divergences

`mmdflux` intentionally keeps the following class rendering behavior:

- No empty attribute/operation boxes for title-only classes.
- Text alignment differs from Mermaid defaults:
  - title + attributes centered
  - methods left-aligned

These choices are product behavior, not parser gaps.
