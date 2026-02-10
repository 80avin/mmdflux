# mmdflux Web Playground

Static Vite + TypeScript playground for `mmdflux-wasm`.

## Commands

```bash
npm install
npm run test
npm run build
npm run dev
```

`npm run build` and `npm run dev` call `wasm-pack` to refresh `public/wasm-pkg` from `../crates/mmdflux-wasm`.

## Included Examples

- Flowchart Basics
- Fan-out
- Sequence Basics
- Sequence Retry
- Class Basics
- Class Interfaces

Examples are wired to the live render pipeline and are useful as smoke fixtures for manual regression checks.
