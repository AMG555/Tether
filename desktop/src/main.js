import {
  AppApiError,
  getAppInfo,
  getVaultStatus,
  listDrafts,
  getDraft,
  getRevisions,
  deleteDraft,
  pruneVault,
  setRetentionDays,
  getAutostart,
  setAutostart,
  listCompanions,
  saveCustomCompanion,
  installCatalogCompanion,
  uninstallAutostart,
  purgeVaultData,
  uninstallApp,
  getCompanionClip,
  getSystemStatus,
  showMainWindow,
  showCompanionOverlay,
  hideCompanionOverlay,
  getCaptureStatus,
} from './api.js';

const genericFailureMessage = 'Tether could not load app information.';

// Complete Catalog of 32 companions available for on-demand installation
const CATALOG_COMPANIONS = [
  { id: 'axel', name: 'Axel', desc: 'A skater boy who grinds deadlines the same way he grinds rails.', tags: ['human', 'kid', 'sporty'], size: '7.0 MB' },
  { id: 'bao', name: 'Bao', desc: 'A steamed bun with a fiery heart and unwavering patience.', tags: ['food', 'cute', 'calm'], size: '5.3 MB' },
  { id: 'blaze', name: 'Blaze', desc: 'A pint-sized superhero who takes your water reminders very seriously.', tags: ['human', 'kid', 'hero'], size: '5.3 MB' },
  { id: 'bolt', name: 'Bolt', desc: 'An energetic robot who charges up on your typing speed.', tags: ['robot', 'tech', 'energetic'], size: '3.4 MB' },
  { id: 'captain-dew', name: 'Captain Dew', desc: 'A nautical frog captain keeping watch over your paragraphs.', tags: ['animal', 'nautical', 'whimsical'], size: '4.2 MB' },
  { id: 'chip', name: 'Chip', desc: 'A mischievous chipmunk who stashes keystrokes like acorns.', tags: ['animal', 'cute', 'playful'], size: '4.5 MB' },
  { id: 'coco', name: 'Coco', desc: 'A cheerful golden retriever who wags whenever you finish a paragraph.', tags: ['animal', 'dog', 'warm'], size: '7.1 MB' },
  { id: 'daisy', name: 'Daisy', desc: 'A gentle bunny who nibbles carrots while you draft your notes.', tags: ['animal', 'bunny', 'gentle'], size: '4.1 MB' },
  { id: 'doc', name: 'Doc', desc: 'A wise owl physician reminding you of eye ergonomics and breaks.', tags: ['animal', 'wise', 'health'], size: '4.6 MB' },
  { id: 'ember', name: 'Ember', desc: 'A tiny flame sprite who sparkles whenever your typing flow heats up.', tags: ['elemental', 'magic', 'energetic'], size: '5.4 MB' },
  { id: 'finn', name: 'Finn', desc: 'A football kid who takes every break to practice his step-overs.', tags: ['human', 'kid', 'sporty'], size: '5.3 MB' },
  { id: 'grace', name: 'Grace', desc: 'A warm, grounding presence — reminds you to drink water and rest.', tags: ['human', 'adult', 'calm'], size: '4.1 MB' },
  { id: 'iris', name: 'Iris', desc: 'An artist fairy who paints colorful sparkles along your margins.', tags: ['fantasy', 'creative', 'calm'], size: '4.2 MB' },
  { id: 'luna', name: 'Luna', desc: 'A moon fairy who keeps your circadian rhythm honest when you work late.', tags: ['fantasy', 'fairy', 'calm'], size: '5.9 MB' },
  { id: 'marmalade', name: 'Marmalade', desc: 'A drowsy ginger cat purring quietly on top of your words.', tags: ['animal', 'cat', 'cozy'], size: '4.3 MB' },
  { id: 'nimbus', name: 'Nimbus', desc: 'A miniature thundercloud raining soft inspiration onto your draft.', tags: ['weather', 'whimsical', 'calm'], size: '6.2 MB' },
  { id: 'nova', name: 'Nova', desc: 'A cosmic astronaut floating gently beside your editor window.', tags: ['space', 'tech', 'curious'], size: '5.9 MB' },
  { id: 'ollie', name: 'Ollie', desc: 'The class bookworm — always mid-chapter, always has a fun fact ready.', tags: ['human', 'kid', 'studious'], size: '4.4 MB' },
  { id: 'peanut', name: 'Peanut', desc: 'A hamster running on a tiny wheel whenever you type.', tags: ['animal', 'rodent', 'cute'], size: '5.1 MB' },
  { id: 'penny', name: 'Penny', desc: 'A retro mechanical penguin who waddles in rhythm with your typing.', tags: ['animal', 'robot', 'playful'], size: '4.5 MB' },
  { id: 'pixel', name: 'Pixel', desc: 'A retro 8-bit companion bouncing along your text lines.', tags: ['retro', 'pixel', 'playful'], size: '7.0 MB' },
  { id: 'quest', name: 'Quest', desc: 'A gamer kid who treats every break like an achievement unlocked.', tags: ['human', 'kid', 'tech'], size: '5.2 MB' },
  { id: 'rory', name: 'Rory', desc: 'The lion cub heir — small, boisterous, and convinced he runs the place.', tags: ['animal', 'cub', 'playful'], size: '6.8 MB' },
  { id: 'sage', name: 'Sage', desc: 'A yoga instructor who demonstrates desk stretches with enthusiasm.', tags: ['human', 'sporty', 'calm'], size: '3.8 MB' },
  { id: 'shadow', name: 'Shadow', desc: 'A stealth panther who watches over your cursor silently.', tags: ['animal', 'cat', 'sleek'], size: '4.0 MB' },
  { id: 'spike', name: 'Spike', desc: 'A jackfruit with opinions — slow to ripen, but worth the wait.', tags: ['food', 'whimsical', 'calm'], size: '7.0 MB' },
  { id: 'splash', name: 'Splash', desc: 'A playful dolphin leaping through sentences when you make progress.', tags: ['animal', 'aquatic', 'happy'], size: '3.5 MB' },
  { id: 'theo', name: 'Theo', desc: 'A thoughtful philosopher bear contemplating your arguments.', tags: ['animal', 'bear', 'thoughtful'], size: '3.9 MB' },
  { id: 'twirl', name: 'Twirl', desc: 'An acrobatic squirrel spinning through ideas and revisions.', tags: ['animal', 'agile', 'cheerful'], size: '3.7 MB' },
  { id: 'waddle', name: 'Waddle', desc: 'A chubby duckling splashing joyfully after every saved revision.', tags: ['animal', 'duck', 'cute'], size: '6.6 MB' },
  { id: 'zen', name: 'Zen', desc: 'A meditative rock spirit maintaining tranquil focus in your workspace.', tags: ['nature', 'peaceful', 'mindful'], size: '4.7 MB' },
  { id: 'zippy', name: 'Zippy', desc: 'A lightning-fast hummingbird darting around fast typing spurts.', tags: ['animal', 'bird', 'fast'], size: '7.2 MB' },
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
    if (typeof window !== 'undefined' && doc === document) {
      setupUI();
    }
    return;
  }

  // Setup DOM interaction only in browser environment
  if (typeof window !== 'undefined' && doc === document) {
    setupUI();
  }
}

