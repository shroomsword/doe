// The editor of the schema: CodeMirror 6 with YAML, and the diagnostics of doe
// underlined where they are in the text.

import { yaml } from '@codemirror/lang-yaml';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { lintGutter, setDiagnostics } from '@codemirror/lint';
import { openSearchPanel } from '@codemirror/search';
import { Compartment, EditorState, type Extension } from '@codemirror/state';
import { EditorView, basicSetup } from 'codemirror';
import { tags } from '@lezer/highlight';
import type { Diagnostic, Position } from './doe.js';
import { editorRange } from './model.js';
import type { ThemeInfo } from './theme.js';

/** The colours of the editor and of the YAML in it, from a theme. */
export function editorTheme(theme: ThemeInfo): Extension {
  const c = theme.colors;
  return [
    EditorView.theme(
      {
        '&': { color: c.foreground, backgroundColor: c.background },
        '.cm-content': { caretColor: c.accent },
        '.cm-cursor, .cm-dropCursor': { borderLeftColor: c.accent },
        '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
          backgroundColor: c.selection,
        },
        '.cm-activeLine': { backgroundColor: c.panel },
        '.cm-activeLineGutter': { backgroundColor: c.panel },
        '.cm-gutters': { backgroundColor: c.panel, color: c.muted, borderRight: `1px solid ${c.line}` },
        '.cm-matchingBracket, .cm-nonmatchingBracket': { backgroundColor: c.selection_part, outline: `1px solid ${c.line}` },
        '.cm-tooltip': { backgroundColor: c.panel, color: c.foreground, border: `1px solid ${c.line}` },
        '.cm-panels': { backgroundColor: c.panel, color: c.foreground },
        '.cm-searchMatch': { backgroundColor: c.selection_part, outline: `1px solid ${c.accent}` },
        '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: c.selection },
      },
      { dark: theme.variant === 'dark' },
    ),
    syntaxHighlighting(
      HighlightStyle.define([
        { tag: [tags.comment, tags.lineComment], color: c.muted, fontStyle: 'italic' },
        { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: c.type },
        { tag: [tags.string, tags.special(tags.string)], color: c.string },
        { tag: [tags.number, tags.integer, tags.float], color: c.number },
        { tag: [tags.bool, tags.null], color: c.boolean },
        { tag: [tags.keyword, tags.atom], color: c.enum },
        { tag: [tags.meta, tags.labelName, tags.typeName, tags.tagName, tags.className], color: c.accent },
        { tag: [tags.punctuation, tags.separator, tags.operator, tags.bracket], color: c.muted },
        { tag: tags.invalid, color: c.error },
      ]),
    ),
  ];
}

export class SchemaEditor {
  private readonly view: EditorView;
  private readonly theme = new Compartment();

  constructor(parent: HTMLElement, text: string, onChange: () => void) {
    this.view = new EditorView({
      doc: text,
      parent,
      extensions: [
        basicSetup,
        yaml(),
        lintGutter(),
        this.theme.of([]),
        this.access.of([]),
        EditorView.contentAttributes.of({ 'aria-label': 'schema' }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onChange();
        }),
      ],
    });
  }

  private readonly access = new Compartment();

  /** Allow or refuse changes to the text (what was fetched is shown, not edited). */
  setReadOnly(readOnly: boolean): void {
    this.view.dispatch({ effects: this.access.reconfigure([EditorState.readOnly.of(readOnly), EditorView.editable.of(!readOnly)]) });
  }

  /** Draw the editor in the colours of a theme. */
  setTheme(theme: ThemeInfo): void {
    this.view.dispatch({ effects: this.theme.reconfigure(editorTheme(theme)) });
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

  /** Give the editor the focus. */
  focus(): void {
    this.view.focus();
  }

  /** Open the search panel of the editor (with the focus in it). */
  search(): void {
    this.view.focus();
    openSearchPanel(this.view);
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
