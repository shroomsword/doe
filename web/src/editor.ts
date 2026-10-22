// The editor of the schema: CodeMirror 6 with YAML, and the diagnostics of doe
// underlined where they are in the text.

import { yaml } from '@codemirror/lang-yaml';
import { lintGutter, setDiagnostics } from '@codemirror/lint';
import { EditorView, basicSetup } from 'codemirror';
import type { Diagnostic, Position } from './doe.js';
import { editorRange } from './model.js';

export class SchemaEditor {
  private readonly view: EditorView;

  constructor(parent: HTMLElement, text: string, onChange: () => void) {
    this.view = new EditorView({
      doc: text,
      parent,
      extensions: [
        basicSetup,
        yaml(),
        lintGutter(),
        EditorView.contentAttributes.of({ 'aria-label': 'schema' }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onChange();
        }),
      ],
    });
  }

  get text(): string {
    return this.view.state.doc.toString();
  }

  /** Replace the whole text (the change listener is told, as for a typed change). */
  setText(text: string): void {
    this.view.dispatch({ changes: { from: 0, to: this.view.state.doc.length, insert: text } });
  }

  /** Underline what the diagnostics are about. */
  showDiagnostics(diagnostics: readonly Diagnostic[]): void {
    const doc = this.view.state.doc;
    const marks = diagnostics.flatMap((diagnostic) => {
      const position = diagnostic.position;
      if (!position || position.line < 1 || position.line > doc.lines) return [];
      const line = doc.line(position.line);
      const { from, to } = editorRange(line.text, line.from, position);
      return [{ from, to, severity: 'error' as const, message: diagnostic.message }];
    });
    this.view.dispatch(setDiagnostics(this.view.state, marks));
  }

  /** Put the cursor where a position is, and show it. */
  reveal(position: Position): void {
    const doc = this.view.state.doc;
    if (position.line < 1 || position.line > doc.lines) return;
    const line = doc.line(position.line);
    const { from, to } = editorRange(line.text, line.from, position);
    this.view.dispatch({ selection: { anchor: from, head: to }, effects: EditorView.scrollIntoView(from, { y: 'center' }) });
    this.view.focus();
  }
}
