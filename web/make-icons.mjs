// Makes favicon.ico (16, 32 and 48 px) and apple-touch-icon.png (180 px) from favicon.svg, which is the only copy of the artwork:
//   node make-icons.mjs
// The SVG is drawn by Chromium (the one Playwright runs the tests with: `npx playwright install chromium`), on a transparent
// background for the icon and on its own opaque square for the touch icon, which iOS rounds by itself. The .ico holds PNG images.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const web = dirname(fileURLToPath(import.meta.url));
const svg = readFileSync(join(web, 'favicon.svg'), 'utf8');

const browser = await chromium.launch();
const page = await browser.newPage({ colorScheme: 'light' });

/** The artwork drawn at `size` pixels, as a PNG; `square` draws it without rounded corners and with no gap at the edge. */
async function render(source, size) {
  await page.setViewportSize({ width: size, height: size });
  const data = Buffer.from(source).toString('base64');
  await page.setContent(`<style>html,body{margin:0;background:transparent}img{display:block}</style><img width="${size}" height="${size}" src="data:image/svg+xml;base64,${data}">`);
  await page.waitForFunction(() => document.images[0].complete);
  return page.screenshot({ type: 'png', omitBackground: true });
}

const sizes = [16, 32, 48];
const images = [];
for (const size of sizes) images.push(await render(svg, size));
// the touch icon is a full square: the corners are the system's to round
const square = svg.replace('x="1.5" y="1.5" width="61" height="61" rx="14"', 'x="0" y="0" width="64" height="64" rx="0"');
const touch = await render(square, 180);
await browser.close();

// ICO: a header, one 16 byte entry for each image, then the PNG files
const head = Buffer.alloc(6 + 16 * images.length);
head.writeUInt16LE(0, 0);
head.writeUInt16LE(1, 2);
head.writeUInt16LE(images.length, 4);
let offset = head.length;
images.forEach((png, i) => {
  const size = sizes[i];
  const at = 6 + 16 * i;
  head.writeUInt8(size, at);
  head.writeUInt8(size, at + 1);
  head.writeUInt8(0, at + 2);
  head.writeUInt8(0, at + 3);
  head.writeUInt16LE(1, at + 4);
  head.writeUInt16LE(32, at + 6);
  head.writeUInt32LE(png.length, at + 8);
  head.writeUInt32LE(offset, at + 12);
  offset += png.length;
});
const ico = Buffer.concat([head, ...images]);
writeFileSync(join(web, 'favicon.ico'), ico);
writeFileSync(join(web, 'apple-touch-icon.png'), touch);
console.log(`favicon.svg ${svg.length} bytes, favicon.ico ${ico.length} bytes (${sizes.join(', ')} px), apple-touch-icon.png ${touch.length} bytes (180 px)`);
