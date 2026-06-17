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

  const projectTree = new MetacatProjectTreeProvider();
  context.subscriptions.push(
    projectTree,
    vscode.window.registerTreeDataProvider('metacat.project', projectTree),
  );

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

class MetacatProjectTreeProvider implements vscode.TreeDataProvider<MetacatProjectItem>, vscode.Disposable {
  private readonly changeEmitter = new vscode.EventEmitter<MetacatProjectItem | undefined | null | void>();
  private readonly disposables: vscode.Disposable[];

  readonly onDidChangeTreeData = this.changeEmitter.event;

  constructor() {
    this.disposables = [
      this.changeEmitter,
      vscode.workspace.onDidCreateFiles(() => this.refresh()),
      vscode.workspace.onDidDeleteFiles(() => this.refresh()),
      vscode.workspace.onDidRenameFiles(() => this.refresh()),
    ];
  }

  getTreeItem(element: MetacatProjectItem): vscode.TreeItem {
    return element;
  }

  async getChildren(element?: MetacatProjectItem): Promise<MetacatProjectItem[]> {
    if (element) {
      return [];
    }

    if (!vscode.workspace.workspaceFolders?.length) {
      return [MetacatProjectItem.message('Open a workspace to see Metacat files.')];
    }

    const files = await vscode.workspace.findFiles('**/*.hex', '**/{node_modules,target,out}/**', 100);
    if (files.length === 0) {
      return [MetacatProjectItem.message('No Metacat files found.')];
    }

    return files
      .sort((left, right) => left.fsPath.localeCompare(right.fsPath))
      .map((uri) => MetacatProjectItem.forFile(uri));
  }

  private refresh(): void {
    this.changeEmitter.fire();
  }

  dispose(): void {
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
  }
}

class MetacatProjectItem extends vscode.TreeItem {
  private constructor(label: string, collapsibleState: vscode.TreeItemCollapsibleState, uri?: vscode.Uri) {
    super(label, collapsibleState);
    if (uri) {
      this.resourceUri = uri;
      this.command = {
        command: 'vscode.open',
        title: 'Open File',
        arguments: [uri],
      };
    }
  }

  static forFile(uri: vscode.Uri): MetacatProjectItem {
    const folder = vscode.workspace.getWorkspaceFolder(uri);
    const label = folder ? path.relative(folder.uri.fsPath, uri.fsPath) : path.basename(uri.fsPath);
    const item = new MetacatProjectItem(label, vscode.TreeItemCollapsibleState.None, uri);
    item.contextValue = 'metacatFile';
    return item;
  }

  static message(label: string): MetacatProjectItem {
    return new MetacatProjectItem(label, vscode.TreeItemCollapsibleState.None);
  }
}
