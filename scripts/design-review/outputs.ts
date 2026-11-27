// What a review captures: the build under review, its themes, the case list and the
// outputs generated for each case and theme. Nothing here compiles; it reads the
// worktree's existing release build.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, readFileSync, readdirSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import type { BrowserContextOptions } from 'playwright';
import { root, cli as sharedCli } from '../browser-harness.ts';

export const dist = resolve(process.env.RICHVIEW_VERIFY_PUBLIC_DIR || join(root, 'dist/web'));
const cli = resolve(root, sharedCli);
const styles = join(root, 'crates/richview-core/assets');
type ReviewInput = { id: string } & (
  { output: 'report'; source: string; page?: never } |
  { output: 'page'; source?: string; page: string }
);
export type ReviewCase = ReviewInput & { about: string };
interface Theme { id: string; aliases: string[] }
interface Diagnostic { level: string; code: string; message: string; path: string }

const skipped = new Set(['research', 'Rendered', 'tmp', 'node_modules', 'target', '__pycache__']);

export function requireBuild() {
  const missing = [cli, join(dist, 'richview_site_bg.wasm')].filter(path => !existsSync(path));
  if (missing.length) throw new Error(`Build this worktree first (cargo run --release --locked -p richview-build); missing ${missing.map(path => relative(root, path)).join(', ')}`);
}

export function buildFacts() {
  const git = (...args: string[]) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
  const sha256 = (file: string) => createHash('sha256').update(readFileSync(file)).digest('hex');
  return {
    worktree: root, commit: git('rev-parse', 'HEAD'), branch: git('branch', '--show-current'), dirty: git('status', '--porcelain').length > 0,
    version: execFileSync(cli, ['--version'], { encoding: 'utf8' }).trim(), wasm_sha256: sha256(join(dist, 'richview_site_bg.wasm')), cli_sha256: sha256(cli),
  };
}

// Every builtin style, one theme per `<name>.rstyle.yaml` in the core assets.
export async function discoverThemes(): Promise<Theme[]> {
  return readdirSync(styles).filter(file => file.endsWith('.rstyle.yaml')).sort()
    .map(file => ({ id: file.slice(0, -'.rstyle.yaml'.length), aliases: [] }));
}

export function pickThemes(names: readonly string[] | undefined, themes: readonly Theme[]) {
  if (!names) return themes.map(theme => theme.id);
  return names.map(name => {
    const theme = themes.find(t => t.id === name || t.aliases.includes(name));
    if (!theme) throw new Error(`Unknown theme '${name}'; this build exposes ${themes.map(t => t.id).join(', ')}`);
    return theme.id;
  });
}

export function pickCases(ids?: readonly string[], quick?: boolean) {
  const list: { cases: (ReviewCase | (Omit<Extract<ReviewCase, { output: 'report' }>, 'output'> & { output?: never }))[]; quick: string[] } = JSON.parse(readFileSync(new URL('./cases.json', import.meta.url), 'utf8'));
  const duplicate = list.cases.find((item, i) => list.cases.findIndex(c => c.id === item.id) !== i);
  if (duplicate) throw new Error(`Case id '${duplicate.id}' appears twice in cases.json`);
  return (ids ?? (quick ? list.quick : list.cases.map(item => item.id))).map(id => {
    const item = list.cases.find(c => c.id === id);
    if (!item) throw new Error(`Unknown case '${id}'; cases are listed in scripts/design-review/cases.json`);
    if (!producers[item.output ?? 'report']) throw new Error(`Case '${id}' uses output '${item.output}', which the harness cannot capture yet`);
    if (item.source && !existsSync(join(root, item.source))) throw new Error(`Case '${id}' source ${item.source} does not exist`);
    return { output: 'report', ...item } as ReviewCase;
  });
}

// Phone widths use a 2x touch screen; wider viewports a 1x desktop.
export function viewport(width: number) {
  const phone = width < 600;
  return { width, height: phone ? Math.round(width * 844 / 390) : 900, dpr: phone ? 2 : 1, mobile: phone };
}
export type ReviewViewport = ReturnType<typeof viewport>;
export const contextOptions = (view: ReviewViewport, extra: BrowserContextOptions = {}): BrowserContextOptions => ({ viewport: { width: view.width, height: view.height }, deviceScaleFactor: view.dpr, isMobile: view.mobile, hasTouch: view.mobile, colorScheme: 'light', reducedMotion: 'no-preference', ...extra });

// One producer per output kind. Each returns the served URL for a case and theme plus
// the native diagnostics of producing it.
const producers = {
  // A report folder is copied (without research caches or previous renders) and
  // generated with the release CLI, which writes only inside the copy's Rendered/.
  report(item: Extract<ReviewInput, { output: 'report' }>, theme: string, out: string) {
    const folder = join(out, 'reports', item.id);
    if (!existsSync(folder)) cpSync(dirname(join(root, item.source)), folder, { recursive: true, filter: path => !skipped.has(basename(path)) && !basename(path).startsWith('.') });
    const name = `${theme}.html`;
    let result: { diagnostics: Diagnostic[] };
    try {
      result = JSON.parse(execFileSync(cli, ['generate', basename(item.source), '--style', theme, '--output', name, '--format', 'json'], { cwd: folder, encoding: 'utf8', maxBuffer: 64 << 20 }));
    } catch (error) {
      throw new Error(`richview generate failed for ${item.id} (${theme}): ${String((error as Error & { stdout?: string }).stdout || (error as Error).message).slice(0, 800)}`);
    }
    const findings = result.diagnostics.filter(d => d.level !== 'info').map(({ level, code, message, path }) => ({ level, code, message, chart: path }));
    return { url: `/reports/${item.id}/Rendered/${name}`, native: { findings, info: result.diagnostics.length - findings.length } };
  },
  // A page that the build already serves from dist/web; `{theme}` in its path is replaced.
  page(item: Extract<ReviewInput, { output: 'page' }>, theme: string) {
    return { url: `/${item.page.replaceAll('{theme}', encodeURIComponent(theme))}`, native: null };
  },
};
export const produce = (item: ReviewInput, theme: string, out: string) =>
  item.output === 'report' ? producers.report(item, theme, out) : producers.page(item, theme);
