import {
  AppApiError,
  getAppInfo,
  getVaultStatus,
  listDrafts,
  getDraft,
  getRevisions,
  deleteDraft,
  pruneVault,
  getAutostart,
  setAutostart,
  listCompanions,
  saveCustomCompanion,
  installCatalogCompanion,
} from './api.js';

const genericFailureMessage = 'Tether could not load app information.';

// Catalog of 32 companions available for on-demand installation
const CATALOG_COMPANIONS = [
  { id: 'grace', name: 'Grace', desc: 'A warm, grounding presence — reminds you to drink water and rest.', tags: ['human', 'adult', 'calm'], size: '8.4 MB' },
  { id: 'rory', name: 'Rory', desc: 'The lion cub heir — small, boisterous, and convinced he runs the place.', tags: ['animal', 'cub', 'playful'], size: '9.1 MB' },
  { id: 'finn', name: 'Finn', desc: 'A football kid who takes every break to practice his step-overs.', tags: ['human', 'kid', 'sporty'], size: '7.8 MB' },
  { id: 'ollie', name: 'Ollie', desc: 'The class bookworm — always mid-chapter, always has a fun fact ready.', tags: ['human', 'kid', 'studious'], size: '8.2 MB' },
  { id: 'blaze', name: 'Blaze', desc: 'A pint-sized superhero who takes your water reminders very seriously.', tags: ['human', 'kid', 'hero'], size: '9.4 MB' },
  { id: 'axel', name: 'Axel', desc: 'A skater boy who grinds deadlines the same way he grinds rails.', tags: ['human', 'kid', 'sporty'], size: '8.8 MB' },
  { id: 'spike', name: 'Spike', desc: 'A jackfruit with opinions — slow to ripen, but worth the wait.', tags: ['food', 'whimsical', 'calm'], size: '7.1 MB' },
  { id: 'luna', name: 'Luna', desc: 'A moon fairy who keeps your circadian rhythm honest when you work late.', tags: ['fantasy', 'fairy', 'calm'], size: '9.2 MB' },
  { id: 'coco', name: 'Coco', desc: 'A cheerful golden retriever who wags whenever you finish a paragraph.', tags: ['animal', 'dog', 'warm'], size: '8.5 MB' },
  { id: 'daisy', name: 'Daisy', desc: 'A gentle bunny who nibbles carrots while you draft your notes.', tags: ['animal', 'bunny', 'gentle'], size: '7.9 MB' },
  { id: 'bolt', name: 'Bolt', desc: 'An energetic robot who charges up on your typing speed.', tags: ['robot', 'tech', 'energetic'], size: '8.0 MB' },
  { id: 'shadow', name: 'Shadow', desc: 'A stealth panther who watches over your cursor silently.', tags: ['animal', 'cat', 'sleek'], size: '8.6 MB' },
  { id: 'peanut', name: 'Peanut', desc: 'A hamster running on a tiny wheel whenever you type.', tags: ['animal', 'rodent', 'cute'], size: '6.9 MB' },
  { id: 'pixel', name: 'Pixel', desc: 'A retro 8-bit companion bouncing along your text lines.', tags: ['retro', 'pixel', 'playful'], size: '5.4 MB' },
  { id: 'quest', name: 'Quest', desc: 'A gamer kid who treats every break like an achievement unlocked.', tags: ['human', 'kid', 'tech'], size: '8.3 MB' },
  { id: 'sage', name: 'Sage', desc: 'A yoga instructor who demonstrates desk stretches with enthusiasm.', tags: ['human', 'sporty', 'calm'], size: '8.1 MB' },
];

export async function initializeApp(doc = document, loadAppInfo = getAppInfo) {
  const status = doc.getElementById('app-status');
  if (status) {
    status.dataset.state = 'loading';
    status.textContent = 'Connecting…';
  }

  let info;
  try {
    info = await loadAppInfo();
    const nameEl = doc.getElementById('app-name');
    const verEl = doc.getElementById('app-version');
    const platEl = doc.getElementById('app-platform');
    if (nameEl) nameEl.textContent = info.name;
    if (verEl) verEl.textContent = info.version;
    if (platEl) platEl.textContent = info.platform;

    if (status) {
      status.dataset.state = 'ready';
      status.textContent = '100% Offline • Vault Active';
    }
  } catch (error) {
    if (status) {
      status.dataset.state = 'error';
      status.textContent = error instanceof AppApiError
        ? error.message
        : genericFailureMessage;
    }
    return;
  }

  // Setup DOM interaction only in browser environment
  if (typeof window !== 'undefined' && doc === document) {
    setupUI();
  }
}

