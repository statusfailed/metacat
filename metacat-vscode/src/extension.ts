import * as cp from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let outputChannel: vscode.OutputChannel;
let languageClient: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  outputChannel = vscode.window.createOutputChannel('Metacat');
  context.subscriptions.push(outputChannel);
  startLanguageServer(context);

  const projectPanel = new MetacatProjectPanelProvider();
  context.subscriptions.push(
    projectPanel,
    vscode.window.registerWebviewViewProvider('metacat.project', projectPanel),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('metacat.checkCurrentFile', checkCurrentFile),
    vscode.commands.registerCommand('metacat.focusArrowPanel', async () => {
      await vscode.commands.executeCommand('workbench.view.extension.metacat').then(undefined, () => undefined);
      await vscode.commands.executeCommand('metacat.project.focus').then(undefined, () => undefined);
      projectPanel.refresh();
    }),
    vscode.commands.registerCommand('metacat.refreshArrowPanel', () => projectPanel.refresh()),
  );
}

export async function deactivate(): Promise<void> {
  await languageClient?.stop();
}

async function checkCurrentFile(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    vscode.window.showWarningMessage('Open a Metacat file before running check.');
    return;
  }

  if (editor.document.isUntitled) {
    vscode.window.showWarningMessage('Save the current file before running check.');
    return;
  }

  if (editor.document.isDirty) {
    await editor.document.save();
  }

  const config = vscode.workspace.getConfiguration('metacat');
  const executable = config.get<string>('executable', 'metacat');
  const theory = config.get<string>('defaultTheory', 'fol.proof');
  const filePath = editor.document.uri.fsPath;
  const cwd = workspaceRootFor(editor.document.uri) ?? path.dirname(filePath);

  outputChannel.clear();
  outputChannel.show(true);
  outputChannel.appendLine(`> ${executable} check ${theory} ${filePath}`);

  const child = cp.spawn(executable, ['check', theory, filePath], {
    cwd,
    shell: process.platform === 'win32',
  });

  child.stdout.on('data', (chunk: Buffer) => {
    outputChannel.append(chunk.toString());
  });

  child.stderr.on('data', (chunk: Buffer) => {
    outputChannel.append(chunk.toString());
  });

  child.on('error', (error: NodeJS.ErrnoException) => {
    const message = error.code === 'ENOENT'
      ? `Could not find metacat executable: ${executable}`
      : error.message;
    outputChannel.appendLine(message);
    vscode.window.showErrorMessage(message);
  });

  child.on('close', (code: number | null) => {
    if (code === 0) {
      vscode.window.showInformationMessage('Metacat check passed.');
      return;
    }

    vscode.window.showErrorMessage(`Metacat check failed with exit code ${code ?? 'unknown'}.`);
  });
}

function workspaceRootFor(uri: vscode.Uri): string | undefined {
  const folder = vscode.workspace.getWorkspaceFolder(uri);
  return folder?.uri.fsPath;
}

function startLanguageServer(context: vscode.ExtensionContext): void {
  const executable = languageServerExecutable(context);
  const serverOptions: ServerOptions = {
    run: {
      command: executable,
      transport: TransportKind.stdio,
    },
    debug: {
      command: executable,
      transport: TransportKind.stdio,
    },
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'metacat' }],
    outputChannel,
  };

  languageClient = new LanguageClient(
    'metacatLanguageServer',
    'Metacat Language Server',
    serverOptions,
    clientOptions,
  );

  context.subscriptions.push(languageClient);
  languageClient.start().catch((error: unknown) => {
    const message = error instanceof Error ? error.message : String(error);
    outputChannel.appendLine(`Failed to start metacat-lsp: ${message}`);
  });
}

function languageServerExecutable(context: vscode.ExtensionContext): string {
  const configured = vscode.workspace
    .getConfiguration('metacat')
    .get<string>('languageServerExecutable', '');
  if (configured.trim().length > 0) {
    return configured;
  }

  const localBinary = path.resolve(context.extensionPath, '..', 'target', 'debug', binaryName('metacat-lsp'));
  if (fs.existsSync(localBinary)) {
    return localBinary;
  }

  return binaryName('metacat-lsp');
}

function binaryName(name: string): string {
  return process.platform === 'win32' ? `${name}.exe` : name;
}

class MetacatProjectPanelProvider implements vscode.WebviewViewProvider, vscode.Disposable {
  private readonly disposables: vscode.Disposable[];
  private updateVersion = 0;
  private view: vscode.WebviewView | undefined;

  constructor() {
    this.disposables = [
      vscode.window.onDidChangeActiveTextEditor(() => this.update()),
      vscode.window.onDidChangeTextEditorSelection((event) => {
        if (event.textEditor === vscode.window.activeTextEditor) {
          this.update();
        }
      }),
      vscode.workspace.onDidChangeTextDocument((event) => {
        if (event.document === vscode.window.activeTextEditor?.document) {
          this.update();
        }
      }),
    ];
  }

  resolveWebviewView(view: vscode.WebviewView): void {
    this.view = view;
    view.webview.options = { enableScripts: false };
    this.update();
  }

  refresh(): void {
    this.update();
  }

