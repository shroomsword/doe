// The list of what is wrong, in the Status pane under the status line: the
// problems of the schema, with where they are, and the error of the parse. When
// nothing is wrong the list is empty and says nothing.

import type { Diagnostic } from './doe.js';

function escape(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

export class ProblemsView {
  private readonly element: HTMLElement;
  private items: Diagnostic[] = [];

  constructor(
    parent: HTMLElement,
    private readonly onPick: (diagnostic: Diagnostic) => void,
  ) {
    this.element = document.createElement('div');
    this.element.className = 'problems';
    this.element.setAttribute('role', 'list');
    this.element.setAttribute('aria-label', 'problems');
    parent.append(this.element);
    this.element.addEventListener('click', (event) => {
      const target = (event.target as HTMLElement).closest<HTMLElement>('[data-item]');
      const item = target?.dataset.item !== undefined ? this.items[Number(target.dataset.item)] : undefined;
      if (item) this.onPick(item);
    });
  }

  show(diagnostics: readonly Diagnostic[]): void {
    this.items = [...diagnostics];
    if (this.items.length === 0) {
      // nothing is wrong: the pane has the status only
      this.element.innerHTML = '';
      return;
    }
    this.element.innerHTML = this.items
      .map((item, index) => {
        const where = item.position ? `<span class="where">${item.position.line}:${item.position.column}</span>` : '';
        return `<button type="button" class="problem" data-item="${index}" role="listitem"><span class="kind">${escape(item.kind)}</span>${where}<span class="message">${escape(item.message)}</span></button>`;
      })
      .join('');
  }
}