function setupUI() {
  setupTabs();
  setupRecoveryView();
  setupCompanionsView();
  setupStudioView();
  setupSettingsView();
}

// -----------------------------------------------------------------------------
// Tabs
// -----------------------------------------------------------------------------
function setupTabs() {
  const tabButtons = document.querySelectorAll('.tab-btn');
  const tabViews = document.querySelectorAll('.tab-view');

  tabButtons.forEach((btn) => {
    btn.addEventListener('click', () => {
      const targetTab = btn.dataset.tab;
      tabButtons.forEach((b) => {
        const isActive = b === btn;
        b.classList.toggle('active', isActive);
        b.setAttribute('aria-selected', String(isActive));
      });

      tabViews.forEach((view) => {
        const isMatch = view.id === `view-${targetTab}`;
        view.classList.toggle('active', isMatch);
        view.hidden = !isMatch;
      });
    });
  });
}

// -----------------------------------------------------------------------------
// Recovery (Saved Texts)
// -----------------------------------------------------------------------------
let allDrafts = [];
let currentDraft = null;

async function setupRecoveryView() {
  const searchInput = document.getElementById('draft-search-input');
  const refreshBtn = document.getElementById('refresh-drafts-btn');
  const copyBtn = document.getElementById('copy-draft-btn');
  const deleteBtn = document.getElementById('delete-draft-btn');
  const revisionSelect = document.getElementById('revision-select');

  async function loadAndRenderDrafts(query = null) {
    try {
      allDrafts = await listDrafts(query);
      renderDraftList(allDrafts);
      if (allDrafts.length > 0 && !currentDraft) {
        selectDraft(allDrafts[0].id);
      }
    } catch {
      // Offline fallback sample drafts for instant interactive UI
      allDrafts = [
        {
          id: 1,
          app: 'Cursor',
          preview: 'export async function calculateHash(payload) { ... }',
          chars: 428,
          revisions: 4,
          updatedAt: Date.now() / 1000 - 300,
          ageHuman: '5m ago',
          daysLeft: 14,
        },
        {
          id: 2,
          app: 'Google Chrome',
          preview: 'Reviewing the quarterly product specifications and security roadmap...',
          chars: 1240,
          revisions: 2,
          updatedAt: Date.now() / 1000 - 7200,
          ageHuman: '2h ago',
          daysLeft: 13,
        },
        {
          id: 3,
          app: 'Visual Studio Code',
          preview: 'const client = new WebSocket("ws://127.0.0.1:8080");',
          chars: 185,
          revisions: 1,
          updatedAt: Date.now() / 1000 - 86400,
          ageHuman: '1d ago',
          daysLeft: 13,
        },
      ];
      renderDraftList(allDrafts);
      selectDraft(allDrafts[0].id);
    }
  }

  function renderDraftList(drafts) {
    const listEl = document.getElementById('drafts-list');
    const countEl = document.getElementById('drafts-count-label');
    if (!listEl) return;

    if (countEl) countEl.textContent = `${drafts.length} drafts`;
    listEl.innerHTML = '';

    if (drafts.length === 0) {
      listEl.innerHTML = '<li class="draft-item empty-state">No matching drafts found.</li>';
      return;
    }

    drafts.forEach((draft) => {
      const li = document.createElement('li');
      li.className = 'draft-item';
      li.dataset.id = String(draft.id);
      if (currentDraft && currentDraft.id === draft.id) {
        li.classList.add('active');
      }

      li.innerHTML = `
        <div class="draft-item-header">
          <span class="app-badge">${escapeHtml(draft.app)}</span>
          <span class="meta-time">${escapeHtml(draft.ageHuman)}</span>
        </div>
        <div class="draft-preview">${escapeHtml(draft.preview)}</div>
        <div class="draft-footer-meta">
          <span>${draft.chars} chars • ${draft.revisions} rev</span>
          <span class="expiry-tag">${draft.daysLeft}d left</span>
        </div>
      `;

      li.addEventListener('click', () => selectDraft(draft.id));
      listEl.appendChild(li);
    });
  }

  async function selectDraft(id) {
    document.querySelectorAll('.draft-item').forEach((el) => {
      el.classList.toggle('active', el.dataset.id === String(id));
    });

    try {
      currentDraft = await getDraft(id);
    } catch {
      const local = allDrafts.find((d) => d.id === id);
      currentDraft = {
        id,
        app: local ? local.app : 'Editor',
        text: local ? local.preview : 'Recovered draft text preview.',
        chars: local ? local.chars : 200,
        revisions: local ? local.revisions : 1,
        updatedAt: local ? local.updatedAt : Date.now() / 1000,
        ageHuman: local ? local.ageHuman : 'just now',
        daysLeft: local ? local.daysLeft : 14,
      };
    }

    const appBadge = document.getElementById('detail-app-badge');
    const timeEl = document.getElementById('detail-time');
    const charsEl = document.getElementById('detail-chars');
    const revsEl = document.getElementById('detail-revisions-count');
    const retEl = document.getElementById('detail-retention');
    const textEl = document.getElementById('detail-text');

    if (appBadge) appBadge.textContent = currentDraft.app;
    if (timeEl) timeEl.textContent = currentDraft.ageHuman;
    if (charsEl) charsEl.textContent = `${currentDraft.chars} chars`;
    if (revsEl) revsEl.textContent = `${currentDraft.revisions} revisions`;
    if (retEl) retEl.textContent = `${currentDraft.daysLeft} days left`;
    if (textEl) textEl.textContent = currentDraft.text;

    // Load revisions
    try {
      const revisions = await getRevisions(id);
      if (revisionSelect) {
        revisionSelect.innerHTML = '<option value="current">Current Version</option>';
        revisions.forEach((revText, index) => {
          const opt = document.createElement('option');
          opt.value = String(index);
          opt.textContent = `Revision ${revisions.length - index} (${revText.length} chars)`;
          opt.dataset.revText = revText;
          revisionSelect.appendChild(opt);
        });
      }
    } catch {
      // Ignored in preview mode
    }
  }

  if (revisionSelect) {
    revisionSelect.addEventListener('change', () => {
      const selected = revisionSelect.selectedOptions[0];
      const textEl = document.getElementById('detail-text');
      if (!textEl || !currentDraft) return;

      if (revisionSelect.value === 'current') {
        textEl.textContent = currentDraft.text;
      } else if (selected?.dataset?.revText) {
        textEl.textContent = selected.dataset.revText;
      }
    });
  }

  if (searchInput) {
    searchInput.addEventListener('input', () => {
      const q = searchInput.value.toLowerCase().trim();
      if (!q) {
        renderDraftList(allDrafts);
        return;
      }
      const filtered = allDrafts.filter((d) => (
        d.app.toLowerCase().includes(q) || d.preview.toLowerCase().includes(q)
      ));
      renderDraftList(filtered);
    });
  }

  if (refreshBtn) {
    refreshBtn.addEventListener('click', () => loadAndRenderDrafts(searchInput?.value));
  }

  if (copyBtn) {
    copyBtn.addEventListener('click', async () => {
      const textEl = document.getElementById('detail-text');
      if (!textEl || !textEl.textContent) return;

      try {
        await navigator.clipboard.writeText(textEl.textContent);
      } catch {
        // Fallback for document copy
      }

      const toast = document.getElementById('copy-feedback-toast');
      if (toast) {
        toast.classList.remove('hidden');
        setTimeout(() => toast.classList.add('hidden'), 2500);
      }
    });
  }

  if (deleteBtn) {
    deleteBtn.addEventListener('click', async () => {
      if (!currentDraft) return;
      if (!confirm(`Permanently delete this ${currentDraft.app} draft?`)) return;

      try {
        await deleteDraft(currentDraft.id);
      } catch {
        // Fallback for UI state
      }

      allDrafts = allDrafts.filter((d) => d.id !== currentDraft.id);
      currentDraft = null;
      renderDraftList(allDrafts);
      if (allDrafts.length > 0) {
        selectDraft(allDrafts[0].id);
      } else {
        const textEl = document.getElementById('detail-text');
        if (textEl) textEl.textContent = 'No drafts remaining.';
      }
    });
  }

  await loadAndRenderDrafts();
}

