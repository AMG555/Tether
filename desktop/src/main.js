import { AppApiError, getAppInfo } from './api.js';

const genericFailureMessage = 'Tether could not load app information.';

export async function initializeApp(doc = document, loadAppInfo = getAppInfo) {
  const status = doc.getElementById('app-status');
  status.dataset.state = 'loading';
  status.textContent = 'Connecting to the local app bridge…';

  try {
    const info = await loadAppInfo();
    doc.getElementById('app-name').textContent = info.name;
    doc.getElementById('app-version').textContent = info.version;
    doc.getElementById('app-platform').textContent = info.platform;
    status.dataset.state = 'ready';
    status.textContent = 'Local app bridge connected.';
  } catch (error) {
    status.dataset.state = 'error';
    status.textContent = error instanceof AppApiError
      ? error.message
      : genericFailureMessage;
  }
}

if (typeof document !== 'undefined') {
  void initializeApp();
}
