// The documentation in the page: a dialog with the help that the build made
// from the README (help.html, served with the page), with its table of
// contents. Internal links scroll inside the dialog.

import { PaneSearch } from './search.js';

export class Help {
  private loaded = false;
  private search: PaneSearch | undefined;
  /** The marks of the matches of the search, in the order of the text. */
  private hits: HTMLElement[] = [];
  private query = '';
  private current = 0;

  constructor(
    private readonly dialog: HTMLDialogElement,
    private readonly body: HTMLElement,
    open: HTMLElement,
    close: HTMLElement,
  ) {
    open.addEventListener('click', () => void this.show());
    close.addEventListener('click', () => this.dialog.close());
    const bar = dialog.querySelector<HTMLElement>('.browse-bar');
    if (bar) {
      this.search = new PaneSearch(
        bar,
        'the help',
        {
          placeholder: 'Search the help',
          find: (query, direction) => this.find(query, direction),
          closed: () => this.clearMarks(),
        },
        () => this.body.focus(),
      );
      // "/" starts the search, as in the panes, unless the text is being typed in the search box
      dialog.addEventListener('keydown', (event) => {
        if (event.key !== '/' || event.ctrlKey || event.altKey || event.metaKey || event.defaultPrevented) return;
        if (event.target instanceof Element && event.target.closest('input, textarea, select')) return;
        event.preventDefault();
        this.search?.open();
      });
      dialog.addEventListener('close', () => this.search?.close());
    }
    this.body.addEventListener('click', (event) => {
      const link = (event.target as Element | null)?.closest<HTMLAnchorElement>('a[data-help]');
      if (!link) return;
      event.preventDefault();
      this.go(link.dataset.help ?? '');
    });
  }

  /** Open the dialog, with the text loaded the first time. */
  async show(section?: string): Promise<void> {
    if (!this.dialog.open) this.dialog.showModal();
    if (!this.loaded) {
      try {
        const response = await fetch('./help.html');
        if (!response.ok) throw new Error(`${response.status}`);
        this.body.innerHTML = await response.text();
        this.loaded = true;
      } catch (error) {
        this.body.textContent = `The help could not be loaded (${error instanceof Error ? error.message : String(error)}).`;
        return;
      }
    }
    if (section) this.go(section);
  }

  /**
   * Mark the text of the help that has `query` (any case) and go to a match: `direction` 0 is the first one, 1 the next,
   * -1 the previous one, wrapping round. Returns what the bar says.
   */
  private find(query: string, direction: -1 | 0 | 1): string {
    if (direction === 0 || this.query !== query) {
      this.mark(query);
      this.current = direction === -1 ? this.hits.length - 1 : 0;
    } else if (this.hits.length > 0) {
      this.current = (this.current + direction + this.hits.length) % this.hits.length;
    }
    if (this.hits.length === 0) return 'no match';
    this.hits.forEach((hit, i) => hit.classList.toggle('current', i === this.current));
    this.hits[this.current]?.scrollIntoView({ block: 'center' });
    return `${this.current + 1} of ${this.hits.length}`;
  }

  private mark(query: string): void {
    this.clearMarks();
    this.query = query;
    const needle = query.toLowerCase();
    const texts: Text[] = [];
    const walker = document.createTreeWalker(this.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) texts.push(node as Text);
    for (const text of texts) {
      const lower = text.data.toLowerCase();
      // toLowerCase can change the length (some letters): then the positions are not the same, so skip the node
      if (lower.length !== text.data.length || !lower.includes(needle)) continue;
      const found: number[] = [];
      for (let at = lower.indexOf(needle); at >= 0; at = lower.indexOf(needle, at + needle.length)) found.push(at);
      // from the end, so that the earlier positions stay right
      for (const at of found.reverse()) {
        const rest = text.splitText(at);
        rest.splitText(needle.length);
        const mark = document.createElement('mark');
        mark.className = 'help-hit';
        rest.replaceWith(mark);
        mark.append(rest);
      }
    }
    this.hits = [...this.body.querySelectorAll<HTMLElement>('mark.help-hit')];
  }

  private clearMarks(): void {
    for (const mark of this.hits) {
      const parent = mark.parentNode;
      mark.replaceWith(...mark.childNodes);
      parent?.normalize();
    }
    this.hits = [];
    this.query = '';
    this.current = 0;
  }

  /** Scroll to a section of the help. */
  private go(id: string): void {
    this.body.querySelector<HTMLElement>(`#${CSS.escape(`help-${id}`)}`)?.scrollIntoView({ block: 'start' });
  }
}