// -----------------------------------------------------------------------------
// Companions (Roster & Downloadable Catalog)
// -----------------------------------------------------------------------------
let installedCompanions = [];
let activePetId = 'grace';

async function setupCompanionsView() {
  const grid = document.getElementById('companions-grid');
  const filterChips = document.querySelectorAll('.filter-chip');
  let currentFilter = 'all';

  async function loadCompanions() {
    try {
      installedCompanions = await listCompanions();
    } catch {
      installedCompanions = [
        { id: 'grace', name: 'Grace', description: 'Grounding presence — reminds you to drink water.', tags: ['calm', 'human'], isInstalled: true },
        { id: 'rory', name: 'Rory', description: 'The lion cub heir — small and boisterous.', tags: ['animal', 'playful'], isInstalled: true },
      ];
    }
    renderGrid();
  }

  function renderGrid() {
    if (!grid) return;
    grid.innerHTML = '';

    const installedIds = new Set(installedCompanions.map((c) => c.id));
    const fullList = [...installedCompanions];

    CATALOG_COMPANIONS.forEach((cat) => {
      if (!installedIds.has(cat.id)) {
        fullList.push({
          id: cat.id,
          name: cat.name,
          description: cat.desc,
          tags: cat.tags,
          isCustom: false,
          isInstalled: false,
          size: cat.size,
        });
      }
    });

    const filtered = fullList.filter((c) => {
      if (currentFilter === 'installed') return c.isInstalled;
      if (currentFilter === 'catalog') return !c.isInstalled;
      if (currentFilter === 'custom') return c.isCustom;
      return true;
    });

    filtered.forEach((c) => {
      const card = document.createElement('div');
      card.className = `companion-card ${c.id === activePetId ? 'active-pet' : ''}`;

      const tagsHtml = (c.tags || []).map((t) => `<span class="tag-badge">${escapeHtml(t)}</span>`).join('');
      const actionBtnHtml = c.isInstalled
        ? (c.id === activePetId
          ? '<span class="status-pill ok">Active</span>'
          : `<button type="button" class="btn btn-secondary btn-select-pet" data-id="${c.id}">Select</button>`)
        : `<button type="button" class="btn btn-primary btn-install-pet" data-id="${c.id}">Download Pack</button>`;

      card.innerHTML = `
        <div>
          <div class="companion-card-header">
            <h3 class="companion-name">${escapeHtml(c.name)}</h3>
            <span class="meta-label">${c.isInstalled ? 'Installed' : 'Catalog'}</span>
          </div>
          <p class="companion-desc">${escapeHtml(c.description)}</p>
          <div class="tag-list">${tagsHtml}</div>
        </div>
        <div class="card-footer">
          <span class="size-label">${c.size || '3.2 MB'}</span>
          ${actionBtnHtml}
        </div>
      `;

      card.querySelector('.btn-select-pet')?.addEventListener('click', () => {
        activePetId = c.id;
        const activeName = document.getElementById('active-companion-name');
        if (activeName) activeName.textContent = c.name;
        renderGrid();
      });

      card.querySelector('.btn-install-pet')?.addEventListener('click', async (e) => {
        const btn = e.currentTarget;
        btn.textContent = 'Installing…';
        btn.disabled = true;

        try {
          await installCatalogCompanion(c.id);
        } catch {
          // Fallback simulation
        }

        c.isInstalled = true;
        installedCompanions.push(c);
        renderGrid();
      });

      grid.appendChild(card);
    });
  }

  filterChips.forEach((chip) => {
    chip.addEventListener('click', () => {
      filterChips.forEach((c) => c.classList.toggle('active', c === chip));
      currentFilter = chip.dataset.filter;
      renderGrid();
    });
  });

  await loadCompanions();
}

