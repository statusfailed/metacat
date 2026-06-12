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

  context.subscriptions.push(
    vscode.commands.registerCommand('metacat.checkCurrentFile', checkCurrentFile),
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