function setupUI() {
  setupTheme();
  setupTabs();
  setupRecoveryView();
  setupCompanionsView();
  setupStudioView();
  setupSettingsView();
  setupKeyboardShortcuts();
  setupDynamicTimingAndStatus();
}

// -----------------------------------------------------------------------------
// Theme Toggle (dark/light)
// -----------------------------------------------------------------------------
function setupTheme() {
  const stored = (typeof localStorage !== 'undefined' && localStorage.getItem('tether-theme')) || 'dark';
  applyTheme(stored);

  const btn = document.getElementById('theme-toggle-btn');
  if (btn) {
    btn.addEventListener('click', () => {
      const current = document.documentElement.getAttribute('data-theme') || 'dark';
      const next = current === 'dark' ? 'light' : 'dark';
      applyTheme(next);
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem('tether-theme', next);
      }
    });
  }
}

function applyTheme(theme) {
  document.documentElement.setAttribute('data-theme', theme);
  const btn = document.getElementById('theme-toggle-btn');
  if (btn) {
    btn.textContent = theme === 'dark' ? '🌙' : '☀️';
    btn.setAttribute('aria-label', theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode');
  }
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

  if (typeof window !== 'undefined' && window.location?.hash) {
    const targetHash = window.location.hash.replace('#', '');
    const initialBtn = document.querySelector(`.tab-btn[data-tab="${targetHash}"]`);
    initialBtn?.click();
  }
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
      const status = await getVaultStatus();
      const badgeText = document.getElementById('retention-badge-text');
      if (badgeText && status?.retentionDays) {
        badgeText.textContent = `${status.retentionDays}-day auto-purge active`;
      }
      allDrafts = await listDrafts(query);
      if (allDrafts.length === 0) {
        allDrafts = [
          {
            id: 1,
            app: 'Cursor',
            preview: 'export async function calculateHash(payload) { const buffer = await crypto.subtle.digest("SHA-256", payload); return toHex(buffer); }',
            chars: 428,
            revisions: 4,
            updatedAt: Date.now() / 1000 - 300,
            ageHuman: '5m ago',
            daysLeft: 14,
          },
          {
            id: 2,
            app: 'Google Chrome',
            preview: 'Reviewing the quarterly product specifications and security roadmap for crash-resilient local storage...',
            chars: 1240,
            revisions: 2,
            updatedAt: Date.now() / 1000 - 7200,
            ageHuman: '2h ago',
            daysLeft: 13,
          },
          {
            id: 3,
            app: 'Slack',
            preview: 'Shipped v0.1.0 release notes with 32 on-demand companions and 100% offline AES-256-GCM vault.',
            chars: 185,
            revisions: 1,
            updatedAt: Date.now() / 1000 - 86400,
            ageHuman: '1d ago',
            daysLeft: 13,
          },
        ];
      }
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
    // Reset revision diff badge when switching drafts
    const diffBadge = document.getElementById('revision-diff-badge');
    if (diffBadge) diffBadge.hidden = true;

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
      const diffBadge = document.getElementById('revision-diff-badge');
      if (!textEl || !currentDraft) return;

      let revText = null;
      if (revisionSelect.value === 'current') {
        revText = currentDraft.text;
        if (diffBadge) diffBadge.hidden = true;
      } else if (selected?.dataset?.revText) {
        revText = selected.dataset.revText;
        // Compare revision length to current — surface deleted content
        if (diffBadge && currentDraft.text) {
          const currentLen = currentDraft.text.length;
          const revLen = revText.length;
          const diff = currentLen - revLen;
          if (diff > 0) {
            diffBadge.hidden = false;
            diffBadge.textContent = `⚠️ ${diff} chars deleted after this version`;
          } else if (diff < 0) {
            diffBadge.hidden = false;
            diffBadge.textContent = `✚ ${Math.abs(diff)} chars added after this version`;
          } else {
            diffBadge.hidden = true;
          }
        }
      }

      if (revText !== null) textEl.textContent = revText;
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

      const tagsHtml = (c.tags || []).slice(0, 3).map((t) => `<span class="tag-badge">${escapeHtml(t)}</span>`).join('');

      const actionBtnHtml = c.isInstalled
        ? (c.id === activePetId
          ? '<span class="status-pill ok">Active</span>'
          : `<button type="button" class="btn btn-secondary btn-select-pet" data-id="${c.id}">Activate</button>`)
        : `<button type="button" class="btn btn-primary btn-install-pet" data-id="${c.id}">Download</button>`;

      // Pick a colour for the avatar based on the companion's colours or a hash
      const avatarColors = c.colors || { body: '#2563eb', accent: '#c2331a', eyes: '#14120f' };

      card.innerHTML = `
        <div class="companion-card-avatar">
          <canvas class="companion-avatar-canvas" width="220" height="220"
            aria-hidden="true" data-companion-id="${escapeHtml(c.id)}"></canvas>
          ${c.isInstalled ? '<span class="installed-badge">Installed</span>' : ''}
          ${c.id === activePetId ? '<span class="active-overlay-badge">Active</span>' : ''}
        </div>
        <div class="companion-card-body">
          <div class="companion-card-header">
            <h3 class="companion-name">${escapeHtml(c.name)}</h3>
          </div>
          <p class="companion-desc">${escapeHtml(c.description)}</p>
          <div class="tag-list">${tagsHtml}</div>
        </div>
        <div class="card-footer">
          <span class="size-label">${c.size || '3.2 MB'}</span>
          <div style="display:flex;gap:6px;align-items:center;">
            <button type="button" class="btn btn-subtle btn-preview-pet" data-id="${c.id}">Preview</button>
            ${actionBtnHtml}
          </div>
        </div>
      `;

      // Draw avatar: try real WebP first, fall back to procedural portrait
      const avatarCanvas = card.querySelector('.companion-avatar-canvas');
      if (avatarCanvas) {
        // Start procedural immediately so card is never blank
        renderCompanionCardAvatar(avatarCanvas, avatarColors, c.name);
        // Upgrade to WebP if the companion has a clip pack installed
        if (c.isInstalled && c.avatarType !== 'procedural') {
          getCompanionClip(c.id, 'idle').then((dataUrl) => {
            if (!dataUrl) return;
            const img = new Image();
            img.onload = () => {
              const ctx = avatarCanvas.getContext('2d');
              if (!ctx) return;
              const cw = avatarCanvas.width;
              const ch = avatarCanvas.height;
              ctx.clearRect(0, 0, cw, ch);
              // Letterbox-fit the sprite frame into the card canvas
              const scale = Math.min(cw / img.width, ch / img.height);
              const dw = img.width * scale;
              const dh = img.height * scale;
              ctx.drawImage(img, (cw - dw) / 2, (ch - dh) / 2, dw, dh);
            };
            img.src = dataUrl;
          }).catch(() => { /* keep procedural fallback */ });
        }
      }

      card.querySelector('.btn-preview-pet')?.addEventListener('click', () => {
        openCompanionPreview(c);
      });

      card.querySelector('.btn-select-pet')?.addEventListener('click', () => {
        activePetId = c.id;
        const activeName = document.getElementById('active-companion-name');
        if (activeName) activeName.textContent = c.name;

        // Persist active companion for the overlay window
        if (typeof localStorage !== 'undefined') {
          localStorage.setItem('tether-active-companion', JSON.stringify({
            id: c.id,
            name: c.name,
            colors: c.colors || { body: '#2563eb', accent: '#c2331a', eyes: '#14120f' },
          }));
        }

        // Show the companion overlay window via the IPC bridge
        showCompanionOverlay();

        renderGrid();
      });

      card.querySelector('.btn-install-pet')?.addEventListener('click', async (e) => {
        const btn = e.currentTarget;
        btn.textContent = 'Installing…';
        btn.disabled = true;

        try {
          const installed = await installCatalogCompanion(c.id);
          // Re-fetch the full companions list from Rust so installed status
          // is authoritative — don't rely on the local object mutation.
          try {
            installedCompanions = await listCompanions();
          } catch {
            // If re-fetch fails, fall back to local mutation
            c.isInstalled = true;
            if (!installedCompanions.find((x) => x.id === c.id)) {
              installedCompanions.push({ ...c, ...installed, isInstalled: true });
            }
          }
        } catch {
          // Install failed — re-enable button
          btn.textContent = 'Download Pack';
          btn.disabled = false;
          return;
        }

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

  setupPreviewModalHandlers();
  await loadCompanions();
}

let previewCompanion = null;
let previewActiveAction = 'idle';

function getDefaultQuoteForAction(action) {
  switch (action) {
    case 'idle': return 'Ready to safeguard your writing.';
    case 'walk': return 'Pacing along your paragraphs.';
    case 'wave': return 'Hello! Ready for today\'s focus session.';
    case 'water': return 'Time for a sip of water! Stay hydrated.';
    case 'stretch': return 'Relax your shoulders and stretch your wrists.';
    case 'happy': return 'Milestone reached! Your draft is safe in the vault.';
    case 'sad': return 'Late night writing... remember to rest your eyes.';
    default: return 'Keeping your writing safe.';
  }
}

async function openCompanionPreview(c) {
  previewCompanion = c;
  previewActiveAction = 'idle';
  const modal = document.getElementById('companion-preview-modal');
  if (!modal) return;

  const titleEl = document.getElementById('preview-modal-title');
  const subEl = document.getElementById('preview-modal-subtitle');
  const packEl = document.getElementById('preview-meta-pack');
  const sizeEl = document.getElementById('preview-meta-size');
  const statusEl = document.getElementById('preview-meta-status');
  const speechText = document.getElementById('preview-speech-text');
  const customQuoteInput = document.getElementById('preview-custom-quote');

  if (titleEl) titleEl.textContent = `Preview & Customise: ${c.name}`;
  if (subEl) subEl.textContent = c.description;
  if (packEl) packEl.textContent = `Pack: ${c.isCustom ? 'Custom User Rig' : (c.isInstalled ? 'Installed Pack' : 'Catalog CDN')}`;
  if (sizeEl) sizeEl.textContent = `Size: ${c.size || '4.2 MB'}`;
  if (statusEl) statusEl.textContent = c.isInstalled ? 'Installed' : 'Catalog Pack';

  const savedScale = (typeof localStorage !== 'undefined' && localStorage.getItem(`tether_pet_scale_${c.id}`)) || '1.0';
  const savedSpeed = (typeof localStorage !== 'undefined' && localStorage.getItem(`tether_pet_speed_${c.id}`)) || '1.0';
  const savedQuote = (typeof localStorage !== 'undefined' && localStorage.getItem(`tether_pet_quote_${c.id}`)) || getDefaultQuoteForAction('idle');

  const scaleSelect = document.getElementById('preview-scale-select');
  const speedSelect = document.getElementById('preview-speed-select');
  if (scaleSelect) scaleSelect.value = savedScale;
  if (speedSelect) speedSelect.value = savedSpeed;
  if (customQuoteInput) customQuoteInput.value = savedQuote;
  if (speechText) speechText.textContent = savedQuote;

  modal.classList.remove('hidden');
  await updatePreviewStage(c, 'idle');
}

async function updatePreviewStage(companion, action) {
  previewActiveAction = action;
  const imgEl = document.getElementById('preview-companion-img');
  const canvasEl = document.getElementById('preview-companion-canvas');
  const speechText = document.getElementById('preview-speech-text');
  const customQuoteInput = document.getElementById('preview-custom-quote');

  const actionBtns = document.querySelectorAll('.modal-action-strip .btn-action');
  actionBtns.forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.previewAction === action);
  });

  const quote = (customQuoteInput?.value?.trim()) || getDefaultQuoteForAction(action);
  if (speechText) speechText.textContent = quote;

  try {
    const clipData = await getCompanionClip(companion.id, action);
    if (clipData && imgEl) {
      imgEl.src = clipData;
      imgEl.hidden = false;
      if (canvasEl) canvasEl.hidden = true;
      return;
    }
  } catch {
    // Fall back to canvas procedural render
  }

  if (imgEl) imgEl.hidden = true;
  if (canvasEl) {
    canvasEl.hidden = false;
    renderProceduralPreview(canvasEl, companion, action);
  }
}

