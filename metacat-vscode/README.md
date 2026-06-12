# Metacat VS Code Extension

Local development extension for Metacat files.

## Development

Install dependencies:

```sh
npm install
```

Compile:

```sh
npm run compile
```

Build the local language server:

```sh
cargo build -p metacat-lsp
```

Run locally:

1. Open this `metacat-vscode` folder in VS Code.
2. Press `F5`.
3. In the Extension Development Host, open a Metacat repo or a `.hex` file.

The `F5` launch task builds both the TypeScript extension and the local
`metacat-lsp` binary. The extension starts `../target/debug/metacat-lsp`
automatically when it exists.

## Configuration

- `metacat.executable`: path to the `metacat` CLI. Defaults to `metacat`.
- `metacat.defaultTheory`: theory name used by `Metacat: Check Current File`. Defaults to `fol.proof`.
- `metacat.languageServerExecutable`: path to the `metacat-lsp` executable. Defaults to the local development binary when available.

## Language Server

The Rust language server lives in `../metacat-lsp`.

Current LSP capabilities:

- full-document sync
- diagnostics
- hover

Project loading:

- For any file-backed document, the LSP walks upward from the file directory and
  uses the nearest `metacat.json`.
- Manifest paths are relative to the directory containing `metacat.json`.
- The current open buffer is always included, and unsaved edits override the
  file contents on disk.
- If no `metacat.json` is found, the LSP uses standalone mode and loads only the
  current file.

Minimal `metacat.json`:

```json
{
  "files": [
    "stdlib/**/*.hex",
    "examples/current.hex"
  ]
}
```

To load files from another folder, use `include`. The `folder` path is relative
to the `metacat.json` directory, and each entry in `files` is relative to that
folder:

```json
{
  "files": [
    "examples/current.hex"
  ],
  "include": [
    {
      "folder": "../stdlib",
      "files": [
        "**/*.hex"
      ]
    }
  ]
}
```

Module guide:

- `capabilities.rs`: advertised LSP capabilities. Add protocol features here first.
- `server.rs`: LSP request/notification wiring.
- `documents.rs`: in-memory document store.
- `project.rs`: nearest `metacat.json` discovery and project load sets.
- `diagnostics.rs`: parse/load/check diagnostics.
- `hover.rs`: hover presentation and feature flow.
- `analysis.rs`: Metacat type/profile lookup helpers.
- `syntax.rs`: source position, token, and Hexpr surface scanning helpers.

For this repository during development, set `metacat.executable` to the built CLI path, for example:

```json
{
  "metacat.executable": "../target/debug/metacat"
}
```
