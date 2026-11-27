import {readFileSync} from 'node:fs';
import {homedir} from 'node:os';
import {join} from 'node:path';

// The Windows spend for a candidate has its own board ask and its own ceiling, named in the release descriptor (RV_RELEASE_DESCRIPTOR).
// The cost guard counts only this release's windows-<sha> folders (cost.mjs).
import type { DescriptorInput } from "../../lib/descriptor.mts";

type Answer = {ask: string; at: string; option: string; label: string; note: string};
const desc = JSON.parse(readFileSync(process.env.RV_RELEASE_DESCRIPTOR!, 'utf8')) as DescriptorInput;
export const board = desc.windows.ask;
const yes = desc.windows.ceilingUsd.toFixed(2);
const ledger = join(homedir(), 'tmp/richview-rewrite/status/answers.jsonl');
const options = new Map([
  [yes, desc.windows.yes],
  ['no', desc.windows.no],
]);

export function latestAnswer(path = ledger): Answer | null {
  let data;
  try { data = readFileSync(path, 'utf8'); }
  catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null; throw error; }
  let latest: Answer | null = null;
  for (const line of data.split(/\r?\n/).filter(line => line.trim())) {
    const row = JSON.parse(line) as Partial<Pick<Answer, "note">> & Omit<Answer, "note">;
    if (row.ask !== board) continue;
    const time = Date.parse(row.at);
    if (!Number.isFinite(time)) throw Error('Invalid ' + board + ' answer timestamp; hold the cut.');
    if (!latest || time >= Date.parse(latest.at)) {
      latest = {ask:board, at:row.at, option:row.option, label:row.label, note:row.note ?? ''};
    }
  }
  if (latest && (!options.has(latest.option) || latest.label !== options.get(latest.option))) {
    throw Error('Unrecognized latest ' + board + ' answer; hold the cut.');
  }
  return latest;
}

export function eligible(answer: Answer | null) {
  if (answer?.option === yes) return 'B site-only, or B combined when the public inputs are ready';
  return 'none — hold the cut';
}

export function authorization(route: string, scope: string, sha: string, answer: Answer | null) {
  if (route !== 'B' || !/^[a-f0-9]{40}$/.test(sha)) throw Error('Invalid route or SHA.');
  const site = scope === 'site' && answer?.option === yes;
  const combined = scope === 'combined' && answer?.option === yes;
  if (!site && !combined) throw Error(board + ' eligible routes: '+eligible(answer));
  const date = String(answer!.at).slice(0, 10);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw Error('Invalid ' + board + ' answer date.');
  return {board, date, route, scope, sha, limit:Number(answer!.option), answer: answer!};
}

export function permits(scope: string, phase: string) {
  return phase === 'settled' || (['combined','site'].includes(scope) && ['site','private-walk'].includes(phase))
    // 'bonus' is the public-only Windows build after a site-only walk of the same candidate (as the previous candidate did).
    || (scope === 'combined' && ['combined','public','bonus'].includes(phase));
}
