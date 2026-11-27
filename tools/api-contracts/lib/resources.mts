// Host reason (ARC-03): verify repository assets and their explicit consumer bindings before
// builds. These filesystem checks never load a runtime or fetch an external resource.
import { createHash } from 'node:crypto';
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { rustTokens } from './rust-surface.mts';
import type { Diagnostic, Node, SourceFile } from 'typescript';

export type ResourceBinding = { consumer: string; source: string; include?: {macro: string; path: string}; entry?: string };
export type IndirectBinding = { consumer: string; producer: string; contract: string };
type ResourceContract = { id: string; producer: string; bindings: ResourceBinding[]; entries?: string[] };

const require = createRequire(import.meta.url);

/** Resolve a regular file inside the repository, including through any symlink. */
export function repositoryFile(root: string, path: unknown) {
  if (typeof path !== 'string' || !path || isAbsolute(path) || path.includes('\\') || path.split('/').some(part => !part || part === '.' || part === '..')) throw new Error(`Expected a canonical repository-relative file: ${path}`);
  const base = realpathSync(root);
  const full = realpathSync(resolve(base, path));
  if (!full.startsWith(base + sep) || !statSync(full).isFile()) throw new Error(`File escapes the repository or is not a regular file: ${path}`);
  return full;
}

function rustString(token: string): string | null {
  const raw = /^(?:b|c)?r(#{0,255})"([\s\S]*)"\1$/.exec(token);
  if (raw) return raw[2];
  if (!/^(?:b|c)?"/.test(token)) return null;
  // Entry names and include paths use ordinary UTF-8 strings; Rust-specific escapes are
  // refused here rather than decoded differently from the compiler.
  try { return JSON.parse(token.replace(/^[bc](?=")/, '')); }
  catch { return null; }
}

/** Literal strings in a consumer, excluding comments, regexes and dynamic templates. */
function strings(file: string, source: string): string[] {
  if (file.endsWith('.rs')) return rustTokens(source).map(rustString).filter(value => value !== null);
  if (/\.[cm]?[jt]s$/.test(file)) {
    // TypeScript is already pinned by the root package; using its parser avoids mistaking
    // a comment or a regex for an actual JavaScript runtime entry string.
    const ts: typeof import('typescript') = require('typescript');
    const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, /\.ts$/.test(file) ? ts.ScriptKind.TS : ts.ScriptKind.JS);
    // The compiler exposes parser diagnostics at runtime but its public SourceFile type omits the field.
    if ((tree as SourceFile & {parseDiagnostics: readonly Diagnostic[]}).parseDiagnostics.length) throw new Error(`Cannot parse consumer ${file}`);
    const values: string[] = [];
    const visit = (node: Node) => {
      if (ts.isStringLiteralLike(node)) values.push(node.text);
      ts.forEachChild(node, visit);
    };
    visit(tree);
    return values;
  }
  throw new Error(`Unsupported consumer syntax: ${file}`);
}

function observedBindings(root: string, contract: ResourceContract, file?: string) {
  if (!Array.isArray(contract.bindings) || !contract.bindings.length) throw new Error(`Resource ${contract.id} needs explicit consumer bindings`);
  const bindings: ResourceBinding[] = [], indirect: IndirectBinding[] = [];
  for (const binding of contract.bindings) {
    if (typeof binding.consumer !== 'string' || !binding.consumer) throw new Error('Resource binding needs a consumer component');
    const consumer = repositoryFile(root, binding.source);
    const source = readFileSync(consumer, 'utf8');
    if (Boolean(binding.include) === Boolean(binding.entry)) throw new Error('A binding names exactly one include or entry');
    if (binding.include) {
      if (!file) throw new Error('Entry-name contracts cannot contain binary/text include bindings');
      if (!binding.source.endsWith('.rs')) throw new Error('A Rust include binding needs a Rust consumer source');
      const { macro, path } = binding.include;
      if (!['include_str', 'include_bytes'].includes(macro) || typeof binding.include.path !== 'string') throw new Error('An include binding needs its macro and literal path');
      const tokens = rustTokens(source);
      const observed = tokens.some((token, at) => token === macro && tokens[at + 1] === '!' && tokens[at + 2] === '(' && tokens[at + 4] === ')' && rustString(tokens[at + 3]) === path);
      if (!observed) throw new Error(`${binding.source} does not include the registered literal ${binding.include.path}`);
      const included = realpathSync(resolve(dirname(consumer), binding.include.path));
      if (included !== file) throw new Error(`${binding.source} includes a different producer resource`);
      bindings.push({ consumer: binding.consumer, source: binding.source, include: { macro, path: binding.include.path } });
    } else {
      if (typeof binding.entry !== 'string' || !binding.entry) throw new Error('An entry binding needs a nonempty literal name');
      if (contract.entries && !contract.entries.includes(binding.entry)) throw new Error(`Unregistered public entry ${binding.entry}`);
      if (!strings(binding.source, source).includes(binding.entry)) throw new Error(`${binding.source} does not name runtime entry ${binding.entry} as a literal`);
      bindings.push({ consumer: binding.consumer, source: binding.source, entry: binding.entry });
    }
    indirect.push({ consumer: binding.consumer, producer: contract.producer, contract: contract.id });
  }
  const order = (value: ResourceBinding) => JSON.stringify(value);
  bindings.sort((a, b) => order(a) < order(b) ? -1 : order(a) > order(b) ? 1 : 0);
  return { bindings, indirect };
}

/** Snapshot one producer asset and prove each declared consumer still names it. */
export function resourceSurface(root: string, contract: ResourceContract & {source: string}) {
  const file = repositoryFile(root, contract.source);
  const { bindings, indirect } = observedBindings(root, contract, file);
  return {
    surface: { source: relative(realpathSync(root), file).split(sep).join('/'), sha256: createHash('sha256').update(readFileSync(file)).digest('hex'), bindings },
    indirect,
  };
}

/** Guard producer-approved runtime names without staging or substituting runtime bytes. */
export function entrySurface(root: string, contract: ResourceContract & {entries: string[]}) {
  const { bindings, indirect } = observedBindings(root, contract);
  for (const entry of contract.entries) if (!bindings.some(binding => binding.entry === entry)) throw new Error(`Public entry ${entry} has no observed binding`);
  return { surface: { entries: [...contract.entries].sort(), bindings }, indirect };
}