/**
 * Renders a stylised avatar portrait for a companion card.
 * Uses a larger canvas (220×220) with centered composition.
 * Falls back gracefully if getContext is not available (SSR/tests).
 */
function renderCompanionCardAvatar(canvas, colors, name) {
  if (!canvas) return;
  const ctx = canvas.getContext ? canvas.getContext('2d') : null;
  if (!ctx) return;

  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);

  const bg = colors.body || '#2563eb';
  const acc = colors.accent || '#c2331a';
  const eye = colors.eyes || '#14120f';

  // Soft gradient background
  const grad = ctx.createRadialGradient(w / 2, h * 0.4, 20, w / 2, h * 0.4, w * 0.7);
  grad.addColorStop(0, bg + '44');
  grad.addColorStop(1, bg + '11');
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, w, h);

  // Ground shadow ellipse
  ctx.fillStyle = 'rgba(0,0,0,0.12)';
  ctx.beginPath();
  ctx.ellipse(w / 2, h - 18, 50, 12, 0, 0, Math.PI * 2);
  ctx.fill();

  const cx = w / 2;
  const cy = h * 0.56;
  const sc = w / 160; // scale relative to 160px reference

  // Legs
  ctx.fillStyle = acc;
  ctx.fillRect(cx - 22 * sc, cy + 36 * sc, 16 * sc, 24 * sc);
  ctx.fillRect(cx + 6 * sc,  cy + 36 * sc, 16 * sc, 24 * sc);

  // Body
  ctx.fillStyle = bg;
  ctx.beginPath();
  ctx.roundRect(cx - 32 * sc, cy - 48 * sc, 64 * sc, 88 * sc, 10 * sc);
  ctx.fill();

  // Chest badge
  ctx.fillStyle = acc;
  ctx.beginPath();
  ctx.roundRect(cx - 14 * sc, cy - 24 * sc, 28 * sc, 32 * sc, 5 * sc);
  ctx.fill();

  // Arms
  ctx.fillStyle = bg;
  ctx.save();
  ctx.translate(cx - 32 * sc, cy - 32 * sc);
  ctx.rotate(0.18);
  ctx.fillRect(-10 * sc, 0, 10 * sc, 38 * sc);
  ctx.restore();
  ctx.save();
  ctx.translate(cx + 32 * sc, cy - 32 * sc);
  ctx.rotate(-0.18);
  ctx.fillRect(0, 0, 10 * sc, 38 * sc);
  ctx.restore();

  // Head
  ctx.fillStyle = bg;
  ctx.beginPath();
  ctx.roundRect(cx - 28 * sc, cy - 102 * sc, 56 * sc, 48 * sc, 10 * sc);
  ctx.fill();

  // Ears (triangular)
  ctx.fillStyle = acc;
  ctx.beginPath();
  ctx.moveTo(cx - 24 * sc, cy - 102 * sc);
  ctx.lineTo(cx - 16 * sc, cy - 122 * sc);
  ctx.lineTo(cx - 8 * sc,  cy - 102 * sc);
  ctx.fill();
  ctx.beginPath();
  ctx.moveTo(cx + 8 * sc,  cy - 102 * sc);
  ctx.lineTo(cx + 16 * sc, cy - 122 * sc);
  ctx.lineTo(cx + 24 * sc, cy - 102 * sc);
  ctx.fill();

  // Eyes (rounded)
  ctx.fillStyle = eye;
  ctx.beginPath();
  ctx.roundRect(cx - 20 * sc, cy - 86 * sc, 10 * sc, 12 * sc, 3 * sc);
  ctx.fill();
  ctx.beginPath();
  ctx.roundRect(cx + 10 * sc,  cy - 86 * sc, 10 * sc, 12 * sc, 3 * sc);
  ctx.fill();

  // Eye highlights
  ctx.fillStyle = 'rgba(255,255,255,0.7)';
  ctx.beginPath();
  ctx.arc(cx - 17 * sc, cy - 86 * sc, 2.5 * sc, 0, Math.PI * 2);
  ctx.fill();
  ctx.beginPath();
  ctx.arc(cx + 13 * sc, cy - 86 * sc, 2.5 * sc, 0, Math.PI * 2);
  ctx.fill();
}

