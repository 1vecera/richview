#!/usr/bin/env node
// Host reason (ARC-03): compare producer-approved API/resource snapshots and emit the gate's
// interface-change report. The checker reads the repository; snapshot preparation writes stdout.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { entrySurface, repositoryFile, resourceSurface } from './lib/resources.mts';
import { publicModule, publicSurface } from './lib/rust-surface.mts';

import type { IndirectBinding, ResourceBinding } from './lib/resources.mts';

type ContractBase = { id: string; producer: string; snapshot: string };
export type RustContract = ContractBase & {kind: 'rust'; source: string; module: string; crateRoot: string; declarations?: {source: string; items: string[]}[]; consumers: string[]};
type Contract = RustContract | (ContractBase & {kind: 'resource'; source: string; bindings: ResourceBinding[]}) | (ContractBase & {kind: 'entries'; entries: string[]; bindings: ResourceBinding[]});
type Manifest = { schema: typeof SCHEMA; contracts: Contract[] };
export type CheckOptions = { root: string; manifest: string; base?: string };
type ApiReport = {
  schema: typeof SCHEMA; status: 'pass' | 'fail'; checked: string[];
  findings: {code: string; message: string; path?: string; contract?: string}[];
  changes: {contract: string; producer: string; consumers: string[]; change: 'changed' | 'added' | 'removed'}[];
  indirect: IndirectBinding[];
};

export const SCHEMA = 'interface-v1';
const sort = <Value,>(values: Value[]) => [...values].sort((a, b) => JSON.stringify(a) < JSON.stringify(b) ? -1 : JSON.stringify(a) > JSON.stringify(b) ? 1 : 0);
const canonical = (value: unknown): unknown => Array.isArray(value) ? value.map(canonical) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical((value as Record<string, unknown>)[key])])) : value;
const same = (a: unknown, b: unknown) => JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
const strings = (value: unknown): value is string[] => Array.isArray(value) && value.every(item => typeof item === 'string' && item.length);

