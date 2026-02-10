# mmdflux

Render Mermaid diagrams as Unicode text, ASCII, SVG, or MMDS JSON.

`mmdflux` is built for terminal-first diagram workflows: quick local rendering, linting, and machine-readable graph output for tooling.

## Install

### Homebrew (recommended)

```bash
brew tap kevinswiber/mmdflux
brew install mmdflux
```

### Cargo

```bash
cargo install mmdflux
```

### Prebuilt binaries

Download platform binaries from [GitHub Releases](https://github.com/kevinswiber/mmdflux/releases).

## Quick Start

```bash
# Render a Mermaid file to text (default format)
mmdflux diagram.mmd

# Read Mermaid from stdin
printf 'graph LR\nA-->B\n' | mmdflux

# ASCII output
mmdflux --format ascii diagram.mmd

# SVG output (flowchart only)
mmdflux --format svg diagram.mmd -o diagram.svg

# MMDS JSON output
mmdflux --format mmds diagram.mmd

# Lint mode (validate input and print diagnostics)
mmdflux --lint diagram.mmd
```

## What It Supports

- Flowchart rendering in text/ASCII/SVG/MMDS
- Class diagram rendering in text/ASCII/SVG/MMDS
- Mermaid-to-MMDS and MMDS-to-Mermaid conversion
- Layout directions: `TD`, `BT`, `LR`, `RL`
- Edge styles: solid, dotted, thick, invisible, cross-arrow, circle-arrow

## Documentation

- [CLI reference](docs/CLI_REFERENCE.md)
- [Class diagram support matrix](docs/CLASS_DIAGRAM_SUPPORT.md)
- [MMDS specification](docs/mmds.md)
- [MMDS JSON schema](docs/mmds.schema.json)
- [Gallery](docs/gallery.md)
- [Library usage](docs/LIBRARY.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Debugging and parity tools](docs/DEBUG.md)
- [Releasing](docs/RELEASING.md)
- [WASM build/test commands](docs/WASM.md)

## License

MIT
