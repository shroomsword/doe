// The first page: an editor for a schema, and what `check` says about it. The
// panes of the IDE come next (#89); this one shows that the pieces connect.

import { EditorView, basicSetup } from 'codemirror';
import { yaml } from '@codemirror/lang-yaml';
import { loadDoe, type Doe } from './doe.js';

const SAMPLE = `meta:
  id: sample
  endian: le
seq:
  - id: magic
    contents: [0x44, 0x4f, 0x45]
  - id: count
    type: u1
  - id: values
    type: u2
    repeat: expr
    repeat-expr: count
`;

declare global {
  interface Window {
    /** For the tests: set when the module is loaded. */
    doe?: Doe & { ready: true };
  }
}

function element(id: string): HTMLElement {
  const found = document.getElementById(id);
  if (!found) throw new Error(`the page has no #${id}`);
  return found;
}

async function main(): Promise<void> {
  const doe = await loadDoe(new URL('./doe_wasm_bg.wasm', import.meta.url));
  element('version').textContent = doe.version();

  const show = (text: string): void => {
    const answer = doe.check({ schema: text });
    element('status').textContent = answer.ok ? `ok: ${answer.id}` : 'the schema has problems';
    element('diagnostics').textContent = answer.diagnostics
      .map((d) => `${d.position ? `${d.position.line}:${d.position.column} ` : ''}${d.message}`)
      .join('\n');
  };

  const view = new EditorView({
    doc: SAMPLE,
    parent: element('editor'),
    extensions: [
      basicSetup,
      yaml(),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) show(update.state.doc.toString());
      }),
    ],
  });
  show(view.state.doc.toString());
  window.doe = { ...doe, ready: true };
}

main().catch((error: unknown) => {
  element('status').textContent = `could not start: ${String(error)}`;
});
