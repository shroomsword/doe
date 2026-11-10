// `doe --ide` for the tests: a directory of types, and a file too large for the
// browser (a sparse file, so it takes no room). Needs target/debug/doe built
// after web/dist (`cargo build -p doe-cli`).
import { spawn } from 'node:child_process';
import { closeSync, ftruncateSync, mkdirSync, openSync, rmSync, writeFileSync, writeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
// a place the tests know (they change the types and the config while the page is open)
const dir = process.env.DOE_IDE_TEST_DIR ?? join(tmpdir(), 'doe-ide-test');
rmSync(dir, { recursive: true, force: true });
mkdirSync(dir, { recursive: true });
const types = join(dir, 'types');
mkdirSync(types);
mkdirSync(join(dir, 'extra'));
writeFileSync(join(dir, 'config.toml'), 'default_types = false\n');
writeFileSync(join(types, 'pair.ksy'), 'meta:\n  id: pair\nimports:\n  - common\nseq:\n  - id: a\n    type: u2be\n  - id: b\n    type: common\n');
writeFileSync(join(types, 'common.ksy'), 'meta:\n  id: common\ndoc: two bytes\nseq:\n  - id: x\n    type: u2be\n');

// 300 MiB: more than the page takes (256 MiB).
const size = 300 * 1024 * 1024;
const big = join(dir, 'big.bin');
const fd = openSync(big, 'w');
ftruncateSync(fd, size);
writeSync(fd, Buffer.from([0x00, 0x01, 0x00, 0x02]), 0, 4, 0);
writeSync(fd, Buffer.from([0xde, 0xad, 0xbe, 0xef]), 0, 4, size - 4);
closeSync(fd);

const child = spawn(join(root, 'target', 'debug', 'doe'), ['--ide', '--port', process.env.PORT ?? '4174', '--config', join(dir, 'config.toml'), '-I', types, big], { stdio: 'inherit' });
const stop = () => {
  child.kill();
  rmSync(dir, { recursive: true, force: true });
};
process.on('SIGTERM', () => { stop(); process.exit(0); });
process.on('SIGINT', () => { stop(); process.exit(0); });
child.on('exit', (code) => { rmSync(dir, { recursive: true, force: true }); process.exit(code ?? 0); });