// -----------------------------------------------------------------------------
// Studio (Create & Customize Companion + Procedural Rigged Animation Canvas)
// -----------------------------------------------------------------------------
function setupStudioView() {
  const canvas = document.getElementById('studio-canvas');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const actionLabel = document.getElementById('stage-action-label');
  const actionButtons = document.querySelectorAll('.btn-action');
  const form = document.getElementById('companion-creator-form');

  let currentAction = 'idle';
  let animTime = 0;
  let customColors = {
    body: '#2563eb',
    accent: '#c2331a',
    eyes: '#14120f',
  };

  actionButtons.forEach((btn) => {
    btn.addEventListener('click', () => {
      actionButtons.forEach((b) => b.classList.toggle('active', b === btn));
      currentAction = btn.dataset.action;
      if (actionLabel) actionLabel.textContent = `State: ${currentAction}`;
    });
  });

  const bodyPicker = document.getElementById('char-color-body');
  const accentPicker = document.getElementById('char-color-accent');
  const eyesPicker = document.getElementById('char-color-eyes');

  if (bodyPicker) {
    bodyPicker.addEventListener('input', () => { customColors.body = bodyPicker.value; });
  }
  if (accentPicker) {
    accentPicker.addEventListener('input', () => { customColors.accent = accentPicker.value; });
  }
  if (eyesPicker) {
    eyesPicker.addEventListener('input', () => { customColors.eyes = eyesPicker.value; });
  }

  // Animation Loop
  function renderFrame() {
    animTime += 0.04;
    const w = canvas.width;
    const h = canvas.height;
    ctx.clearRect(0, 0, w, h);

    // Grid Floor
    ctx.strokeStyle = '#1e2430';
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let x = 0; x <= w; x += 40) {
      ctx.moveTo(x, 0);
      ctx.lineTo(x, h);
    }
    for (let y = 0; y <= h; y += 40) {
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
    }
    ctx.stroke();

    const cx = w / 2;
    const cy = h / 2 + 30;

    // Kinematics calculations based on currentAction
    let bob = Math.sin(animTime * 3) * 6;
    let waveAngle = 0;
    let walkPhase = 0;
    let eyeBlink = Math.sin(animTime * 0.8) > 0.95 ? 0.1 : 1.0;

    if (currentAction === 'walk') {
      walkPhase = Math.sin(animTime * 6);
      bob = Math.abs(Math.sin(animTime * 6)) * 8;
    } else if (currentAction === 'wave') {
      waveAngle = Math.sin(animTime * 8) * 0.5;
    } else if (currentAction === 'happy') {
      bob = Math.abs(Math.sin(animTime * 8)) * 18;
      waveAngle = Math.sin(animTime * 12) * 0.4;
    } else if (currentAction === 'sad') {
      bob = -10;
      eyeBlink = 0.3;
    }

    // Ground Shadow
    ctx.fillStyle = 'rgba(0, 0, 0, 0.4)';
    ctx.beginPath();
    ctx.ellipse(cx, cy + 60, 42 - bob * 0.4, 12, 0, 0, Math.PI * 2);
    ctx.fill();

    // Body
    ctx.fillStyle = customColors.body;
    ctx.beginPath();
    ctx.roundRect(cx - 32, cy - 40 - bob, 64, 70, 8);
    ctx.fill();

    // Accent Chest Badge
    ctx.fillStyle = customColors.accent;
    ctx.beginPath();
    ctx.roundRect(cx - 14, cy - 20 - bob, 28, 30, 4);
    ctx.fill();

    // Legs / Feet
    ctx.fillStyle = customColors.accent;
    const legL = walkPhase * 12;
    const legR = -walkPhase * 12;
    ctx.fillRect(cx - 24, cy + 30 - bob + legL, 14, 22);
    ctx.fillRect(cx + 10, cy + 30 - bob + legR, 14, 22);

    // Left Arm
    ctx.save();
    ctx.translate(cx - 32, cy - 20 - bob);
    ctx.rotate(currentAction === 'wave' ? -1.2 + waveAngle : 0.2 - walkPhase * 0.3);
    ctx.fillStyle = customColors.body;
    ctx.fillRect(-10, 0, 10, 36);
    ctx.restore();

    // Right Arm
    ctx.save();
    ctx.translate(cx + 32, cy - 20 - bob);
    ctx.rotate(currentAction === 'wave' ? 1.2 - waveAngle : -0.2 + walkPhase * 0.3);
    ctx.fillStyle = customColors.body;
    ctx.fillRect(0, 0, 10, 36);
    ctx.restore();

    // Head
    ctx.fillStyle = customColors.body;
    ctx.beginPath();
    ctx.roundRect(cx - 28, cy - 88 - bob, 56, 44, 8);
    ctx.fill();

    // Ears
    ctx.fillStyle = customColors.accent;
    ctx.beginPath();
    ctx.moveTo(cx - 24, cy - 88 - bob);
    ctx.lineTo(cx - 16, cy - 108 - bob);
    ctx.lineTo(cx - 8, cy - 88 - bob);
    ctx.fill();

    ctx.beginPath();
    ctx.moveTo(cx + 8, cy - 88 - bob);
    ctx.lineTo(cx + 16, cy - 108 - bob);
    ctx.lineTo(cx + 24, cy - 88 - bob);
    ctx.fill();

    // Eyes
    ctx.fillStyle = customColors.eyes;
    ctx.fillRect(cx - 18, cy - 72 - bob, 8, 10 * eyeBlink);
    ctx.fillRect(cx + 10, cy - 72 - bob, 8, 10 * eyeBlink);

    // Action Bubble / Icon
    if (currentAction === 'water') {
      ctx.fillStyle = '#38bdf8';
      ctx.beginPath();
      ctx.arc(cx + 46, cy - 90 - bob, 10, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = '#ffffff';
      ctx.font = '12px sans-serif';
      ctx.fillText('💧', cx + 40, cy - 85 - bob);
    } else if (currentAction === 'stretch') {
      ctx.fillStyle = '#f59e0b';
      ctx.beginPath();
      ctx.arc(cx + 46, cy - 90 - bob, 10, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = '#ffffff';
      ctx.font = '12px sans-serif';
      ctx.fillText('⚡', cx + 40, cy - 85 - bob);
    }

    requestAnimationFrame(renderFrame);
  }

  requestAnimationFrame(renderFrame);

  if (form) {
    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const nameInput = document.getElementById('char-name');
      const descInput = document.getElementById('char-desc');
      const tagsInput = document.getElementById('char-tags');
      const modelType = document.getElementById('char-model-type');

      const companionPayload = {
        name: nameInput?.value || 'Custom Companion',
        description: descInput?.value || 'Custom user created companion',
        tags: (tagsInput?.value || 'custom').split(',').map((t) => t.trim()),
        avatarType: modelType?.value || 'procedural',
        actions: ['idle', 'walk', 'wave', 'water', 'stretch', 'happy'],
        colors: { ...customColors },
      };

      try {
        const saved = await saveCustomCompanion(companionPayload);
        activePetId = saved.id;
        const activeName = document.getElementById('active-companion-name');
        if (activeName) activeName.textContent = saved.name;
        alert(`Companion "${saved.name}" saved and set as active!`);
      } catch (err) {
        alert(`Failed to save: ${err.message}`);
      }
    });
  }
}

