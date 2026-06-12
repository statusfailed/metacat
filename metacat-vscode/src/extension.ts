import * as cp from 'child_process';
import * as path from 'path';
import * as vscode from 'vscode';

let outputChannel: vscode.OutputChannel;

export function activate(context: vscode.ExtensionContext): void {
  outputChannel = vscode.window.createOutputChannel('Metacat');
  context.subscriptions.push(outputChannel);

  context.subscriptions.push(
    vscode.commands.registerCommand('metacat.checkCurrentFile', checkCurrentFile),
  );
}

export function deactivate(): void {
  // Nothing to clean up. The output channel is disposed through subscriptions.
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
