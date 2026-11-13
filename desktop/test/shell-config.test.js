import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { checkPerformance } from '../scripts/check-perf.js';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

async function readJson(relativePath) {
  return JSON.parse(await readFile(path.join(desktopRoot, relativePath), 'utf8'));
}

test('Tauri bundles only the local UI and uses an offline-only CSP', async () => {
  const config = await readJson('src-tauri/tauri.conf.json');

  assert.equal(config.build.frontendDist, '../src');
  assert.equal(config.build.devUrl, undefined);
  assert.equal(config.app.withGlobalTauri, true);
  assert.equal(config.app.security.csp, [
    "default-src 'self' asset: http://asset.localhost",
    'connect-src ipc: http://ipc.localhost',
    "img-src 'self' asset: http://asset.localhost data: blob:",
    "style-src 'self'",
    "script-src 'self'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
  ].join('; '));
});

test('the main window has no plugin permissions or remote capability origins', async () => {
  const capability = await readJson('src-tauri/capabilities/main.json');

  assert.deepEqual(capability.windows, ['main']);
  assert.ok(capability.permissions.includes('allow-get-app-info'));
  assert.ok(capability.permissions.includes('allow-get-vault-status'));
  assert.ok(capability.permissions.includes('allow-list-drafts'));
  assert.equal(capability.remote, undefined);
  assert.ok(
    capability.permissions.every((perm) => !perm.includes(':') && !perm.startsWith('plugin:')),
    'capability must not include plugin permissions'
  );
});

test('the Tauri build manifest registers only explicitly allowed commands', async () => {
  const buildScript = await readFile(path.join(desktopRoot, 'src-tauri/build.rs'), 'utf8');

  assert.match(buildScript, /commands\(\s*&\[/);
  assert.match(buildScript, /"get_app_info"/);
  assert.match(buildScript, /"list_drafts"/);
  assert.match(buildScript, /"get_vault_status"/);
  assert.equal((buildScript.match(/\.commands\(/g) ?? []).length, 1);
});

test('the Windows build references an existing application icon', async () => {
  const config = await readJson('src-tauri/tauri.conf.json');

  assert.deepEqual(config.bundle.icon, ['icons/icon.ico']);
  const icon = await readFile(path.join(desktopRoot, 'src-tauri', config.bundle.icon[0]));
  assert.ok(icon.length > 0);
});

test('the shell registers no Tauri plugins', async () => {
  const cargoManifest = await readFile(path.join(desktopRoot, 'src-tauri/Cargo.toml'), 'utf8');
  const config = await readJson('src-tauri/tauri.conf.json');

  assert.doesNotMatch(cargoManifest, /tauri-plugin-/);
  assert.equal(config.plugins, undefined);
});

test('bundled frontend sources contain no external network references', async () => {
  const sourceDirectory = path.join(desktopRoot, 'src');
  const files = await readdir(sourceDirectory);
  const source = await Promise.all(files.map((file) => readFile(path.join(sourceDirectory, file), 'utf8')));

  for (const contents of source) {
    assert.doesNotMatch(contents, /https?:\/\/(?!ipc\.localhost\b|asset\.localhost\b)/);
  }
});

test('desktop package satisfies performance and size budget', async () => {
  const perf = await checkPerformance();
  assert.ok(perf.frontendSize < 200 * 1024, 'frontend assets must be under 200 KB');
  if (perf.binarySize !== null) {
    assert.ok(perf.binarySize < 5 * 1024 * 1024, 'release binary must be under 5 MB');
  }
});