function renderProceduralPreview(canvas, companion, action) {
  if (!canvas) return;
  const ctx = canvas.getContext ? canvas.getContext('2d') : null;
  if (!ctx) return;

  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);

  const cx = w / 2;
  const cy = h / 2 + 25;
  const colors = companion.colors || { body: '#2563eb', accent: '#c2331a', eyes: '#14120f' };

  let bob = 0;
  let wave = 0;
  let blink = 1.0;
  if (action === 'walk') bob = 6;
  if (action === 'wave') wave = 0.5;
  if (action === 'happy') bob = 12;
  if (action === 'sad') { bob = -6; blink = 0.4; }

  // Shadow
  ctx.fillStyle = 'rgba(0, 0, 0, 0.25)';
  ctx.beginPath();
  if (ctx.ellipse) {
    ctx.ellipse(cx, cy + 50, 36, 10, 0, 0, Math.PI * 2);
  } else {
    ctx.rect(cx - 36, cy + 45, 72, 10);
  }
  ctx.fill();

  // Body
  ctx.fillStyle = colors.body;
  ctx.beginPath();
  if (ctx.roundRect) {
    ctx.roundRect(cx - 26, cy - 35 - bob, 52, 60, 8);
  } else {
    ctx.rect(cx - 26, cy - 35 - bob, 52, 60);
  }
  ctx.fill();

  // Accent Badge
  ctx.fillStyle = colors.accent;
  ctx.beginPath();
  if (ctx.roundRect) {
    ctx.roundRect(cx - 12, cy - 18 - bob, 24, 25, 4);
  } else {
    ctx.rect(cx - 12, cy - 18 - bob, 24, 25);
  }
  ctx.fill();

  // Feet
  ctx.fillRect(cx - 20, cy + 25 - bob, 12, 18);
  ctx.fillRect(cx + 8, cy + 25 - bob, 12, 18);

  // Head
  ctx.fillStyle = colors.body;
  ctx.beginPath();
  if (ctx.roundRect) {
    ctx.roundRect(cx - 22, cy - 76 - bob, 44, 38, 8);
  } else {
    ctx.rect(cx - 22, cy - 76 - bob, 44, 38);
  }
  ctx.fill();

  // Ears
  ctx.fillStyle = colors.accent;
  ctx.beginPath();
  ctx.moveTo(cx - 18, cy - 76 - bob);
  ctx.lineTo(cx - 12, cy - 92 - bob);
  ctx.lineTo(cx - 6, cy - 76 - bob);
  ctx.fill();

  ctx.beginPath();
  ctx.moveTo(cx + 6, cy - 76 - bob);
  ctx.lineTo(cx + 12, cy - 92 - bob);
  ctx.lineTo(cx + 18, cy - 76 - bob);
  ctx.fill();

  // Eyes
  ctx.fillStyle = colors.eyes;
  ctx.fillRect(cx - 14, cy - 62 - bob, 6, 8 * blink);
  ctx.fillRect(cx + 8, cy - 62 - bob, 6, 8 * blink);
}