  private async update(): Promise<void> {
    if (!this.view) {
      return;
    }
    const version = ++this.updateVersion;

    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'metacat') {
      this.view.webview.html = renderPanel({ kind: 'empty', message: 'Open a Metacat file.' });
      return;
    }

    const details = await semanticArrowDetailsAt(editor.document, editor.selection.active);
    if (version !== this.updateVersion) {
      return;
    }
    this.view.webview.html = renderPanel(details ?? {
      kind: 'empty',
      message: 'Select an arrow name or an arrow declaration.',
    });
  }

  dispose(): void {
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
  }
}

type PanelModel = ArrowDetails | EmptyPanel;

interface EmptyPanel {
  kind: 'empty';
  message: string;
}

interface ArrowDetails {
  kind: 'arrow';
  declarationKind: 'arr' | 'def';
  name: string;
  source: string;
  target: string;
  metavariables: string[];
  prettyMetavariables: string[];
  error?: string | null;
}

interface SemanticArrowDetails {
  declarationKind: 'arr' | 'def';
  name: string;
  source: string;
  target: string;
  metavariables: string[];
  prettyMetavariables: string[];
  error?: string | null;
}

async function semanticArrowDetailsAt(
  document: vscode.TextDocument,
  position: vscode.Position,
): Promise<ArrowDetails | undefined> {
  if (!languageClient) {
    return undefined;
  }

  try {
    const details = await languageClient.sendRequest<SemanticArrowDetails | null>('metacat/arrowDetails', {
      uri: document.uri.toString(),
      position,
    });
    if (!details) {
      return undefined;
    }
    return {
      kind: 'arrow',
      ...details,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    outputChannel.appendLine(`Failed to fetch Metacat arrow details: ${message}`);
    return undefined;
  }
}

function renderPanel(model: PanelModel): string {
  const body = model.kind === 'arrow'
    ? renderArrowDetails(model)
    : `<p class="empty">${escapeHtml(model.message)}</p>`;

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <style>
    body {
      color: var(--vscode-foreground);
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      margin: 0;
      padding: 12px;
    }
    .arrow-name {
      font-size: 13px;
      font-weight: 600;
      margin-bottom: 12px;
      overflow-wrap: anywhere;
    }
    .kind {
      color: var(--vscode-descriptionForeground);
      font-weight: 400;
      margin-left: 4px;
    }
    .section {
      margin-bottom: 14px;
    }
    .label {
      color: var(--vscode-descriptionForeground);
      font-size: 11px;
      margin-bottom: 4px;
      text-transform: uppercase;
    }
    code {
      background: var(--vscode-textCodeBlock-background);
      border-radius: 4px;
      display: block;
      font-family: var(--vscode-editor-font-family);
      line-height: 1.45;
      overflow-wrap: anywhere;
      padding: 8px;
      white-space: pre-wrap;
    }
    .meta {
      background: var(--vscode-editor-findMatchHighlightBackground);
      border: 1px solid var(--vscode-editor-findMatchBorder);
      border-radius: 3px;
      color: var(--vscode-editor-foreground);
      padding: 0 2px;
    }
    .pills {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }
    .pill {
      background: var(--vscode-badge-background);
      border-radius: 10px;
      color: var(--vscode-badge-foreground);
      font-family: var(--vscode-editor-font-family);
      padding: 2px 7px;
    }
    .empty {
      color: var(--vscode-descriptionForeground);
      margin: 0;
    }
    .error {
      background: var(--vscode-inputValidation-errorBackground);
      border: 1px solid var(--vscode-inputValidation-errorBorder);
      border-radius: 4px;
      color: var(--vscode-inputValidation-errorForeground);
      padding: 8px;
    }
  </style>
</head>
<body>${body}</body>
</html>`;
}

function renderArrowDetails(details: ArrowDetails): string {
  const metavariables = details.metavariables.length > 0
    ? `<div class="pills">${details.metavariables.map((name) => `<span class="pill">${escapeHtml(name)}</span>`).join('')}</div>`
    : '<p class="empty">None</p>';
  const error = details.error
    ? `<div class="section">
  <div class="label">Error</div>
  <div class="error">${escapeHtml(details.error)}</div>
</div>`
    : '';

  return `<div class="arrow-name">${escapeHtml(details.name)} <span class="kind">${details.declarationKind}</span></div>
${error}
<div class="section">
  <div class="label">Source</div>
  <code>${renderPrettyLabel(details.source, details.prettyMetavariables)}</code>
</div>
<div class="section">
  <div class="label">Target</div>
  <code>${renderPrettyLabel(details.target, details.prettyMetavariables)}</code>
</div>
<div class="section">
  <div class="label">Metavariables</div>
  ${metavariables}
</div>`;
}

function renderPrettyLabel(text: string, metavariables: string[]): string {
  const names = new Set(metavariables);
  if (names.size === 0) {
    return escapeHtml(text);
  }

  const pattern = new RegExp(`\\b(${[...names].map(escapeRegExp).join('|')})\\b`, 'g');
  return text
    .split(pattern)
    .map((part) => {
      const escaped = escapeHtml(part);
      return names.has(part) ? `<span class="meta">${escaped}</span>` : escaped;
    })
    .join('');
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}
