// The shipped pages are single HTML files with inline scripts. A syntax error in one of them
// renders a blank page and no test failure anywhere — that happened during this project, when a
// stray backtick inside a template literal blanked the Character Studio while every suite stayed
// green. These tests compile the inline scripts the way the browser would.
//
//   node --test gui/pages.test.js

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import vm from 'node:vm';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');

const PAGES = [
  'gui/clip-studio.html',
  'gui/clip-preview.html',
  'assets/pet-studio.html',
];

/** Every inline <script> body in a page, in document order. */
function inlineScripts(html) {
  const out = [];
  const re = /<script([^>]*)>([\s\S]*?)<\/script>/gi;
  let m;
  while ((m = re.exec(html)) !== null) {
    const [, attrs, body] = m;
    if (/\bsrc\s*=/.test(attrs)) continue; // external, checked separately
    if (body.trim()) out.push({ body, module: /type\s*=\s*["']module["']/.test(attrs) });
  }
  return out;
}

for (const page of PAGES) {
  const path = join(root, page);
  if (!existsSync(path)) continue;

  test(`${page}: inline scripts compile`, () => {
    const html = readFileSync(path, 'utf8');
    const scripts = inlineScripts(html);
    assert.ok(scripts.length > 0, 'page has an inline script to check');

    scripts.forEach(({ body, module }, i) => {
      if (module) {
        // `import` is legal in a module script by design; a stray `export` is not.
        assert.ok(
          !/^\s*export\s/m.test(body),
          `${page} script ${i}: export inside an inline module is a syntax error`
        );
        return;
      }
      assert.doesNotThrow(
        () => new vm.Script(body, { filename: `${page}#script${i}` }),
        `${page} script ${i} does not parse — the page will render blank`
      );
    });
  });

  test(`${page}: every id the script looks up exists in the markup`, () => {
    const html = readFileSync(path, 'utf8');
    const scripts = inlineScripts(html).map((s) => s.body).join('\n');
    // Collect ids referenced by $('x') / getElementById('x') / querySelector('#x')
    const wanted = new Set();
    for (const re of [/\$\(\s*['"]([\w-]+)['"]\s*\)/g, /getElementById\(\s*['"]([\w-]+)['"]/g]) {
      let m;
      while ((m = re.exec(scripts)) !== null) wanted.add(m[1]);
    }
    const missing = [...wanted].filter((id) => !new RegExp(`id=["']${id}["']`).test(html));
    assert.deepEqual(missing, [], `${page} looks up ids that no element declares: ${missing}`);
  });

  test(`${page}: no network references`, () => {
    const html = readFileSync(path, 'utf8');
    for (const bad of ['https://', 'http://', '//fonts.', 'cdn.']) {
      // allow the SVG/XML namespace, which is an identifier, not a request
      const stripped = html.replace(/xmlns=["']http:\/\/www\.w3\.org\/2000\/svg["']/g, '');
      assert.ok(!stripped.includes(bad), `${page} references the network: ${bad}`);
    }
  });
}

test('the clip player maths still load and export the documented names', async () => {
  const m = await import('./clip-player-math.js');
  for (const name of ['advanceClip', 'chooseClip', 'placeClipPet', 'clamp']) {
    assert.equal(typeof m[name], 'function', `clip-player-math exports ${name}`);
  }
});

test('companions catalog: exports documented symbols', async () => {
  const m = await import('./companions.js');
  for (const name of ['COMPANIONS', 'COMPANION_STATES', 'COMPANION_BASE_URL', 'clipUrl', 'stillUrl', 'thumbnailUrl']) {
    assert.ok(name in m, `companions.js must export ${name}`);
  }
});

test('companions catalog: no sheru in the main list', async () => {
  const { COMPANIONS } = await import('./companions.js');
  const ids = COMPANIONS.map((c) => c.id);
  assert.ok(!ids.includes('sheru'), 'sheru ships separately — must not appear in the catalog');
});

test('companions catalog: every entry has required fields with correct types', async () => {
  const { COMPANIONS } = await import('./companions.js');
  assert.ok(COMPANIONS.length > 0, 'catalog must not be empty');
  for (const c of COMPANIONS) {
    assert.equal(typeof c.id, 'string', `${c.id ?? '?'}: id must be a string`);
    assert.equal(typeof c.name, 'string', `${c.id}: name must be a string`);
    assert.equal(typeof c.description, 'string', `${c.id}: description must be a string`);
    assert.ok(Array.isArray(c.tags), `${c.id}: tags must be an array`);
    assert.ok(c.id.length > 0, `${c.id}: id must not be empty`);
    assert.ok(c.name.length > 0, `${c.id}: name must not be empty`);
    assert.ok(c.description.length > 0, `${c.id}: description must not be empty`);
    assert.ok(c.tags.length > 0, `${c.id}: tags must have at least one entry`);
    // slugs are lowercase hyphenated only
    assert.match(c.id, /^[a-z][a-z0-9-]*$/, `${c.id}: id must be lowercase-hyphenated`);
  }
});

test('companions catalog: no duplicate ids', async () => {
  const { COMPANIONS } = await import('./companions.js');
  const ids = COMPANIONS.map((c) => c.id);
  const dupes = ids.filter((id, i) => ids.indexOf(id) !== i);
  assert.deepEqual(dupes, [], `duplicate companion ids: ${dupes}`);
});

test('companions catalog: url helpers return correct paths', async () => {
  const { clipUrl, stillUrl, thumbnailUrl, COMPANION_BASE_URL } = await import('./companions.js');
  const base = COMPANION_BASE_URL;
  assert.equal(clipUrl('amma', 'idle'), `${base}/amma/idle.webp`);
  assert.equal(stillUrl('amma', 'walk'), `${base}/amma/walk.still.webp`);
  assert.equal(thumbnailUrl('mittu'), `${base}/mittu/idle.still.webp`);
});

test('companions catalog: 32 companions (all except sheru)', async () => {
  const { COMPANIONS } = await import('./companions.js');
  assert.equal(COMPANIONS.length, 32, `expected 32 companions, got ${COMPANIONS.length}`);
});
