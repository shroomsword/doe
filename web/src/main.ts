import { App } from './app.js';
import type { Doe } from './doe.js';

declare global {
  interface Window {
    /** For the tests: set when the module is loaded. */
    doe?: Doe & { ready: true };
    /** For the tests. */
    doeApp?: App;
  }
}

const app = new App(new URL('./doe_wasm_bg.wasm', import.meta.url));
app
  .start()
  .then((doe) => {
    window.doe = { ...doe, ready: true };
    window.doeApp = app;
  })
  .catch((error: unknown) => {
    const status = document.getElementById('status');
    if (status) status.textContent = `could not start: ${String(error)}`;
  });