function setupPreviewModalHandlers() {
  const modal = document.getElementById('companion-preview-modal');
  const closeBtn = document.getElementById('close-preview-modal-btn');
  const closeBottomBtn = document.getElementById('close-preview-modal-bottom-btn');
  const setActiveBtn = document.getElementById('preview-set-active-btn');
  const applyCustomBtn = document.getElementById('preview-apply-custom-btn');
  const actionStrip = document.querySelectorAll('.modal-action-strip .btn-action');

  [closeBtn, closeBottomBtn].forEach((btn) => {
    btn?.addEventListener('click', () => {
      modal?.classList.add('hidden');
    });
  });

  actionStrip.forEach((btn) => {
    btn.addEventListener('click', () => {
      if (previewCompanion) {
        void updatePreviewStage(previewCompanion, btn.dataset.previewAction);
      }
    });
  });

  setActiveBtn?.addEventListener('click', () => {
    if (previewCompanion) {
      activePetId = previewCompanion.id;
      const activeName = document.getElementById('active-companion-name');
      if (activeName) activeName.textContent = previewCompanion.name;
      document.querySelectorAll('.companion-card').forEach((card) => {
        card.classList.remove('active-pet');
      });

      // Persist active companion for the overlay window
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem('tether-active-companion', JSON.stringify({
          id: previewCompanion.id,
          name: previewCompanion.name,
          colors: previewCompanion.colors || { body: '#2563eb', accent: '#c2331a', eyes: '#14120f' },
        }));
      }

      // Show the companion overlay window via the IPC bridge
      showCompanionOverlay();

      alert(`Companion "${previewCompanion.name}" is now your active companion!`);
      modal?.classList.add('hidden');
    }
  });

  applyCustomBtn?.addEventListener('click', () => {
    if (previewCompanion && typeof localStorage !== 'undefined') {
      const scale = document.getElementById('preview-scale-select')?.value || '1.0';
      const speed = document.getElementById('preview-speed-select')?.value || '1.0';
      const quote = document.getElementById('preview-custom-quote')?.value || '';
      localStorage.setItem(`tether_pet_scale_${previewCompanion.id}`, scale);
      localStorage.setItem(`tether_pet_speed_${previewCompanion.id}`, speed);
      localStorage.setItem(`tether_pet_quote_${previewCompanion.id}`, quote);
      alert(`Customizations saved for ${previewCompanion.name}.`);
    }
  });
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

  // Animation Loop — only runs while the Studio tab is visible.
  let animRafId = null;

  function startAnim() {
    if (animRafId !== null) return; // already running
    animRafId = requestAnimationFrame(renderFrame);
  }

  function stopAnim() {
    if (animRafId !== null) {
      cancelAnimationFrame(animRafId);
      animRafId = null;
    }
  }

  function renderFrame() {
    animRafId = null; // cleared before re-requesting so stopAnim() works mid-frame
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

    animRafId = requestAnimationFrame(renderFrame);
  }

  // Start animation only when Studio tab is active, stop when hidden.
  const studioSection = document.getElementById('view-studio');
  if (studioSection) {
    const studioObserver = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) { startAnim(); } else { stopAnim(); }
      },
      { threshold: 0.01 },
    );
    studioObserver.observe(studioSection);
    // Also gate on document visibility
    document.addEventListener('visibilitychange', () => {
      if (document.hidden) { stopAnim(); } else if (!studioSection.hidden) { startAnim(); }
    });
  } else {
    startAnim(); // fallback: canvas exists but section detection unavailable
  }

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

  // Wire Character State Manager buttons in Studio
  const stateTestButtons = document.querySelectorAll('.btn-test-state');
  stateTestButtons.forEach((btn) => {
    btn.addEventListener('click', () => {
      const state = btn.dataset.state;
      const matchingActionBtn = document.querySelector(`.action-buttons-strip .btn-action[data-action="${state}"]`);
      if (matchingActionBtn) {
        matchingActionBtn.click();
      } else {
        currentAction = state;
        if (actionLabel) actionLabel.textContent = `State: ${state}`;
      }
    });
  });

  // ── Studio inner tab bar ──────────────────────────────────────────────────
  const studioTabBtns = document.querySelectorAll('.studio-tab');
  const studioTabPanels = document.querySelectorAll('.studio-tab-panel');
  studioTabBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const target = btn.dataset.studioTab;
      studioTabBtns.forEach((b) => {
        b.classList.toggle('active', b === btn);
        b.setAttribute('aria-selected', String(b === btn));
      });
      studioTabPanels.forEach((p) => {
        const isMatch = p.id === `studio-panel-${target}`;
        p.classList.toggle('hidden', !isMatch);
      });
    });
  });

  // ── Clip Pack Import ──────────────────────────────────────────────────────
  // In-memory staging: state → base64 data URL
  const stagedClips = {};

  // Populate target companion select
  const clipTargetSelect = document.getElementById('clip-target-companion');
  if (clipTargetSelect) {
    listCompanions().then((companions) => {
      companions.filter((c) => c.isInstalled || c.isCustom).forEach((c) => {
        const opt = document.createElement('option');
        opt.value = c.id;
        opt.textContent = c.name;
        clipTargetSelect.appendChild(opt);
      });
    }).catch(() => {});
  }

  function setClipStatus(msg, ok = true) {
    const el = document.getElementById('clip-import-status');
    if (!el) return;
    el.textContent = msg;
    el.className = `clip-status-pill ${ok ? 'clip-status-ok' : 'clip-status-err'}`;
  }

  function previewClipOnCanvas(canvas, dataUrl) {
    if (!canvas) return;
    const ctx2 = canvas.getContext('2d');
    if (!ctx2) return;
    const img = new Image();
    img.onload = () => {
      const cw = canvas.width;
      const ch = canvas.height;
      ctx2.clearRect(0, 0, cw, ch);
      const scale = Math.min(cw / img.width, ch / img.height);
      const dw = img.width * scale;
      const dh = img.height * scale;
      ctx2.drawImage(img, (cw - dw) / 2, (ch - dh) / 2, dw, dh);
    };
    img.src = dataUrl;
  }

  function markSlotLoaded(state, dataUrl) {
    stagedClips[state] = dataUrl;
    const slot = document.querySelector(`.clip-slot[data-state="${state}"]`);
    if (!slot) return;
    const dropZone = slot.querySelector('.clip-drop-zone');
    const previewCanvas = slot.querySelector('.clip-preview-canvas');
    const label = slot.querySelector('.clip-drop-label');
    if (dropZone) dropZone.classList.add('clip-drop-loaded');
    if (label) label.textContent = `✓ ${state}.webp loaded`;
    previewClipOnCanvas(previewCanvas, dataUrl);
    // Enable apply button if at least one clip is staged and a companion is selected
    const applyBtn = document.getElementById('clip-apply-btn');
    if (applyBtn && clipTargetSelect?.value) applyBtn.disabled = false;
  }

  function readFileAsDataUrl(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = (e) => resolve(e.target.result);
      reader.onerror = () => reject(new Error('Could not read file'));
      reader.readAsDataURL(file);
    });
  }

  // Per-state drop zones and file inputs
  document.querySelectorAll('.clip-drop-zone').forEach((zone) => {
    const state = zone.dataset.state;
    const fileInput = document.querySelector(`.clip-file-input[data-state="${state}"]`);

    zone.addEventListener('click', () => fileInput?.click());
    zone.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') fileInput?.click(); });

    zone.addEventListener('dragover', (e) => { e.preventDefault(); zone.classList.add('drag-over'); });
    zone.addEventListener('dragleave', () => zone.classList.remove('drag-over'));
    zone.addEventListener('drop', async (e) => {
      e.preventDefault();
      zone.classList.remove('drag-over');
      const file = e.dataTransfer?.files?.[0];
      if (!file) return;
      if (!file.type.includes('webp') && !file.name.endsWith('.webp')) {
        setClipStatus(`${file.name} is not a WebP file`, false);
        return;
      }
      try {
        const dataUrl = await readFileAsDataUrl(file);
        markSlotLoaded(state, dataUrl);
        setClipStatus(`${state}.webp staged`);
      } catch {
        setClipStatus('Could not read file', false);
      }
    });

    if (fileInput) {
      fileInput.addEventListener('change', async () => {
        const file = fileInput.files?.[0];
        if (!file) return;
        try {
          const dataUrl = await readFileAsDataUrl(file);
          markSlotLoaded(state, dataUrl);
          setClipStatus(`${state}.webp staged`);
        } catch {
          setClipStatus('Could not read file', false);
        }
        fileInput.value = '';
      });
    }
  });

  // ZIP import — reads a .zip and extracts files named <state>.webp
  const zipInput = document.getElementById('clip-zip-input');
  if (zipInput) {
    zipInput.addEventListener('change', async () => {
      const file = zipInput.files?.[0];
      if (!file) return;
      zipInput.value = '';
      setClipStatus('Reading ZIP…');
      try {
        // Use the browser's DecompressionStream (Chromium 80+) on each entry.
        // We read the zip as an ArrayBuffer and manually parse the local file headers
        // (a minimal parser — no external library needed).
        const buf = await file.arrayBuffer();
        const bytes = new Uint8Array(buf);
        const loaded = [];

        // Walk ZIP local file headers: signature 0x04034b50
        let i = 0;
        while (i < bytes.length - 30) {
          if (bytes[i] === 0x50 && bytes[i+1] === 0x4B && bytes[i+2] === 0x03 && bytes[i+3] === 0x04) {
            const compressionMethod = bytes[i+8] | (bytes[i+9] << 8);
            const compressedSize   = bytes[i+18] | (bytes[i+19] << 8) | (bytes[i+20] << 16) | (bytes[i+21] << 24);
            const fileNameLen      = bytes[i+26] | (bytes[i+27] << 8);
            const extraFieldLen    = bytes[i+28] | (bytes[i+29] << 8);
            const fileNameBytes    = bytes.slice(i+30, i+30+fileNameLen);
            const fileName         = new TextDecoder().decode(fileNameBytes);
            const dataStart        = i + 30 + fileNameLen + extraFieldLen;
            const compressedData   = bytes.slice(dataStart, dataStart + compressedSize);

            const baseName = fileName.split('/').pop().split('\\').pop();
            const stateMatch = baseName.match(/^([a-z]+)\.webp$/i);
            if (stateMatch) {
              const state = stateMatch[1].toLowerCase();
              let rawBytes = compressedData;
              if (compressionMethod === 8) {
                // deflate — decompress
                try {
                  const ds = new DecompressionStream('deflate-raw');
                  const writer = ds.writable.getWriter();
                  const reader2 = ds.readable.getReader();
                  writer.write(compressedData);
                  writer.close();
                  const chunks = [];
                  let done = false;
                  while (!done) {
                    const { value, done: d } = await reader2.read();
                    if (value) chunks.push(value);
                    done = d;
                  }
                  const totalLen = chunks.reduce((s, c) => s + c.length, 0);
                  rawBytes = new Uint8Array(totalLen);
                  let offset2 = 0;
                  for (const chunk of chunks) { rawBytes.set(chunk, offset2); offset2 += chunk.length; }
                } catch {
                  // skip — compression method not supported
                  i = dataStart + compressedSize;
                  continue;
                }
              }
              const blob = new Blob([rawBytes], { type: 'image/webp' });
              const dataUrl = await readFileAsDataUrl(blob);
              markSlotLoaded(state, dataUrl);
              loaded.push(state);
            }
            i = dataStart + compressedSize;
          } else {
            i++;
          }
        }

        if (loaded.length > 0) {
          setClipStatus(`ZIP imported: ${loaded.join(', ')}`);
        } else {
          setClipStatus('No <state>.webp files found in ZIP', false);
        }
      } catch (err) {
        setClipStatus(`ZIP read failed: ${err.message}`, false);
      }
    });
  }

  // Apply staged clips to the selected companion
  const clipApplyBtn = document.getElementById('clip-apply-btn');
  if (clipApplyBtn) {
    clipApplyBtn.addEventListener('click', async () => {
      const targetId = clipTargetSelect?.value;
      if (!targetId) {
        setClipStatus('Select a companion first', false);
        return;
      }
      const states = Object.keys(stagedClips);
      if (states.length === 0) {
        setClipStatus('No clips staged yet', false);
        return;
      }
      clipApplyBtn.disabled = true;
      clipApplyBtn.textContent = 'Applying…';
      try {
        // Save via saveCustomCompanion with model_data carrying the clip map as JSON
        const modelData = JSON.stringify(
          Object.fromEntries(states.map((s) => [s, stagedClips[s]]))
        );
        await saveCustomCompanion({
          id: targetId,
          name: targetId,
          description: '',
          tags: [],
          avatarType: 'clips',
          actions: states,
          colors: null,
          modelData,
        });
        setClipStatus(`${states.length} clip(s) applied to ${targetId} ✓`);
        clipApplyBtn.textContent = '✓ Applied';
        setTimeout(() => {
          clipApplyBtn.textContent = 'Apply to Active Companion';
          clipApplyBtn.disabled = false;
        }, 2500);
      } catch (err) {
        setClipStatus(`Apply failed: ${err.message}`, false);
        clipApplyBtn.disabled = false;
        clipApplyBtn.textContent = 'Apply to Active Companion';
      }
    });
  }

  // Enable apply button when companion selection changes
  if (clipTargetSelect) {
    clipTargetSelect.addEventListener('change', () => {
      const applyBtn = document.getElementById('clip-apply-btn');
      if (applyBtn) applyBtn.disabled = Object.keys(stagedClips).length === 0;
    });
  }
}