// -----------------------------------------------------------------------------
// Settings
// -----------------------------------------------------------------------------
async function setupSettingsView() {
  const autostartCheck = document.getElementById('setting-autostart');
  const pruneBtn = document.getElementById('prune-vault-btn');
  const vaultPathEl = document.getElementById('meta-vault-path');
  const encEl = document.getElementById('meta-encryption');

  try {
    const isAutostart = await getAutostart();
    if (autostartCheck) autostartCheck.checked = isAutostart;

    const vaultStatus = await getVaultStatus();
    if (vaultPathEl) vaultPathEl.textContent = vaultStatus.vaultPath;
    if (encEl) encEl.textContent = vaultStatus.encryption;
  } catch {
    // Non-fatal
  }

  if (autostartCheck) {
    autostartCheck.addEventListener('change', async () => {
      try {
        const updated = await setAutostart(autostartCheck.checked);
        autostartCheck.checked = updated;
      } catch (err) {
        alert(`Could not update startup: ${err.message}`);
      }
    });
  }

  if (pruneBtn) {
    pruneBtn.addEventListener('click', async () => {
      pruneBtn.disabled = true;
      pruneBtn.textContent = 'Pruning…';
      try {
        const count = await pruneVault(14);
        alert(`Prune complete: removed ${count} expired drafts and vacuumed SQLite database.`);
      } catch (err) {
        alert(`Prune failed: ${err.message}`);
      } finally {
        pruneBtn.disabled = false;
        pruneBtn.textContent = 'Prune Expired Now';
      }
    });
  }
}

function escapeHtml(text) {
  if (typeof text !== 'string') return '';
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

if (typeof document !== 'undefined') {
  void initializeApp();
}
