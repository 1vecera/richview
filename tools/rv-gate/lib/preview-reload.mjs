// Host reason (ARC-03): bound the playground reload that recovers a preview frame Playwright's Firefox lost.
//
// Playwright's Firefox (Juggler) can miss a srcdoc navigation's start and then give the new document
// no execution context (microsoft/playwright#43124): Firefox draws the chart, but every locator and
// evaluate in the frame waits forever. verify-browser.mjs then reloads the playground once and runs
// the same draw and the same assertions again, and the receipt lists each recovery (S27-844, lead,
// 3 Nov). The recovery goes when DEL-49's pin reaches a stable Playwright with the fix.
import { appendFileSync } from 'node:fs';
import { isAbsolute } from 'node:path';

export const PREVIEW_RELOAD = { issue: 'microsoft/playwright#43124', maxRecoveries: 5, maxPerCase: 2 };

/**
 * Why a draw that timed out may not reload the playground, or null when it may. `failure` is what
 * verify-browser's capturePreviewFailure read. Only Firefox reloads, only when the parent rendered
 * the source and neither preview frame answered within its two seconds, at most `maxRecoveries`
 * times in one run and at most `maxPerCase` times for one case.
 */
export function reloadRefusal(failure, { engine, reloads, title }) {
  if (engine !== 'firefox') return `${engine} never reloads the preview`;
  if (failure.parent?.hasExpected !== true) return 'the parent has not rendered this source';
  if (!failure.snapshotTimeout || Object.hasOwn(failure, 'currentFrame') || Object.hasOwn(failure, 'retainedFrame')) return 'the preview frame answered, so the driver has not lost it';
  if (reloads.filter(reload => reload.case === title).length >= PREVIEW_RELOAD.maxPerCase) return `the preview already reloaded ${PREVIEW_RELOAD.maxPerCase} times in the same case`;
  if (reloads.length >= PREVIEW_RELOAD.maxRecoveries) return `the run already reloaded the preview ${PREVIEW_RELOAD.maxRecoveries} times`;
  return null;
}

/** Write one recovery to the gate's browser event journal, which puts it in the step's receipt details. */
export function recordPreviewReload({ title, recovery, screenshot }, journal = process.env.RICHVIEW_BROWSER_EVENT_LOG) {
  if (journal && !isAbsolute(journal)) throw new Error('RICHVIEW_BROWSER_EVENT_LOG must be an absolute path');
  const line = JSON.stringify({ version: 1, event: 'firefox-preview-reload', surface: 'playwright', engine: 'firefox', issue: PREVIEW_RELOAD.issue, recovery, maxRecoveries: PREVIEW_RELOAD.maxRecoveries, case: title, screenshot });
  if (journal) appendFileSync(journal, line + '\n', { mode: 0o600 });
  console.error('RICHVIEW_PREVIEW_RELOAD ' + line);
}

/** The line a full's summary and a candidate's go card print: how many reloads, and each one's step, case and screenshot. */
export function previewReloadLine(steps) {
  const reloads = steps.flatMap(step => (step.details?.previewReloads ?? []).map(reload => `${step.id}: ${reload.case} (screenshot \`${reload.screenshot}\`)`));
  return `Firefox preview reloads (${PREVIEW_RELOAD.issue}): ${reloads.length ? `${reloads.length}. ${reloads.join('; ')}` : 'none'}.`;
}
