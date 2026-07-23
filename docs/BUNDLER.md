# Bundler — Turbopack only

Folio uses **Turbopack exclusively**. The dev and build scripts in `package.json` invoke `next dev --turbopack` and `next build --turbopack` explicitly.

## Forbidden

Do **not** add a `webpack:` block to `next.config.ts`. If a dependency requires webpack-only loader support, replace that dependency instead.

## Why

- Next.js 16 ships Turbopack as the stable default. Keeping it explicit locks intent against future Next.js default flips.
- Native bindings (`lib/native/vision-ocr`) load at runtime via `createRequire`; they are not bundled by either bundler, so this is bundler-agnostic.
- Aliases live in two places and must stay in sync:
  - `tsconfig.json` — path aliases into `./lib`, `./components`, and `./app` (TypeScript)
  - `next.config.ts` — Next.js configuration (bundler)

## Verifying

```sh
cd /Users/fluke/Desktop/Work/folio && bun run dev
# tail /tmp/folio.dev.log — must show "[Turbopack]" marker, NOT "[webpack]"
```
