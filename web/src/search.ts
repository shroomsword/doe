// The search bar of a pane (Parsed, Bytes, Status): a text box under the pane's
// title bar. Enter goes to the next match, Shift+Enter to the previous one and
// Esc closes the bar and gives the focus back to the pane.

export interface SearchHandler {
  /** Look for the text; `direction` is 1 for the next, -1 for the previous one, 0 when the text changed. Returns what to say (how many matches, or why not). */
  find(query: string, direction: -1 | 0 | 1): string;
  /** The bar was closed: forget what it marked. */
  closed(): void;
  /** What the box says when it is empty. */
  placeholder: string;
}

export class PaneSearch {
  private readonly bar: HTMLElement;
  private readonly input: HTMLInputElement;
  private readonly count: HTMLElement;

  /**
   * @param after the element that the bar comes after (the title bar of the pane)
   * @param name what the pane is called, for the box's name
   * @param focusPane where the focus goes when the bar is closed
   */
  constructor(
    after: HTMLElement,
    name: string,
    private readonly handler: SearchHandler,
    private readonly focusPane: () => void,
  ) {
    this.bar = document.createElement('div');
    this.bar.className = 'pane-search';
    this.bar.hidden = true;
    this.bar.setAttribute('role', 'search');
    this.input = document.createElement('input');
    this.input.type = 'search';
    this.input.autocomplete = 'off';
    this.input.spellcheck = false;
    this.input.placeholder = handler.placeholder;
    this.input.setAttribute('aria-label', `search in ${name}`);
    this.count = document.createElement('span');
    this.count.className = 'pane-search-count';
    this.count.setAttribute('role', 'status');
    const close = document.createElement('button');
    close.type = 'button';
    close.textContent = 'Close';
    close.addEventListener('click', () => this.close());
    this.bar.append(this.input, this.count, close);
    after.after(this.bar);
    this.input.addEventListener('input', () => this.run(0));
    this.input.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        this.run(event.shiftKey ? -1 : 1);
      } else if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        this.close();
      }
    });
  }

  get isOpen(): boolean {
    return !this.bar.hidden;
  }

  /** Show the bar with the focus in it (the text that was there is selected, or `text` is put there). */
  open(text?: string): void {
    this.bar.hidden = false;
    if (text !== undefined) this.input.value = text;
    this.input.focus();
    this.input.select();
    if (text !== undefined) this.input.setSelectionRange(text.length, text.length);
    if (this.input.value !== '') this.run(0);
  }

  close(): void {
    this.bar.hidden = true;
    this.input.value = '';
    this.count.textContent = '';
    this.handler.closed();
    this.focusPane();
  }

  private run(direction: -1 | 0 | 1): void {
    this.count.textContent = this.input.value === '' ? '' : this.handler.find(this.input.value, direction);
    if (this.input.value === '') this.handler.closed();
  }
}
