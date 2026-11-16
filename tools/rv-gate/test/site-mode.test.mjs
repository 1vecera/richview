// Host reason (ARC-03): test the local browser-gate expectations for the site's build modes.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { siteModeExpectations } from '../../../scripts/site-mode.mjs';

const chrome = readFileSync(new URL('../../../crates/richview-build/src/chrome.rs', import.meta.url), 'utf8');
const buildPrompt = name => {
  const match = chrome.match(new RegExp(`const ${name}: &str = "([^"]*)";`));
  assert.ok(match, `missing build prompt ${name}`);
  return match[1];
};

test('private install-off site expects the install-free prompt, no repository link and the app sign-in', () => {
  const expected = siteModeExpectations(false, false);
  assert.match(expected.prompt, /^If RichView is installed, run richview learn\./);
  assert.equal(expected.prompt, buildPrompt('AGENT_PROMPT_INSTALL_OFF'));
  assert.doesNotMatch(expected.prompt, /github\.com/);
  assert.deepEqual(expected.header, { links: ['Docs', 'Gallery', 'Sign in'], github: null, motto: 0 });
});

test('public install-on site expects the GitHub prompt and repository link', () => {
  const expected = siteModeExpectations(true, true);
  assert.match(expected.prompt, /^Install RichView from https:\/\/github\.com\/1vecera\/richview\/releases\/latest/);
  assert.equal(expected.prompt, buildPrompt('AGENT_PROMPT'));
  assert.deepEqual(expected.header, { links: ['Docs', 'Gallery', 'RichView on GitHub', 'Sign in'], github: 'https://github.com/1vecera/richview', motto: 0 });
});

test('public install-off keeps the repository link, and private install-on is invalid', () => {
  const expected = siteModeExpectations(false, true);
  assert.match(expected.prompt, /^If RichView is installed/);
  assert.equal(expected.header.github, 'https://github.com/1vecera/richview');
  assert.throws(() => siteModeExpectations(true, false), /public repository/);
});
