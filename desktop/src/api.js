const errorMessages = Object.freeze({
  BRIDGE_UNAVAILABLE: 'The desktop app bridge is unavailable.',
  INVALID_RESPONSE: 'Tether returned invalid app information.',
  APP_INFO_UNAVAILABLE: 'Tether could not load app information.',
  VAULT_ERROR: 'Could not access the encrypted vault.',
  OPERATION_FAILED: 'The requested operation failed.',
});

/**
 * @typedef {{ name: string, version: string, platform: string }} AppInfo
 * @typedef {{ ready: boolean, draftCount: number, retentionDays: number, vaultPath: string, encryption: string, offline: boolean }} VaultStatus
 * @typedef {{ id: number, app: string, preview: string, chars: number, revisions: number, updatedAt: number, ageHuman: string, daysLeft: number }} DraftItem
 * @typedef {{ id: number, app: string, text: string, chars: number, revisions: number, updatedAt: number, ageHuman: string, daysLeft: number }} DraftDetail
 * @typedef {{ id: string, name: string, description: string, tags: string[], isCustom: boolean, isInstalled: boolean, avatarType: string, actions: string[], colors?: { body: string, accent: string, eyes: string } }} CompanionItem
 */

export class AppApiError extends Error {
  constructor(code, detail = null) {
    const message = errorMessages[code] || detail || 'An unknown error occurred.';
    super(message);
    this.name = 'AppApiError';
    this.code = code;
  }

  toJSON() {
    return { code: this.code, message: this.message };
  }
}