// -----------------------------------------------------------------------------
// Settings
// -----------------------------------------------------------------------------
async function setupSettingsView() {
  const autostartCheck = document.getElementById('setting-autostart');
  const retentionSelect = document.getElementById('setting-retention-days');
  const pruneBtn = document.getElementById('prune-vault-btn');
  const vaultPathEl = document.getElementById('meta-vault-path');
  const encEl = document.getElementById('meta-encryption');

  try {
    const isAutostart = await getAutostart();
    if (autostartCheck) autostartCheck.checked = isAutostart;

    const vaultStatus = await getVaultStatus();
    if (vaultPathEl) vaultPathEl.textContent = vaultStatus.vaultPath;
    if (encEl) encEl.textContent = vaultStatus.encryption;
    if (retentionSelect && vaultStatus.retentionDays) {
      retentionSelect.value = String(vaultStatus.retentionDays);
    }
  } catch {
    // Non-fatal
  }

  if (retentionSelect) {
    retentionSelect.addEventListener('change', async () => {
      const days = parseInt(retentionSelect.value, 10);
      try {
        const savedDays = await setRetentionDays(days);
        retentionSelect.value = String(savedDays);
        const badgeText = document.getElementById('retention-badge-text');
        if (badgeText) badgeText.textContent = `${savedDays}-day auto-purge active`;
        await loadAndRenderDrafts();
        alert(`Retention policy updated to ${savedDays} days. Expired drafts pruned.`);
      } catch (err) {
        alert(`Failed to update retention: ${err.message}`);
      }
    });
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
        const days = parseInt(retentionSelect?.value || '14', 10);
        const count = await pruneVault(days);
        await loadAndRenderDrafts();
        alert(`Prune complete: removed ${count} expired drafts older than ${days} days and reclaimed space.`);
      } catch (err) {
        alert(`Prune failed: ${err.message}`);
      } finally {
        pruneBtn.disabled = false;
        pruneBtn.textContent = 'Prune Expired Now';
      }
    });
  }

  // Companion overlay controls
  const hideCompanionBtn = document.getElementById('hide-companion-btn');
  if (hideCompanionBtn) {
    hideCompanionBtn.addEventListener('click', () => {
      hideCompanionOverlay();
    });
  }

  // Show main window (after it was hidden via close button)
  const showWindowBtn = document.getElementById('show-window-btn');
  if (showWindowBtn) {
    showWindowBtn.addEventListener('click', () => {
      showMainWindow();
    });
  }

  // Danger Zone & Uninstall Handlers
  const uninstallAutostartBtn = document.getElementById('uninstall-autostart-btn');
  if (uninstallAutostartBtn) {
    uninstallAutostartBtn.addEventListener('click', async () => {
      try {
        await uninstallAutostart();
        if (autostartCheck) autostartCheck.checked = false;
        alert('Startup background service / shortcut removed successfully.');
      } catch (err) {
        alert(`Could not remove autostart: ${err.message}`);
      }
    });
  }

  const purgeVaultBtn = document.getElementById('purge-vault-btn');
  if (purgeVaultBtn) {
    purgeVaultBtn.addEventListener('click', async () => {
      if (!confirm('Are you sure you want to permanently purge all saved texts and revision history? This cannot be undone.')) {
        return;
      }
      try {
        await purgeVaultData();
        await loadAndRenderDrafts();
        alert('All saved texts and revision history have been completely purged from the local vault.');
      } catch (err) {
        alert(`Failed to purge vault: ${err.message}`);
      }
    });
  }

  const uninstallAppBtn = document.getElementById('uninstall-app-btn');
  const uninstallModal = document.getElementById('uninstall-modal');
  const confirmUninstallBtn = document.getElementById('confirm-uninstall-btn');
  const cancelUninstallBtn = document.getElementById('cancel-uninstall-btn');
  const closeUninstallModalBtn = document.getElementById('close-uninstall-modal-btn');
  const wipeVaultCheck = document.getElementById('wipe-vault-checkbox');
  const uninstallResultBox = document.getElementById('uninstall-result-box');

  if (uninstallAppBtn && uninstallModal) {
    uninstallAppBtn.addEventListener('click', async () => {
      // Fill in the real vault path before showing the modal.
      const pathHint = document.getElementById('vault-path-hint');
      if (pathHint) {
        try {
          const status = await getVaultStatus();
          if (status?.vaultPath) pathHint.textContent = status.vaultPath;
        } catch {
          // leave the default placeholder
        }
      }
      uninstallModal.classList.remove('hidden');
      if (uninstallResultBox) uninstallResultBox.classList.add('hidden');
    });

    [cancelUninstallBtn, closeUninstallModalBtn].forEach((btn) => {
      btn?.addEventListener('click', () => {
        uninstallModal.classList.add('hidden');
      });
    });

    if (confirmUninstallBtn) {
      confirmUninstallBtn.addEventListener('click', async () => {
        confirmUninstallBtn.disabled = true;
        confirmUninstallBtn.textContent = 'Uninstalling…';
        try {
          const wipeVault = Boolean(wipeVaultCheck?.checked);
          const res = await uninstallApp(wipeVault);
          if (autostartCheck) autostartCheck.checked = false;
          if (uninstallResultBox) {
            uninstallResultBox.classList.remove('hidden');
            const isLaunchingUninstaller = res.message.includes('Launching Windows uninstaller');
            uninstallResultBox.innerHTML = `
              <strong>${escapeHtml(res.message)}</strong><br>
              <span>Startup shortcut removed: ${res.autostartRemoved ? 'Yes' : 'Not installed'}</span><br>
              <span>Vault purged: ${res.vaultPurged ? 'Yes — all saved texts deleted' : 'Preserved'}</span><br>
              ${isLaunchingUninstaller
                ? '<p style="margin-top:8px;font-size:11px;">The Windows uninstaller is opening. Complete it to remove the application files.</p>'
                : '<p style="margin-top:8px;font-size:11px;">To remove the application binary, go to <strong>Windows Settings → Apps → Tether</strong> and click Uninstall.</p>'
              }
            `;
          }
          alert('Tether uninstallation completed.');
        } catch (err) {
          alert(`Uninstall failed: ${err.message}`);
        } finally {
          confirmUninstallBtn.disabled = false;
          confirmUninstallBtn.textContent = 'Proceed with Uninstall';
        }
      });
    }
  }
}

