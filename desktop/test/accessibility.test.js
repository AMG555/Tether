import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import {
  checkAccessibility,
  contrastRatio,
  findContrastFailures,
  findSeriousAccessibilityViolations,
} from '../scripts/check-a11y.js';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

test('the desktop screen has no critical or serious accessibility findings', async () => {
  await assert.doesNotReject(checkAccessibility());
});

test('the accessibility check detects an image without alternative text', async () => {
  const html = '<!doctype html><html lang="en"><body><img src="pet.png"></body></html>';
  const violations = await findSeriousAccessibilityViolations(html);

  assert.ok(violations.some(({ id }) => id === 'image-alt'));
});

test('the contrast check enforces WCAG AA text contrast', () => {
  assert.equal(contrastRatio('#000000', '#ffffff'), 21);
  const failures = findContrastFailures(`
    :root {
      --paper: #ffffff;
      --paper-raised: #ffffff;
      --ink: #777777;
      --ink-soft: #333333;
      --ink-faint: #666666;
      --signal: #c2331a;
      --ok: #1f6b3a;
    }
  `);

  assert.ok(failures.some(({ foreground }) => foreground === 'ink'));
});

test('the desktop screen has one page heading and a live app-status region', async () => {
  const html = await readFile(path.join(desktopRoot, 'src', 'index.html'), 'utf8');

  assert.equal((html.match(/<h1\b/g) ?? []).length, 1);
  assert.match(html, /id="app-status"[^>]*role="status"[^>]*aria-live="polite"/);
});