function getInvoker() {
  const invoke = globalThis.__TAURI__?.core?.invoke
    ?? globalThis.__TAURI_INTERNALS__?.invoke;
  return typeof invoke === 'function' ? invoke : null;
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
    const invoke = getInvoker();
    if (!invoke) {
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

/** @returns {Promise<VaultStatus>} */
export async function getVaultStatus() {
  const invoke = getInvoker();
  if (!invoke) {
    return {
      ready: true,
      draftCount: 0,
      retentionDays: 14,
      vaultPath: '~/.local/share/tether',
      encryption: 'AES-256-GCM + HKDF',
      offline: true,
    };
  }
  try {
    return await invoke('get_vault_status');
  } catch (error) {
    throw new AppApiError('VAULT_ERROR', String(error));
  }
}

/**
 * @param {string|null} [query]
 * @param {number} [limit]
 * @returns {Promise<DraftItem[]>}
 */
export async function listDrafts(query = null, limit = 100) {
  const invoke = getInvoker();
  if (!invoke) {
    return [];
  }
  try {
    return await invoke('list_drafts', { query, limit });
  } catch (error) {
    throw new AppApiError('VAULT_ERROR', String(error));
  }
}

/**
 * @param {number} id
 * @returns {Promise<DraftDetail>}
 */
export async function getDraft(id) {
  const invoke = getInvoker();
  if (!invoke) {
    throw new AppApiError('BRIDGE_UNAVAILABLE');
  }
  try {
    return await invoke('get_draft', { id });
  } catch (error) {
    throw new AppApiError('VAULT_ERROR', String(error));
  }
}

/**
 * @param {number} id
 * @returns {Promise<string[]>}
 */
export async function getRevisions(id) {
  const invoke = getInvoker();
  if (!invoke) {
    return [];
  }
  try {
    return await invoke('get_revisions', { id });
  } catch (error) {
    throw new AppApiError('VAULT_ERROR', String(error));
  }
}

/**
 * @param {number} id
 * @returns {Promise<void>}
 */
export async function deleteDraft(id) {
  const invoke = getInvoker();
  if (!invoke) {
    return;
  }
  try {
    await invoke('delete_draft', { id });
  } catch (error) {
    throw new AppApiError('OPERATION_FAILED', String(error));
  }
}

/**
 * @param {number} [maxDays]
 * @returns {Promise<number>}
 */
export async function pruneVault(maxDays) {
  const invoke = getInvoker();
  if (!invoke) {
    return 0;
  }
  try {
    return await invoke('prune_vault', { maxDays: maxDays ?? null });
  } catch (error) {
    throw new AppApiError('OPERATION_FAILED', String(error));
  }
}

/**
 * @param {number} days
 * @returns {Promise<number>}
 */
export async function setRetentionDays(days) {
  const invoke = getInvoker();
  if (!invoke) {
    return days;
  }
  try {
    return await invoke('set_retention_days', { days });
  } catch (error) {
    throw new AppApiError('OPERATION_FAILED', String(error));
  }
}

/** @returns {Promise<boolean>} */
export async function getAutostart() {
  const invoke = getInvoker();
  if (!invoke) {
    return false;
  }
  try {
    return await invoke('get_autostart_status');
  } catch {
    return false;
  }
}

/**
 * @param {boolean} enabled
 * @returns {Promise<boolean>}
 */
export async function setAutostart(enabled) {
  const invoke = getInvoker();
  if (!invoke) {
    return enabled;
  }
  try {
    return await invoke('set_autostart', { enabled });
  } catch (error) {
    throw new AppApiError('OPERATION_FAILED', String(error));
  }
}

/** @returns {Promise<CompanionItem[]>} */
export async function listCompanions() {
  const invoke = getInvoker();
  if (!invoke) {
    return [
      {
        id: 'grace',
        name: 'Grace',
        description: 'A warm, grounding presence — reminds you to drink water and rest.',
        tags: ['human', 'adult', 'calm'],
        isCustom: false,
        isInstalled: true,
        avatarType: 'procedural',
        actions: ['idle', 'walk', 'wave', 'water', 'stretch'],
        colors: { body: '#2563eb', accent: '#c2331a', eyes: '#14120f' },
      },
      {
        id: 'rory',
        name: 'Rory',
        description: 'The lion cub heir — small, boisterous, and convinced he runs the place.',
        tags: ['animal', 'cub', 'playful'],
        isCustom: false,
        isInstalled: true,
        avatarType: 'procedural',
        actions: ['idle', 'walk', 'happy', 'wave'],
        colors: { body: '#d97706', accent: '#b45309', eyes: '#14120f' },
      },
    ];
  }
  try {
    return await invoke('list_companions');
  } catch (error) {
    throw new AppApiError('OPERATION_FAILED', String(error));
  }
}

/**
 * @param {object} payload
 * @returns {Promise<CompanionItem>}
 */
export async function saveCustomCompanion(payload) {
  const invoke = getInvoker();
  if (!invoke) {
    return { ...payload, id: payload.id || 'custom-pet', isCustom: true, isInstalled: true };
  }
  try {
    return await invoke('save_custom_companion', { payload });
  } catch (error) {
    throw new AppApiError('OPERATION_FAILED', String(error));
  }
}

/**
 * @param {string} id
 * @returns {Promise<void>}
 */
export async function deleteCustomCompanion(id) {
  const invoke = getInvoker();
  if (!invoke) {
    return;
  }
  try {
    await invoke('delete_custom_companion', { id });
  } catch (error) {
    throw new AppApiError('OPERATION_FAILED', String(error));
  }
}

/**
 * @param {string} id
 * @returns {Promise<CompanionItem>}
 */
export async function installCatalogCompanion(id) {
  const invoke = getInvoker();
  if (!invoke) {
    return {
      id,
      name: id,
      description: `Downloaded ${id} pack`,
      tags: ['downloaded'],
      isCustom: false,
      isInstalled: true,
      avatarType: 'procedural',
      actions: ['idle', 'walk', 'wave', 'water', 'happy'],
      colors: { body: '#2563eb', accent: '#c2331a', eyes: '#14120f' },
    };
  }
  try {
    return await invoke('install_catalog_companion', { id });
  } catch (error) {
    throw new AppApiError('OPERATION_FAILED', String(error));
  }
}
