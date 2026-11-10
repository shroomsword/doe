// Files inside files in the page: a `.tar.gz` is a gzip file that holds a tar. The page follows the `payload:` of the schema it
// found (the same rule as `doe::layers`), decoding in the worker, and keeps the layers it found so that the person can look at
// any of them. No DOM here.

import type { Work } from './share.js';

/** How many layers are followed at most (a gzip of a tar of ...), as `doe::layers::MAX_DEPTH`. */
export const MAX_LAYERS = 4;

/**
 * The name the payload of a file would have: the name without its last extension, and `.tar` for the extensions that stand for
 * a compressed tar (`x.tgz` holds `x.tar`), as `doe::layers::payload_name`.
 */
export function payloadName(fileName: string): string {
  const dot = fileName.lastIndexOf('.');
  if (dot <= 0) return fileName;
  const stem = fileName.slice(0, dot);
  switch (fileName.slice(dot + 1).toLowerCase()) {
    case 'tgz':
    case 'taz':
    case 'tbz':
    case 'tbz2':
    case 'txz':
    case 'tzst':
      return `${stem}.tar`;
    default:
      return stem;
  }
}

/** One layer: the file itself, or what a layer holds. */
export interface Layer {
  /** The name of the file or of the payload (`x.tar.gz`, `x.tar`). */
  name: string;
  bytes: Uint8Array;
  /** The schema that fits the layer, with what it imports (what `applyWork` takes). */
  work: Work;
  /** The type that parses it, for the breadcrumb. */
  type: string;
}

/** "gzip_file > tar", as the command line says it. */
export function describeLayers(layers: readonly Layer[]): string {
  return layers.map((layer) => layer.type).join(' > ');
}
