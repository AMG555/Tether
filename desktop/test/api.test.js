import assert from 'node:assert/strict';
import test from 'node:test';
import { AppApiError, getAppInfo } from '../src/api.js';

const appInfo = {
  name: 'Tether',
  version: '0.1.0',
  platform: 'windows',
};

async function withBridge(invoke, run) {
  const previous = globalThis.__TAURI__;
  globalThis.__TAURI__ = { core: { invoke } };

  try {
    await run();
  } finally {
    if (previous === undefined) {
      delete globalThis.__TAURI__;
    } else {
      globalThis.__TAURI__ = previous;
    }
  }
}

test('getAppInfo invokes only the app-information command and returns its metadata', async () => {
  const commands = [];

  await withBridge(async (command) => {
    commands.push(command);
    return appInfo;
  }, async () => {
    assert.deepEqual(await getAppInfo(), appInfo);
    assert.deepEqual(commands, ['get_app_info']);
  });
});

test('getAppInfo uses __TAURI_INTERNALS__ when global __TAURI__ is absent', async () => {
  const previousInternals = globalThis.__TAURI_INTERNALS__;
  const previousTauri = globalThis.__TAURI__;
  delete globalThis.__TAURI__;
  globalThis.__TAURI_INTERNALS__ = {
    invoke: async (command) => {
      assert.equal(command, 'get_app_info');
      return appInfo;
    },
  };

  try {
    assert.deepEqual(await getAppInfo(), appInfo);
  } finally {
    if (previousInternals === undefined) {
      delete globalThis.__TAURI_INTERNALS__;
    } else {
      globalThis.__TAURI_INTERNALS__ = previousInternals;
    }
    if (previousTauri !== undefined) {
      globalThis.__TAURI__ = previousTauri;
    }
  }
});

test('getAppInfo rejects bridge failures without exposing their details', async () => {
  await withBridge(async () => {
    throw new Error('draft-key-sentinel');
  }, async () => {
    await assert.rejects(getAppInfo(), (error) => {
      assert.ok(error instanceof AppApiError);
      assert.equal(error.code, 'APP_INFO_UNAVAILABLE');
      assert.doesNotMatch(JSON.stringify(error), /draft-key-sentinel/);
      return true;
    });
  });
});

test('getAppInfo rejects malformed metadata instead of returning a fallback', async () => {
  await withBridge(async () => ({ name: 'Tether' }), async () => {
    await assert.rejects(getAppInfo(), (error) => {
      assert.ok(error instanceof AppApiError);
      assert.equal(error.code, 'INVALID_RESPONSE');
      return true;
    });
  });
});
