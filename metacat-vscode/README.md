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

For this repository during development, set `metacat.executable` to the built CLI path, for example:

```json
{
  "metacat.executable": "../target/debug/metacat"
}
```
