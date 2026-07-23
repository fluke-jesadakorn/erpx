# AGENTS.md

Folio is a standalone Next.js application rooted at this directory.

## Package manager

Use `bun` for Node dependencies and scripts. Do not use npm.

## Verification

Run `bun run lint` and `bunx tsc --noEmit`. Do not use a production build as verification.

## Development

Run `bun run dev` on port 3004. Keep the development server in the background when handing work back.

## Runtime

Keep `.env`, `.env.local`, `infra/.env`, and `.runtime/` outside Git. PostgreSQL, MinIO, Ollama, and n8n are host-native services. n8n state under `.runtime/n8n/` must retain its encryption key.

## Structure

Application routes live under `app/`, shared UI under `components/`, server libraries under `lib/`, database files under `db/`, and service templates under `infra/`.
