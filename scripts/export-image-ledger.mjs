// Host reason (ARC-03): verify the image ledger used by PDF export checks.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

// The ledger's descriptive fields can contain commas, quotes and line breaks. Reject malformed
// CSV rather than giving a font exception to bytes that cannot be tied to a trustworthy row.
function ledgerRows(text) {
  const rows = [];
  let row = [], field = '', state = 'start';
  const finishField = () => { row.push(field); field = ''; state = 'start'; };
  const finishRow = () => { finishField(); rows.push(row); row = []; };
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (state === 'quoted') {
      if (ch === '"' && text[i + 1] === '"') { field += '"'; i++; }
      else if (ch === '"') state = 'closed';
      else field += ch;
    } else if (ch === ',') finishField();
    else if (ch === '\n' || ch === '\r') {
      if (ch === '\r' && text[i + 1] === '\n') i++;
      finishRow();
    } else if (ch === '"' && state === 'start') state = 'quoted';
    else if (state === 'closed' || ch === '"') throw new Error('Malformed image ledger CSV');
    else { field += ch; state = 'plain'; }
  }
  if (state === 'quoted') throw new Error('Malformed image ledger CSV: unterminated quoted field');
  if (row.length || state !== 'start') finishRow();
  const [header, ...records] = rows;
  const required = ['id', 'asset', 'creator', 'source_url', 'source_date', 'collected_at', 'license', 'license_url', 'local_notices', 'changes', 'source_ids', 'sha256', 'collection_run'];
  if (!header || required.some(key => header.filter(column => column === key).length !== 1)) {
    throw new Error('Image ledger has missing or duplicate required columns');
  }
  return records.map(record => {
    if (record.length !== header.length) throw new Error('Malformed image ledger CSV: wrong field count');
    const item = Object.fromEntries(header.map((key, i) => [key, record[i]]));
    if (!item.asset || !/^[0-9a-f]{64}$/.test(item.sha256)) throw new Error('Invalid image ledger asset or SHA-256');
    return item;
  });
}

// The example's SVGs keep their licensed faces across themes only when their bytes still match
// the image ledger. Unembedded PDF fonts remain rejected by the caller.
export function imageFonts(name, folder) {
  if (name !== 'openai-vs-anthropic') return [];
  const rows = ledgerRows(readFileSync(join(folder, 'ledger/image-sources.csv'), 'utf8'));
  return rows.filter(row => row.asset.endsWith('.svg')).map(({ asset: file, sha256 }) => {
    const bytes = readFileSync(join(folder, file));
    if (createHash('sha256').update(bytes).digest('hex') !== sha256) throw new Error(`Ledgered image changed: ${file}`);
    const families = [...bytes.toString('utf8').matchAll(/@font-face\s*\{[^}]*font-family\s*:\s*['"]([^'"]+)['"]/g)]
      .map(([, family]) => family.replace(/\s+/g, ''));
    if (!families.length) throw new Error(`Ledgered image has no embedded font declarations: ${file}`);
    return { file, sha256, families: [...new Set(families)] };
  });
}
