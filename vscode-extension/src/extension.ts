import * as vscode from 'vscode';

const keywordDocs: Record<string, string> = {
  'fn': 'Function declaration (Rust-style). Example: fn greet(name) { print(name); }',
  'let': 'Declare a variable. Use let mut in Rust-inspired Zsharpend syntax for mutable bindings.',
  'mut': 'Mutable binding modifier: let mut count = 0;',
  'enum': 'Declare an enum; Zsharpend supports data-bearing variants and destructuring match.',
  'match': 'Pattern matching expression with variants and destructuring.',
  'trait': 'Trait declaration. Trait implementations are checked by Whetstone.',
  'import': 'Import a project, installed, or host-provided module.',
  'if': 'Conditional statement. Zsharpend supports elif, else, and not.',
  'elif': 'Additional conditional branch.',
  'else': 'Fallback conditional branch.',
  'loop': 'Infinite loop; exit with break or return.',
  'while': 'Loop while condition is true.',
  'for': 'Iterate over a sequence or range.',
  'return': 'Return from a function.',
  'break': 'Exit the current loop.',
  'continue': 'Continue to the next iteration.',
  'true': 'Boolean true.',
  'false': 'Boolean false.',
  'Some': 'Optional-value constructor.',
  'Ok': 'Successful result value.',
  'Err': 'Error result value.',
  'try': 'Error-handling keyword, where supported by the language context.',
  'print': 'Print output; printf-style formatting is also supported.',
  'printf': 'Print with printf-style formatting.',
  'const': 'Declare a constant binding.',
  'class': 'Class-like declaration in Z#++ syntax.',
  'new': 'Construct an instance in supported Z#++ syntax.',
  'switch': 'Multi-branch switch statement (HolyC-inspired).',
  'case': 'Switch branch; Zsharpend supports ranges in case labels.',
  'include': 'Include directive.',
  '#include': 'C-style include directive in supported syntax.',
  'gfx3d': '3D extension: meshes, camera, and z-buffered canvas renderer.',
  'nd': 'N-dimensional array extender with broadcasting, slicing, masks, and reductions.',
  'os': 'Host OS extension: terminal, processes, filesystem, and environment access outside the sandbox.',
  'Extender': 'Rust trait used by host extensions to add modules or rewrite code.',
  'whetstone': 'Compiler/bundler and project runner; uses .wtf Whetstone Translation Files.',
  'None': 'Null/empty value (Python-compatible spelling).'
};

const completionItems: vscode.CompletionItem[] = [];
const keywords = Object.keys(keywordDocs);
for (const word of keywords) {
  const item = new vscode.CompletionItem(word, vscode.CompletionItemKind.Keyword);
  item.detail = 'Zsharpend / Z#++ keyword';
  item.documentation = new vscode.MarkdownString(keywordDocs[word]);
  completionItems.push(item);
}
for (const word of ['println', 'len', 'range', 'str', 'int', 'float', 'list', 'dict', 'set', 'map', 'filter', 'reduce', 'sqrt', 'sin', 'cos', 'abs', 'min', 'max', 'sum', 'open', 'read', 'write', 'input', 'assert']) {
  const item = new vscode.CompletionItem(word, vscode.CompletionItemKind.Function);
  item.detail = 'Built-in / common helper (availability depends on runtime and imports)';
  completionItems.push(item);
}

export function activate(context: vscode.ExtensionContext) {
  const selector: vscode.DocumentSelector = ['zxx', 'zsp', 'zsx', 'zsg'];
  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(selector, {
      provideCompletionItems(document, position) {
        const linePrefix = document.lineAt(position).text.slice(0, position.character);
        const items = completionItems.slice();
        const importMatch = linePrefix.match(/\bimport\s+([\w.]*)$/);
        if (importMatch) {
          const moduleItem = new vscode.CompletionItem('nd', vscode.CompletionItemKind.Module);
          moduleItem.detail = 'N-dimensional array extender';
          items.push(moduleItem);
          for (const mod of ['gfx3d', 'os', 'math', 'random', 'string', 'collections', 'time']) {
            const entry = new vscode.CompletionItem(mod, vscode.CompletionItemKind.Module);
            items.push(entry);
          }
        }
        return items;
      }
    }, '.', ' ')
  );
  context.subscriptions.push(
    vscode.languages.registerHoverProvider(selector, {
      provideHover(document, position) {
        const range = document.getWordRangeAtPosition(position);
        if (!range) return;
        const word = document.getText(range);
        const description = keywordDocs[word];
        if (!description) return;
        return new vscode.Hover(new vscode.MarkdownString('**' + word + '**\n\n' + description), range);
      }
    })
  );
  context.subscriptions.push(
    vscode.commands.registerCommand('zsharp.openSyntaxBook', () => {
      const book = vscode.Uri.joinPath(context.extensionUri, 'docs', 'syntax-book.md');
      vscode.commands.executeCommand('vscode.open', book);
    })
  );
}

export function deactivate() {}
