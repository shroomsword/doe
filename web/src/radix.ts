// How integers are shown in the tree: in decimal, hexadecimal, binary or octal.
// The same choice exists in the terminal viewer, and the strings are the same:
// `formatInteger` is `doe::radix::format_integer` in TypeScript, and both are
// tested with one table of values (crates/doe/tests/radix.json). The number
// arrives as decimal text and is handled as a BigInt, never as a JavaScript
// number, which would lose digits of a 64-bit value.

import type { TraceNode } from './doe.js';

export type Radix = 'dec' | 'hex' | 'bin' | 'oct';

export const RADIXES: readonly { id: Radix; name: string }[] = [
  { id: 'dec', name: 'decimal' },
  { id: 'hex', name: 'hexadecimal' },
  { id: 'bin', name: 'binary' },
  { id: 'oct', name: 'octal' },
];

export const isRadix = (value: unknown): value is Radix => RADIXES.some((radix) => radix.id === value);

const PREFIX = { hex: '0x', bin: '0b', oct: '0o' } as const;
const BASE = { hex: 16, bin: 2, oct: 8 } as const;
const BITS_PER_DIGIT = { hex: 4, bin: 1, oct: 3 } as const;

/**
 * An integer, given as decimal text, in a radix: `0x001f`, `0b00011111`,
 * `0o37`, `-0x1f`. `bits` is the width of the type, to which the digits are
 * padded; null when the value has no width (an instance).
 */
export function formatInteger(decimal: string, bits: number | null, radix: Radix): string {
  const value = BigInt(decimal);
  if (radix === 'dec') return value.toString();
  const digits = (value < 0n ? -value : value).toString(BASE[radix]);
  const width = bits === null ? 0 : Math.ceil(bits / BITS_PER_DIGIT[radix]);
  return `${value < 0n ? '-' : ''}${PREFIX[radix]}${digits.padStart(width, '0')}`;
}

/** The width in bits of the node's value, when it was read from the input. */
export function widthOf(node: TraceNode): number | null {
  return node.start !== null && node.end !== null && node.end > node.start ? node.end - node.start : null;
}

/**
 * How a node's integer is written, or null when the node holds none. An enum
 * member is its name and the number in the radix: `text (0x01)`.
 */
export function integerText(node: TraceNode, radix: Radix): string | null {
  if (!node.integer) return null;
  const number = formatInteger(node.integer.value, widthOf(node), radix);
  return node.integer.name === null ? number : `${node.integer.name} (${number})`;
}

/**
 * The name under which a field's own radix is kept: its type and its name, so
 * that it holds for every element of a list and for the next parse of the same
 * schema, and not for a position in the tree. An element of a list is the
 * field of the list, with `[]`.
 */
export function fieldKey(parentKey: string, parent: TraceNode, node: TraceNode): string {
  if (node.name.startsWith('[')) return `${parentKey}[]`;
  return `${parent.type ?? parent.name}.${node.name}`;
}

const GLOBAL_KEY = 'doe.radix';

/** The radix of the page: one for all integers, and the ones that fields have of their own. */
export class Radixes {
  /** The radix of every integer that has none of its own. */
  global: Radix = 'dec';
  private readonly fields = new Map<string, Radix>();

  constructor(private readonly onChange: () => void) {
    try {
      const saved = localStorage.getItem(GLOBAL_KEY);
      if (isRadix(saved)) this.global = saved;
    } catch {
      // no storage: the choice is not remembered
    }
  }

  setGlobal(radix: Radix): void {
    this.global = radix;
    try {
      localStorage.setItem(GLOBAL_KEY, radix);
    } catch {
      // no storage
    }
    this.onChange();
  }

  /** Give a field a radix of its own; null takes it back to the global one. */
  setField(key: string, radix: Radix | null): void {
    if (radix === null) this.fields.delete(key);
    else this.fields.set(key, radix);
    this.onChange();
  }

  of(key: string): Radix {
    return this.fields.get(key) ?? this.global;
  }

  own(key: string): Radix | null {
    return this.fields.get(key) ?? null;
  }

  /** The choices of fields, to keep with the work. */
  toJSON(): Record<string, Radix> {
    return Object.fromEntries(this.fields);
  }

  /** Replace the choices of fields (when other work is opened). */
  replace(fields: Record<string, Radix> | undefined): void {
    this.fields.clear();
    for (const [key, radix] of Object.entries(fields ?? {})) this.fields.set(key, radix);
    this.onChange();
  }

  get count(): number {
    return this.fields.size;
  }
}
