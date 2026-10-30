// The documentation in the page: a dialog with the help that the build made
// from the README (help.html, served with the page), with its table of
// contents. Internal links scroll inside the dialog.

export class Help {
  private loaded = false;

  constructor(
    private readonly dialog: HTMLDialogElement,
    private readonly body: HTMLElement,
    open: HTMLElement,
    close: HTMLElement,
  ) {
    open.addEventListener('click', () => void this.show());
    close.addEventListener('click', () => this.dialog.close());
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

  /** Scroll to a section of the help. */
  private go(id: string): void {
    this.body.querySelector<HTMLElement>(`#${CSS.escape(`help-${id}`)}`)?.scrollIntoView({ block: 'start' });
  }
}