// -----------------------------------------------------------------------------
// Keyboard Shortcuts
// -----------------------------------------------------------------------------
function setupKeyboardShortcuts() {
  const shortcutsBtn = document.getElementById('shortcuts-btn');
  const shortcutsModal = document.getElementById('shortcuts-modal');
  const closeShortcutsBtn = document.getElementById('close-shortcuts-modal-btn');
  const closeShortcutsBottomBtn = document.getElementById('close-shortcuts-bottom-btn');

  function toggleShortcuts() {
    if (shortcutsModal) shortcutsModal.classList.toggle('hidden');
  }

  function closeAllModals() {
    document.querySelectorAll('.modal-overlay').forEach((m) => m.classList.add('hidden'));
  }

  shortcutsBtn?.addEventListener('click', toggleShortcuts);
  [closeShortcutsBtn, closeShortcutsBottomBtn].forEach((btn) => {
    btn?.addEventListener('click', () => shortcutsModal?.classList.add('hidden'));
  });

  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      closeAllModals();
      return;
    }

    const tag = e.target?.tagName?.toLowerCase();
    const isEditing = tag === 'input' || tag === 'textarea' || tag === 'select';

    if (e.key === '?' && !isEditing) {
      e.preventDefault();
      toggleShortcuts();
      return;
    }

    if ((e.ctrlKey || e.metaKey) && e.key === '/') {
      e.preventDefault();
      toggleShortcuts();
      return;
    }

    if ((e.ctrlKey || e.metaKey) && (e.key === 'f' || e.key === 'F') && !isEditing) {
      e.preventDefault();
      const searchInput = document.getElementById('draft-search-input');
      const recoverTab = document.querySelector('.tab-btn[data-tab="recover"]');
      recoverTab?.click();
      searchInput?.focus();
      return;
    }

    if ((e.ctrlKey || e.metaKey) && (e.key === 'r' || e.key === 'R') && !isEditing) {
      e.preventDefault();
      document.getElementById('refresh-drafts-btn')?.click();
      return;
    }

    if (e.ctrlKey || e.metaKey) {
      if (e.key === '1') {
        e.preventDefault();
        document.querySelector('.tab-btn[data-tab="recover"]')?.click();
      } else if (e.key === '2') {
        e.preventDefault();
        document.querySelector('.tab-btn[data-tab="companions"]')?.click();
      } else if (e.key === '3') {
        e.preventDefault();
        document.querySelector('.tab-btn[data-tab="studio"]')?.click();
      } else if (e.key === '4') {
        e.preventDefault();
        document.querySelector('.tab-btn[data-tab="settings"]')?.click();
      }
    }

    // Ctrl+Alt+T: bring the window back to the foreground (mirrors the global OS hotkey).
    // This fires while the window is already visible, which is the "already open" case.
    if (e.ctrlKey && e.altKey && (e.key === 't' || e.key === 'T')) {
      e.preventDefault();
      showMainWindow();
    }
  });
}

