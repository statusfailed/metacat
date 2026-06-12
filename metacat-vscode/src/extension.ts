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
    vscode.languages.registerHoverProvider({ language: 'metacat' }, new MetacatHoverProvider()),
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

class MetacatHoverProvider implements vscode.HoverProvider {
  provideHover(
    document: vscode.TextDocument,
    position: vscode.Position,
  ): vscode.ProviderResult<vscode.Hover> {
    const range = document.getWordRangeAtPosition(position, /[A-Za-z0-9_-]+/);
    if (!range) {
      return undefined;
    }

    const variable = document.getText(range);
    const offset = document.offsetAt(position);
    const text = document.getText();
    if (!isInsideFrobenius(text, offset)) {
      return undefined;
    }

    const expression = expressionAround(text, offset);
    const binder = nearestBinder(text, offset, variable);

    const markdown = new vscode.MarkdownString(undefined, true);
    markdown.isTrusted = false;
    markdown.appendMarkdown(`\`${variable}\``);

    if (binder) {
      markdown.appendMarkdown('\n\nBound by:\n');
      markdown.appendCodeblock(binder, 'metacat');
    }

    if (expression) {
      markdown.appendMarkdown('\n\nContaining expression:\n');
      markdown.appendCodeblock(expression, 'metacat');
    }

    return new vscode.Hover(markdown, range);
  }
}

type Delimiter = '(' | '[' | '{';

interface OpenDelimiter {
  char: Delimiter;
  offset: number;
}

function isInsideFrobenius(text: string, offset: number): boolean {
  return delimiterStackAt(text, offset).some((entry) => entry.char === '[');
}

function expressionAround(text: string, offset: number): string | undefined {
  const stack = delimiterStackAt(text, offset);
  const parent = [...stack].reverse().find((entry) => entry.char === '(' || entry.char === '{')
    ?? stack[stack.length - 1];
  if (!parent) {
    return undefined;
  }

  const end = matchingCloseOffset(text, parent.offset);
  if (end === undefined) {
    return undefined;
  }

  return text.slice(parent.offset, end + 1).trim();
}

function nearestBinder(text: string, offset: number, variable: string): string | undefined {
  const start = topLevelExpressionStart(text, offset);
  const prefix = text.slice(start, offset);
  const frobeniusPattern = /\[([A-Za-z0-9_\-\s]*?)\.(\s*)\]/g;
  let match: RegExpExecArray | null;
  let result: string | undefined;

  while ((match = frobeniusPattern.exec(prefix)) !== null) {
    const sources = match[1].trim().split(/\s+/).filter(Boolean);
    if (sources.includes(variable)) {
      result = match[0];
    }
  }

  return result;
}

function delimiterStackAt(text: string, offset: number): OpenDelimiter[] {
  const stack: OpenDelimiter[] = [];
  let inComment = false;

  for (let i = 0; i < Math.min(offset, text.length); i += 1) {
    const char = text[i];
    if (inComment) {
      if (char === '\n') {
        inComment = false;
      }
      continue;
    }

    if (char === '#') {
      inComment = true;
      continue;
    }

    if (char === '(' || char === '[' || char === '{') {
      stack.push({ char, offset: i });
      continue;
    }

    if (char === ')' || char === ']' || char === '}') {
      const opener = openerFor(char);
      const lastIndex = findLastIndex(stack, (entry) => entry.char === opener);
      if (lastIndex >= 0) {
        stack.splice(lastIndex);
      }
    }
  }

  return stack;
}

function matchingCloseOffset(text: string, openOffset: number): number | undefined {
  const open = text[openOffset];
  const close = closeFor(open);
  if (!close) {
    return undefined;
  }

  let depth = 0;
  let inComment = false;
  for (let i = openOffset; i < text.length; i += 1) {
    const char = text[i];
    if (inComment) {
      if (char === '\n') {
        inComment = false;
      }
      continue;
    }

    if (char === '#') {
      inComment = true;
      continue;
    }

    if (char === open) {
      depth += 1;
    } else if (char === close) {
      depth -= 1;
      if (depth === 0) {
        return i;
      }
    }
  }

  return undefined;
}

function topLevelExpressionStart(text: string, offset: number): number {
  const stack = delimiterStackAt(text, offset);
  return stack[0]?.offset ?? 0;
}

function openerFor(char: string): Delimiter | undefined {
  switch (char) {
    case ')':
      return '(';
    case ']':
      return '[';
    case '}':
      return '{';
    default:
      return undefined;
  }
}

function closeFor(char: string): string | undefined {
  switch (char) {
    case '(':
      return ')';
    case '[':
      return ']';
    case '{':
      return '}';
    default:
      return undefined;
  }
}

function findLastIndex<T>(items: T[], predicate: (item: T) => boolean): number {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    if (predicate(items[i])) {
      return i;
    }
  }
  return -1;
}
