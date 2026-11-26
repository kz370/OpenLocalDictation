# AGENTS.md — Open Local Dictation Agent Instructions

This file governs all human and AI contributors. It takes precedence over model defaults.

## 1. Specs-sync mandate (binding)

`specs/` is the single source of truth for architecture, modules, data models and the IPC API.

- Any code change MUST update the matching `specs/` files in the same commit / pull request:
  - Rust (`src-tauri/src/`): `specs/full_documentation.md`, `specs/catalog.txt`, `specs/relationships.txt`, `specs/data_models.txt` (if types change), `specs/api_reference.md` (if a command or event changes), `specs/architecture_overview.md` (if layers change) and the relevant `specs/diagrams/*.mmd`.
  - UI (`src/`): `specs/full_documentation.md`, `specs/catalog.txt`, `specs/relationships.txt`, `specs/api_reference.md` (if IPC use changes).
  - Config / manifests / build scripts (`tauri.conf.json`, `Cargo.toml`, `scripts/*.bat`, `scripts/*.ps1`, `local-dictation.iss`, capabilities): `specs/developer_guide.md` and `specs/full_documentation.md`.
- If no spec update is needed, say why in the commit message or PR description (for example "typo only, no behaviour change").
- A change with behaviour and no spec update MUST be rejected in review.
- After updating specs, append a line to `specs/runtime.md` under `## Log`.

## 2. Quality gates

```text
cd src-tauri && cargo fmt --all
cd src-tauri && cargo clippy --all-targets
cd src-tauri && cargo test
npm run typecheck && npm run build
```

## 3. Safety rules

- Errors shown to users say what went wrong, why, and how to fix it (`{code, detail}` from Rust, mapped to text in `src/strings.ts`).
- Anything downloaded is verified with SHA-256 (`models.rs`). The CUDA pack parts come from pinned URLs and the pack is checked for its required files.
- Audio and dictated text never leave the machine and are never written to logs.
- The app never deletes models or the CUDA libraries; there is no delete command and no delete button. Do not add one.
- Every confirmation or message uses the app's own `Dialog` component, never `window.confirm`, `alert` or `prompt`.
- No purple in the UI; the palette is graphite and teal.
- Commit messages follow Conventional Commits (`feat:`, `fix:`, `docs:`).
