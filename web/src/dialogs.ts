// A click outside a dialog closes it, as Esc does. A modal `<dialog>` receives the click on its backdrop as a click on the
// dialog element itself, with the pointer outside its box, so that is what is looked for: the press and the release both on
// the backdrop. A press inside that is released outside (selecting text in a field and letting go over the backdrop), or the
// other way round, is not a click on the backdrop and closes nothing.

/** Whether a pointer event is outside the box of the dialog (its padding is inside it). */
export function outside(dialog: HTMLElement, event: { clientX: number; clientY: number }): boolean {
  const box = dialog.getBoundingClientRect();
  return event.clientX < box.left || event.clientX > box.right || event.clientY < box.top || event.clientY > box.bottom;
}

/**
 * Close `dialog` when the backdrop is clicked. It is closed the way Esc closes it: the dialog gets its `cancel` event, which a
 * dialog that has something to go back from first (the reader of the type browser) may stop, and is closed if nobody does.
 * Whatever a dialog does when it is cancelled with Esc (not applying a value that was typed, keeping the old one) is what a
 * click outside does too. Focus goes back to what opened it, as the browser does for a modal dialog.
 */
export function closeOnBackdropClick(dialog: HTMLDialogElement): void {
  let pressedOutside = false;
  dialog.addEventListener('pointerdown', (event) => {
    pressedOutside = event.target === dialog && outside(dialog, event);
  });
  dialog.addEventListener('click', (event) => {
    const released = event.target === dialog && outside(dialog, event);
    const clicked = pressedOutside && released;
    pressedOutside = false;
    if (!clicked || !dialog.open) return;
    const cancel = new Event('cancel', { cancelable: true });
    dialog.dispatchEvent(cancel);
    if (!cancel.defaultPrevented) dialog.close();
  });
}

/** Every dialog of the page closes by a click outside it. */
export function closeAllOnBackdropClick(root: ParentNode = document): void {
  for (const dialog of root.querySelectorAll('dialog')) closeOnBackdropClick(dialog);
}
