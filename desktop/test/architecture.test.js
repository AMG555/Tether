import assert from 'node:assert/strict';
import test from 'node:test';
import { checkArchitecture, findBridgeViolations } from '../scripts/check-architecture.js';

test('desktop source confines Tauri IPC to the API adapter', async () => {
  await assert.doesNotReject(checkArchitecture());
});

test('the architecture check detects a direct Tauri call outside the adapter', () => {
  const violations = findBridgeViolations([
    { relativePath: 'api.js', content: 'window.__TAURI__.core.invoke("get_app_info");' },
    { relativePath: 'main.js', content: 'window.__TAURI__.core.invoke("extra_command");' },
  ]);

  assert.deepEqual(violations, ['main.js']);
});
