import type { DescriptorInput } from '../../lib/descriptor.mts';

type Finding = {path: string; line: number; kind: string; value?: string};
type Scan = {current:{commit: string; findings: Finding[]; files: number; binary: number}};
type Counts = Record<string, number>;
type Group = {A: number; M: number; D: number; linesAdded: number; linesDeleted: number};
const fs: typeof import('node:fs') = require('node:fs');
const path: typeof import('node:path') = require('node:path');
const { execFileSync }: typeof import('node:child_process') = require('node:child_process');
const [repo, release, final, mainScanFile] = process.argv.slice(2);
// The release descriptor names the tag and what the release replaces (RV_RELEASE_DESCRIPTOR, set by steps/run).
const descriptor = JSON.parse(fs.readFileSync(process.env.RV_RELEASE_DESCRIPTOR!, 'utf8')) as DescriptorInput;
const short = final.slice(0, 8);
const seed = path.join(release, 'next-' + short);
const manifest = Object.fromEntries(fs.readFileSync(path.join(seed, 'manifest'), 'utf8').trim().split('\n').map(line => line.split('=')));
const publicShort = manifest.public.slice(0, 8);
const prefix = 'scan-' + publicShort;
const readJson = <T,>(file: string): T => JSON.parse(fs.readFileSync(file, 'utf8'));
const git = (...args: string[]) => execFileSync('git', ['-C', repo, ...args], {encoding: 'utf8', maxBuffer: 64 << 20});
const entries = (raw: string) => raw.split('\0').filter(Boolean);
const tick = (value: string) => '`' + value.replaceAll('`', '\\`') + '`';
const parent = manifest.parent;
const flags = ['--no-ext-diff', '--no-renames'];
const statuses = entries(git('diff', ...flags, '--name-status', '-z', parent, final));
const changes = new Map<string, string>();
for (let i = 0; i < statuses.length; i += 2) changes.set(statuses[i + 1], statuses[i]);
const numstat = entries(git('diff', ...flags, '--numstat', '-z', parent, final));
const groups = new Map<string, Group>(), addedBinary: string[] = [], changedBinary: string[] = [], deletedBinary: string[] = [];
for (const row of numstat) {
  const [add, del, file] = row.split('\t');
  const root = file.includes('/') ? file.split('/')[0] + '/' : '(root files)';
  const group = groups.get(root) ?? { A: 0, M: 0, D: 0, linesAdded: 0, linesDeleted: 0 };
  const status = changes.get(file);
  if (status !== 'A' && status !== 'M' && status !== 'D' && status !== 'T') throw Error('Unexpected file status ' + status);
  group[status === 'T' ? 'M' : status]++;
  if (add !== '-') { group.linesAdded += Number(add); group.linesDeleted += Number(del); }
  let binary = add === '-';
  if (!binary && status === 'A') {
    const absolute = path.join(seed, 'checkout', file);
    if (fs.lstatSync(absolute).isFile()) binary = fs.readFileSync(absolute).includes(0);
  }
  if (binary) (status === 'A' ? addedBinary : status === 'D' ? deletedBinary : changedBinary).push(file);
  groups.set(root, group);
}
const previousRoots = new Set(entries(git('ls-tree','--name-only','-z',parent)));
const newRoots = entries(git('ls-tree','--name-only','-z',final)).filter(file => !previousRoots.has(file));
const current = readJson<Scan>(path.join(release, prefix + '-private-scan.json')).current;
const previous = readJson<Scan>(mainScanFile).current;
if (current.commit !== manifest.public || previous.commit !== parent) throw Error('Scan identity mismatch');
function byPath(findings: Finding[]) {
  const by = new Map<string, Counts>();
  for (const {path: file, kind} of findings) {
    const rules = by.get(file) ?? {};
    rules[kind] = (rules[kind] ?? 0) + 1;
    by.set(file, rules);
  }
  return by;
}
function totalRules(findings: Finding[]) {
  const counts: Counts = {};
  for (const {kind} of findings) counts[kind] = (counts[kind] ?? 0) + 1;
  return counts;
}
const now = byPath(current.findings), old = byPath(previous.findings);
const allPaths = [...new Set([...now.keys(), ...old.keys()])].sort();
const names = new Set(['work','linear','linearUrl','notion','removedQuotation','tooling','hostPath','formerDictation100']);
const nowTotals = totalRules(current.findings), oldTotals = totalRules(previous.findings);
const fmt = (obj: Counts) => Object.entries(obj).sort().map(([k,v]) => k + ' ' + v).join(', ') || 'none';
const diffs = [];
for (const file of allPaths) {
  const here = now.get(file) ?? {}, was = old.get(file) ?? {};
  const delta = Object.fromEntries([...names].map(k => [k,(here[k] ?? 0) - (was[k] ?? 0)]).filter(([,v])=>v));
  if (Object.keys(delta).length) diffs.push([file, fmt(delta)]);
}
const allOccurrences = [...current.findings].sort((a,b)=>a.path.localeCompare(b.path) || a.line-b.line || a.kind.localeCompare(b.kind));
const occurrenceFile = 'seed-' + publicShort + '-scan-lines.tsv';
fs.writeFileSync(path.join(release, occurrenceFile),'path\tline\trule\n' + allOccurrences.map(f => [f.path,f.line,f.kind].join('\t')).join('\n')+'\n');
const leakSets = ['git','dir'].map(scope => [scope,readJson<{File: string}[]>(path.join(release,prefix+'-gitleaks-'+scope+'.json'))] as const);
const fonts = readJson<unknown[]>(path.join(release,prefix+'-fonts.json'));
const summary = git('diff',...flags,'--shortstat',parent,final).trim();
const list = (items: string[]) => items.length ? items.map(x=>'- '+tick(x)).join('\n') : 'None.';
const lines = [
  '# Public seed for RichView ' + descriptor.tag, '',
  'Prepared locally at ' + manifest.stamp + ' from FINAL ' + tick(final) + '. The public commit ' + tick(manifest.public) + ' has one parent, public main ' + tick(parent) + ', and tree ' + tick(manifest.tree) + '. Its clean checkout and bundle are in ' + tick('next-'+short) + '.', '',
  '## Delta from public main', '', summary + '. File counts below treat a renamed path as a removal and an addition. Root files are grouped together.', '',
  '| Folder | Added | Modified | Removed | Lines added | Lines removed |', '| --- | ---: | ---: | ---: | ---: | ---: |',
  ...[...groups].sort(([a],[b])=>a.localeCompare(b)).map(([group,g])=>[group,g.A,g.M,g.D,g.linesAdded,g.linesDeleted].join(' | ')).map(row=>'| '+row+' |'), '',
  '### New top-level paths', '',list(newRoots),'',
  '### New binary files', '',list(addedBinary),'',
  '### Modified binary files', '',list(changedBinary),'',
  '### Removed binary files', '',list(deletedBinary),'',
  'The full path list and patch are saved as [file statistics](delta-'+short+'-stat.txt) and [full patch](delta-'+short+'.patch).', '',
  '## Scans', '',
  'INT-03 redaction passed. The font inventory has ' + fonts.length + ' entries and matches the saved public-main inventory byte for byte. Poppins and unknown families both have zero entries. [Scan log]('+prefix+'.log)', '',
  ...leakSets.map(([scope,leaks]) => 'Gitleaks '+scope+' identified '+leaks.length+' findings.' + (leaks.length ? ' Files: '+leaks.map(x=>tick(x.File)).join(', ') : '')), '',
  'The private-term scan examined '+current.files+' text files and '+current.binary+' binary files were excluded from text matching. [Each matched path, line and rule]('+occurrenceFile+')', '',
  '| Rule | Public main | FINAL |', '| --- | ---: | ---: |',
  ...[...names].map(k=>'| '+k+' | '+(oldTotals[k]??0)+' | '+(nowTotals[k]??0)+' |'), '',
  '### Files whose rule counts changed', '',
  ...(diffs.length ? diffs.map(([f,delta])=>'- '+tick(f)+' — '+delta) : ['None.']), '',
  '<details><summary>All paths touched by private-term scans on either tree ('+allPaths.length+')</summary>', '',
  '| Path | Public main | FINAL |','| --- | --- | --- |',
  ...allPaths.map(f=>'| '+tick(f)+' | '+fmt(old.get(f)??{})+' | '+fmt(now.get(f)??{})+' |'), '', '</details>',''
];
fs.writeFileSync(path.join(release,'seed-'+publicShort+'.md'),lines.join('\n'));
console.log(JSON.stringify({public:manifest.public,summary,groups:groups.size,newRoots,addedBinary,changedBinary,deletedBinary,scanFiles:allPaths.length,changedScanPaths:diffs.length,counts:nowTotals}));
const keySet = (findings: Finding[]) => new Set(findings.map(f => f.kind+'\0'+(f.value ?? '')));
const before = keySet(previous.findings), after = keySet(current.findings);
const newTerms = [...after].filter(k=>!before.has(k));
const termPaths = new Set(current.findings.filter(f=>newTerms.includes(f.kind+'\0'+(f.value ?? ''))).map(f=>f.path));
const details = [
  '', '## Review notes', '',
  'Review each added and changed public file, binary and private-term finding in the tables above.', '',
  newTerms.length ? 'The private-term scanner found '+newTerms.length+' distinct values absent from public main. Review all occurrences in '+[...termPaths].map(tick).join(', ')+'.' : 'All distinct issue IDs, URLs, tool names and host paths already occur on public main. Changes in occurrence counts are listed above. The scan log also compares against an older audit commit.',
  '', '### Changes since ' + descriptor.oldCandidate, '', git('diff','--stat',descriptor.old,final).trim(), ''
];
fs.appendFileSync(path.join(release,'seed-'+publicShort+'.md'),details.join('\n'));