function keys(value: unknown, allowed: string[], label: string): asserts value is Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${label} must be an object`);
  for (const key of Object.keys(value)) if (!allowed.includes(key)) throw new Error(`Unknown ${label} key ${key}`);
}

function validateContract(contract: unknown): asserts contract is Contract {
  keys(contract, ['id', 'producer', 'kind', 'source', 'snapshot', 'module', 'crateRoot', 'declarations', 'consumers', 'bindings', 'entries'], 'contract');
  if (typeof contract.id !== 'string' || !/^[a-z][a-z0-9._-]*$/.test(contract.id) || typeof contract.producer !== 'string' || !contract.producer) throw new Error('A contract needs a stable id and producer component');
  if (contract.kind !== 'rust' && contract.kind !== 'resource' && contract.kind !== 'entries') throw new Error(`Unsupported contract kind ${contract.kind}`);
  const common = ['id', 'producer', 'kind', 'snapshot'];
  keys(contract, [...common, ...(contract.kind === 'rust' ? ['source', 'module', 'crateRoot', 'declarations', 'consumers'] : contract.kind === 'resource' ? ['source', 'bindings'] : ['entries', 'bindings'])], `${contract.kind} contract`);
  if (typeof contract.snapshot !== 'string' || (contract.kind !== 'entries' && typeof contract.source !== 'string')) throw new Error('A contract needs its source and snapshot paths');
  if (contract.kind === 'entries') {
    if (!strings(contract.entries) || !contract.entries.length || new Set(contract.entries).size !== contract.entries.length) throw new Error('Entry-name contracts need unique public names, not a staged runtime file');
  }
  if (contract.kind === 'rust') {
    if (typeof contract.module !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*::[A-Za-z_][A-Za-z0-9_]*$/.test(contract.module) || typeof contract.crateRoot !== 'string' || !strings(contract.consumers)) throw new Error('A Rust contract needs its crate root, direct public module and consumer list');
    if (contract.declarations !== undefined && !Array.isArray(contract.declarations)) throw new Error('Supplementary declarations must be an array');
    for (const declaration of contract.declarations ?? []) {
      keys(declaration, ['source', 'items'], 'declaration');
      if (typeof declaration.source !== 'string' || !strings(declaration.items) || !declaration.items.length || new Set(declaration.items).size !== declaration.items.length) throw new Error('Supplementary declarations need a source and unique explicit public item names');
    }
  } else {
    if (!Array.isArray(contract.bindings) || !contract.bindings.length) throw new Error('Resource bindings must be nonempty');
    for (const binding of contract.bindings) {
      keys(binding, ['consumer', 'source', 'include', 'entry'], 'binding');
      if (binding.include) keys(binding.include, ['macro', 'path'], 'include');
    }
  }
}

/** Prepare one canonical snapshot of explicitly registered public declarations or resources. */
export function snapshotContract({ root, contract }: {root: string; contract: unknown}) {
  validateContract(contract);
  let surface, indirect: IndirectBinding[] = [];
  if (contract.kind === 'rust') {
    const crateRoot = repositoryFile(root, contract.crateRoot);
    const name = contract.module.split('::')[1];
    const registration = publicModule(readFileSync(crateRoot, 'utf8'), name);
    const files = [join(dirname(crateRoot), `${name}.rs`), join(dirname(crateRoot), name, 'mod.rs')].filter(existsSync);
    const source = repositoryFile(root, contract.source);
    if (files.length !== 1 || realpathSync(files[0]) !== source) throw new Error(`Public module ${contract.module} does not load the registered facade source`);
    const exports = publicSurface(readFileSync(source, 'utf8'));
    if (!exports.length) throw new Error(`Facade ${contract.id} has no explicit public exports`);
    const declarations = (contract.declarations ?? []).flatMap(item => publicSurface(readFileSync(repositoryFile(root, item.source), 'utf8'), { select: item.items }));
    const seen = new Set();
    for (const item of declarations) {
      if (seen.has(item.name)) throw new Error(`Duplicate supplementary declaration ${item.name}`);
      seen.add(item.name);
    }
    surface = { module: contract.module, registration, exports, declarations: sort(declarations) };
  } else ({ surface, indirect } = contract.kind === 'entries' ? entrySurface(root, contract) : resourceSurface(root, contract));
  return { snapshot: { schema: SCHEMA, id: contract.id, producer: contract.producer, kind: contract.kind, surface }, indirect };
}

function validateManifest(manifest: unknown): Manifest {
  keys(manifest, ['schema', 'contracts'], 'manifest');
  if (manifest.schema !== SCHEMA || !Array.isArray(manifest.contracts) || !manifest.contracts.length) throw new Error('Expected a nonempty interface-v1 contract manifest');
  const ids = new Set(), snapshots = new Set();
  for (const contract of manifest.contracts) {
    validateContract(contract);
    if (ids.has(contract.id) || snapshots.has(contract.snapshot)) throw new Error('Contract ids and snapshot paths must be unique');
    ids.add(contract.id); snapshots.add(contract.snapshot);
  }
  return { schema: SCHEMA, contracts: manifest.contracts as Contract[] };
}

function manifestFile(root: string, path: string) {
  return validateManifest(JSON.parse(readFileSync(repositoryFile(root, path), 'utf8')));
}

function gitBase(root: string, base: string) {
  const hash = execFileSync('git', ['rev-parse', '--verify', '--end-of-options', `${base}^{commit}`], { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  if (!/^[a-f0-9]{40,64}$/.test(hash)) throw new Error('Git did not resolve the base to a commit');
  return hash;
}

function priorSnapshot(root: string, hash: string, path: string): unknown {
  // A missing path in a valid tree is an added interface. Git failures remain failures.
  const found = execFileSync('git', ['ls-tree', '--name-only', '-z', hash, '--', path], { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (!found) return null;
  return JSON.parse(execFileSync('git', ['show', `${hash}:${path}`], { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }));
}

/** Check current snapshots and report API/resource changes without hiding reviewed updates. */
export function checkContracts({ root, manifest, base }: CheckOptions): ApiReport {
  const report: ApiReport = { schema: SCHEMA, status: 'fail', checked: [], findings: [], changes: [], indirect: [] };
  try {
    root = realpathSync(root);
    const config = manifestFile(root, manifest);
    const hash = base ? gitBase(root, base) : null;
    const priorConfig = hash ? priorSnapshot(root, hash, manifest) : null;
    const priorContracts = new Map(priorConfig ? validateManifest(priorConfig).contracts.map(contract => [contract.id, contract]) : []);
    for (const contract of config.contracts) {
      try {
        const { snapshot, indirect } = snapshotContract({ root, contract });
        report.indirect.push(...indirect);
        const expected = JSON.parse(readFileSync(repositoryFile(root, contract.snapshot), 'utf8'));
        if (!same(snapshot, expected)) report.findings.push({ contract: contract.id, code: 'snapshot.mismatch', path: contract.snapshot, message: 'Current public interface differs from the reviewed snapshot' });
        const old = priorContracts.get(contract.id);
        const previous = hash ? (old ? priorSnapshot(root, hash, old.snapshot) : null) : expected;
        if (hash && old && !previous) throw new Error(`Base contract ${contract.id} has no reviewed snapshot`);
        if (!same(snapshot, previous)) report.changes.push({ contract: contract.id, producer: contract.producer, consumers: sort(contract.kind === 'rust' ? contract.consumers : [...new Set(indirect.map(binding => binding.consumer))]), change: previous ? 'changed' : 'added' });
        report.checked.push(contract.id);
      } catch (error) {
        report.findings.push({ contract: contract.id, code: 'contract.invalid', message: (error as Error).message });
      }
    }
    if (hash) for (const old of priorContracts.values()) if (!config.contracts.some(contract => contract.id === old.id)) {
      const previous = priorSnapshot(root, hash, old.snapshot);
      if (!previous) throw new Error(`Base contract ${old.id} has no reviewed snapshot`);
      report.changes.push({ contract: old.id, producer: old.producer, consumers: sort(old.kind === 'rust' ? old.consumers : [...new Set(old.bindings.map(binding => binding.consumer))]), change: 'removed' });
    }
    report.indirect = sort([...new Map(report.indirect.map(binding => [JSON.stringify(binding), binding])).values()]);
    report.checked.sort(); report.changes = sort(report.changes);
    if (!report.findings.length && report.checked.length === config.contracts.length) report.status = 'pass';
  } catch (error) {
    report.findings.push({ code: 'manifest.invalid', message: (error as Error).message });
  }
  return report;
}

function main(args: string[]) {
  const [command, ...rest] = args;
  const options: Omit<CheckOptions, 'manifest'> & {manifest?: string; contract?: string} = { root: process.cwd() };
  const seen = new Set();
  for (let at = 0; at < rest.length; at += 2) {
    const flags = { '--root': 'root', '--manifest': 'manifest', '--base': 'base', '--contract': 'contract' } as const;
    const key = flags[rest[at] as keyof typeof flags];
    if (!key || !rest[at + 1] || rest[at + 1].startsWith('--') || seen.has(key)) throw new Error(`Invalid or duplicate checker option ${rest[at]}`);
    seen.add(key);
    options[key] = rest[at + 1];
  }
  if (!options.manifest || !['check', 'snapshot'].includes(command)) throw new Error('Usage: check.mjs check|snapshot --root <repo> --manifest <json> [--base <gitref>]');
  if ((command === 'check' && options.contract) || (command === 'snapshot' && options.base)) throw new Error('--contract selects a snapshot to prepare; --base compares a complete check');
  if (command === 'check') {
    const report = checkContracts({ ...options, manifest: options.manifest });
    process.stdout.write(JSON.stringify(report, null, 2) + '\n');
    process.exitCode = report.status === 'pass' ? 0 : 1;
  } else {
    const manifest = manifestFile(options.root, options.manifest);
    const contracts = options.contract ? manifest.contracts.filter(contract => contract.id === options.contract) : manifest.contracts;
    if (!contracts.length) throw new Error(`Unknown contract ${options.contract}`);
    const snapshots = contracts.map(contract => ({ path: contract.snapshot, snapshot: snapshotContract({ root: options.root, contract }).snapshot }));
    process.stdout.write(JSON.stringify(options.contract ? snapshots[0].snapshot : { schema: SCHEMA, snapshots }, null, 2) + '\n');
  }
}

if (process.argv[1] && (resolve(process.argv[1]) === fileURLToPath(import.meta.url) ||
	resolve(process.argv[1]) === fileURLToPath(import.meta.url.replace(/\.mts$/, ".mjs")))) {
  try { main(process.argv.slice(2)); }
  catch (error) {
    process.stdout.write(JSON.stringify({ schema: SCHEMA, status: 'fail', checked: [], findings: [{ code: 'arguments.invalid', message: (error as Error).message }], changes: [], indirect: [] }) + '\n');
    process.exitCode = 2;
  }
}
