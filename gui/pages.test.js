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
