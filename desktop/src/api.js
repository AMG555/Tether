const errorMessages = Object.freeze({
  BRIDGE_UNAVAILABLE: 'The desktop app bridge is unavailable.',
  INVALID_RESPONSE: 'Tether returned invalid app information.',
  APP_INFO_UNAVAILABLE: 'Tether could not load app information.',
});

/**
 * @typedef {{ name: string, version: string, platform: string }} AppInfo
 */

export class AppApiError extends Error {
  constructor(code) {
    const message = errorMessages[code];
    if (!message) {
      throw new TypeError('Unknown app API error code');
    }

    super(message);
    this.name = 'AppApiError';
    this.code = code;
  }

  toJSON() {
    return { code: this.code, message: this.message };
  }
}

function isAppInfo(value) {
  return value !== null
    && typeof value === 'object'
    && typeof value.name === 'string'
    && value.name.length > 0
    && typeof value.version === 'string'
    && value.version.length > 0
    && typeof value.platform === 'string'
    && value.platform.length > 0;
}

/** @returns {Promise<AppInfo>} */
export async function getAppInfo() {
  try {
    const invoke = globalThis.__TAURI__?.core?.invoke
      ?? globalThis.__TAURI_INTERNALS__?.invoke;
    if (typeof invoke !== 'function') {
      throw new AppApiError('BRIDGE_UNAVAILABLE');
    }

    const info = await invoke('get_app_info');
    if (!isAppInfo(info)) {
      throw new AppApiError('INVALID_RESPONSE');
    }

    return info;
  } catch (error) {
    if (error instanceof AppApiError) {
      throw error;
    }

    throw new AppApiError('APP_INFO_UNAVAILABLE');
  }
}