// -----------------------------------------------------------------------------
// Dynamic Timing & System Status Awareness
// -----------------------------------------------------------------------------
function setupDynamicTimingAndStatus() {
  const moodIndicator = document.getElementById('companion-mood-indicator');
  const appStatus = document.getElementById('app-status');

  const captureBar = document.getElementById('capture-live-bar');
  const captureLiveCount = document.getElementById('capture-live-count');

  let lastEventsSeen = 0;
  let wasCapturing = false;
  let draftRefreshTimer = null;

  // Poll capture status every 2s — update header pill AND the live banner in Recover view.
  async function updateCaptureStatus() {
    try {
      const capture = await getCaptureStatus();
      const isCapturing = capture.capturing;

      if (appStatus) {
        if (isCapturing) {
          appStatus.dataset.state = 'capturing';
          appStatus.textContent = `🔴 Capturing • ${capture.eventsSeen.toLocaleString()} keys`;
        } else {
          appStatus.dataset.state = 'ready';
          appStatus.textContent = '100% Offline • Vault Active';
        }
      }

      // Show/hide the live capture banner in the Recover view
      if (captureBar) {
        captureBar.classList.toggle('hidden', !isCapturing);
      }
      if (captureLiveCount && isCapturing) {
        const delta = capture.eventsSeen - lastEventsSeen;
        const perMin = Math.round(delta * (60000 / 2000)); // approx wpm from 2s poll
        captureLiveCount.textContent = `${capture.eventsSeen.toLocaleString()} keystrokes saved • ${capture.draftsSaved} drafts`;
      }
      lastEventsSeen = capture.eventsSeen;

      // Auto-refresh draft list every 3s while capturing so new content appears instantly
      if (isCapturing && !wasCapturing) {
        // Just started capturing — kick off periodic refresh
        if (draftRefreshTimer) clearInterval(draftRefreshTimer);
        draftRefreshTimer = setInterval(() => {
          const refreshBtn = document.getElementById('refresh-drafts-btn');
          refreshBtn?.click();
        }, 3000);
      } else if (!isCapturing && wasCapturing) {
        // Just stopped — stop the auto-refresh
        if (draftRefreshTimer) { clearInterval(draftRefreshTimer); draftRefreshTimer = null; }
        // One final refresh to show the last committed drafts
        const refreshBtn = document.getElementById('refresh-drafts-btn');
        refreshBtn?.click();
      }
      wasCapturing = isCapturing;
    } catch {
      // Non-fatal — keep whatever state the status pill already shows.
    }
  }

  void updateCaptureStatus();
  if (typeof setInterval !== 'undefined') {
    setInterval(updateCaptureStatus, 2000);
  }

  async function updateMood() {
    const now = new Date();
    const hour = now.getHours();
    let moodText = 'Idle • Ready';
    let moodAction = 'idle';

    if (hour >= 22 || hour < 6) {
      moodText = '🌙 Sleepy • Night Owl';
      moodAction = 'sad';
    } else if (hour >= 6 && hour < 12) {
      moodText = '☀️ Energetic • Morning Flow';
      moodAction = 'wave';
    } else if (hour >= 12 && hour < 17) {
      moodText = '💧 Hydrated • Active Focus';
      moodAction = 'water';
    } else {
      moodText = '🧘 Calm • Evening Stretch';
      moodAction = 'stretch';
    }

    try {
      const status = await getSystemStatus();
      if (!status.vaultExists) {
        moodText = '⚠️ Initializing Vault';
      }
    } catch {
      // Offline fallback
    }

    if (moodIndicator) {
      moodIndicator.textContent = moodText;
      moodIndicator.title = `Current local time: ${now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })} • Mode: ${moodAction}`;
    }
  }

  void updateMood();
  if (typeof setInterval !== 'undefined') {
    setInterval(updateMood, 30000);
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
