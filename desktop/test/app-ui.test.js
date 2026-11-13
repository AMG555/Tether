import assert from 'node:assert/strict';
import test from 'node:test';
import { initializeApp } from '../src/main.js';

function createDocument() {
  const elements = new Map(
    ['app-status', 'app-name', 'app-version', 'app-platform']
      .map((id) => [id, { textContent: '', dataset: {} }]),
  );

  return {
    elements,
    getElementById(id) {
      return elements.get(id);
    },
  };
}

test('the app screen displays metadata after the local bridge connects', async () => {
  const document = createDocument();

  await initializeApp(document, async () => ({
    name: 'Tether',
    version: '0.1.0',
    platform: 'windows',
  }));

  assert.equal(document.getElementById('app-name').textContent, 'Tether');
  assert.equal(document.getElementById('app-version').textContent, '0.1.0');
  assert.equal(document.getElementById('app-platform').textContent, 'windows');
  assert.equal(document.getElementById('app-status').dataset.state, 'ready');
});

test('the app screen shows a safe failure state without logging raw details', async (t) => {
  const document = createDocument();
  const messages = [];
  const originalMethods = Object.fromEntries(
    ['error', 'warn', 'log'].map((method) => [method, console[method]]),
  );

  for (const method of Object.keys(originalMethods)) {
    console[method] = (...args) => messages.push(args.join(' '));
  }
  t.after(() => {
    Object.assign(console, originalMethods);
  });

  await initializeApp(document, async () => {
    throw new Error('draft-key-sentinel');
  });

  const status = document.getElementById('app-status');
  assert.equal(status.dataset.state, 'error');
  assert.equal(status.textContent, 'Tether could not load app information.');
  assert.doesNotMatch(status.textContent, /draft-key-sentinel/);
  assert.deepEqual(messages, []);
});
