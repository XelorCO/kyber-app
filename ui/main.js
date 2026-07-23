import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import { check as checkUpdate } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

// ══════════════════════════════════════════════════
//  STATE
// ══════════════════════════════════════════════════
let entries     = [];
let isUnlocked  = false;
let vaultPath   = '';
let importFormat = 'bitwarden';
let editingId    = null; // null = new entry

// ══════════════════════════════════════════════════
//  UTILS
// ══════════════════════════════════════════════════
const $  = id => document.getElementById(id);
const on = (id, ev, fn) => $(id).addEventListener(ev, fn);

// ══════════════════════════════════════════════════
//  LICENCE SYSTEM (freemium)
// ══════════════════════════════════════════════════
let isLicensed = false;

async function checkLicenseStatus() {
  try {
    const payload = await invoke('check_license');
    isLicensed = true;
    updateLicenseUI(payload);
  } catch (_) {
    isLicensed = false;
    updateLicenseUI(null);
  }
}

function updateLicenseUI(payload) {
  if (payload) {
    $('sett-license-free')?.classList.add('hidden');
    $('sett-license-pro')?.classList.remove('hidden');
    if ($('sett-license-name')) {
      $('sett-license-name').textContent = `${payload.name} <${payload.email}> — ${payload.tier}`;
    }
  } else {
    $('sett-license-free')?.classList.remove('hidden');
    $('sett-license-pro')?.classList.add('hidden');
  }
}

async function activateLicenseKey(key, errElId) {
  if (!key) return false;
  $(errElId).textContent = '';
  try {
    const payload = await invoke('activate_license', { licenseKey: key });
    isLicensed = true;
    updateLicenseUI(payload);
    showToast(`✓ Licence activée ! Bienvenue ${payload.name}`);
    return true;
  } catch(e) {
    $(errElId).textContent = typeof e === 'string' ? e : 'Clé invalide ou format incorrect.';
    return false;
  }
}

// ── Modale Upgrade ─────────────────────────────────────────────────────
// `reason` adapte le message : 'entries' (limite de mots de passe, défaut)
// ou 'vaults' (limite de coffres) — les deux limites déclenchent la même
// modale mais ne doivent pas afficher le même texte.
function showUpgradeModal(reason = 'entries') {
  $('upgrade-desc').innerHTML = reason === 'vaults'
    ? 'La version gratuite est limitée à <strong>1 coffre</strong>.<br>Passez à Kyber Premium pour créer autant de coffres que vous voulez.'
    : 'La version gratuite est limitée à <strong>10 mots de passe</strong>.<br>Passez à Kyber Premium pour en stocker un nombre illimité.';
  $('upgrade-license-key').value = '';
  $('upgrade-err').textContent = '';
  $('upgrade-modal').classList.remove('hidden');
}

on('upgrade-cancel', 'click', () => $('upgrade-modal').classList.add('hidden'));
on('upgrade-go-btn', 'click', () => invoke('open_upgrade_url'));
on('upgrade-activate-btn', 'click', async () => {
  const key = $('upgrade-license-key').value.trim();
  const ok = await activateLicenseKey(key, 'upgrade-err');
  if (ok) $('upgrade-modal').classList.add('hidden');
});

// ── Paramètres — Licence ──────────────────────────────────────────────────
on('sett-upgrade-btn', 'click', () => invoke('open_upgrade_url'));
on('sett-activate-btn', 'click', async () => {
  const key = $('sett-license-key').value.trim();
  await activateLicenseKey(key, 'sett-license-err');
});

// ══════════════════════════════════════════════════
//  MISES À JOUR (tauri-plugin-updater, signées)
// ══════════════════════════════════════════════════
let pendingUpdate = null;
let updateInProgress = false;

async function checkForUpdates(manual = false) {
  const status = $('update-status');
  if (manual) status.textContent = 'Recherche de mise à jour…';
  try {
    const update = await checkUpdate();
    if (update) {
      pendingUpdate = update;
      status.textContent = `Nouvelle version ${update.version} disponible !` +
        (update.body ? ` / ${update.body}` : '');
      $('update-badge').classList.remove('hidden');
      $('update-install-btn').classList.remove('hidden');
      if (!manual) showUpdateModal(update);
    } else if (manual) {
      status.textContent = '✓ Kyber est à jour.';
    }
  } catch (_) {
    // Hors ligne ou serveur injoignable — silencieux au démarrage, explicite en manuel
    if (manual) status.textContent = 'Vérification impossible (connexion internet requise).';
  }
}

// ── Popup de mise à jour (au démarrage) ──
function showUpdateModal(update) {
  $('um-version').textContent = update.version;
  const notes = $('um-notes');
  if (update.body) { notes.textContent = update.body; notes.classList.remove('hidden'); }
  else notes.classList.add('hidden');
  $('um-progress').classList.add('hidden');
  $('um-install').disabled = false;
  $('update-modal').classList.remove('hidden');
}

function closeUpdateModal() {
  if (updateInProgress) return; // pas de fermeture pendant le téléchargement
  $('update-modal').classList.add('hidden');
}

// Téléchargement + installation, avec progression écrite dans un ou plusieurs éléments
async function installUpdate(progressEls) {
  if (!pendingUpdate || updateInProgress) return;
  updateInProgress = true;
  const setP = txt => progressEls.forEach(el => {
    el.textContent = txt;
    el.classList.remove('hidden');
  });
  try {
    let total = 0, received = 0;
    await pendingUpdate.downloadAndInstall((ev) => {
      if (ev.event === 'Started') { total = ev.data.contentLength || 0; }
      else if (ev.event === 'Progress') {
        received += ev.data.chunkLength;
        if (total) setP(`Téléchargement… ${Math.round(received / total * 100)}%`);
      }
      else if (ev.event === 'Finished') { setP('Installation…'); }
    });
    await relaunch();
  } catch (e) {
    updateInProgress = false;
    setP('Échec de la mise à jour : ' + (typeof e === 'string' ? e : 'réessayez plus tard.'));
    $('um-install').disabled = false;
    $('update-install-btn').disabled = false;
  }
}

on('update-check-btn', 'click', () => checkForUpdates(true));

on('update-install-btn', 'click', () => {
  $('update-install-btn').disabled = true;
  installUpdate([$('update-status')]);
});

on('um-later', 'click', closeUpdateModal);
on('um-install', 'click', () => {
  $('um-install').disabled = true;
  installUpdate([$('um-progress'), $('update-status')]);
});

// Vérification silencieuse au démarrage (3s après lancement, non bloquant)
setTimeout(() => checkForUpdates(false), 3000);


function passwordStrength(pwd) {
  let s = 0;
  if (pwd.length >= 8)  s += 10;
  if (pwd.length >= 12) s += 15;
  if (pwd.length >= 16) s += 10;
  if (pwd.length >= 24) s += 5;
  if (/[A-Z]/.test(pwd)) s += 15;
  if (/[a-z]/.test(pwd)) s += 10;
  if (/[0-9]/.test(pwd)) s += 15;
  if (/[^A-Za-z0-9]/.test(pwd)) s += 20;
  return Math.min(s, 100);
}

function renderStrength(score, fillId, lblId) {
  const fill = $(fillId), lbl = $(lblId);
  fill.style.width = score + '%';
  if (score < 30) { fill.style.background = '#EF4444'; lbl.textContent = 'Très faible'; lbl.style.color = '#EF4444'; }
  else if (score < 50) { fill.style.background = '#F97316'; lbl.textContent = 'Faible'; lbl.style.color = '#F97316'; }
  else if (score < 70) { fill.style.background = '#F59E0B'; lbl.textContent = 'Moyen'; lbl.style.color = '#F59E0B'; }
  else if (score < 90) { fill.style.background = '#10B981'; lbl.textContent = 'Fort'; lbl.style.color = '#10B981'; }
  else { fill.style.background = '#6366F1'; lbl.textContent = 'Très fort'; lbl.style.color = '#6366F1'; }
}

function domainFromUrl(url) {
  try { return new URL(url.startsWith('http') ? url : 'https://' + url).hostname.replace('www.',''); }
  catch { return url; }
}

// Icône lettre 100% locale — aucun appel réseau (le service Google fuitait les domaines)
function letterIcon(url) {
  const domain = domainFromUrl(url) || '?';
  const letter = domain[0].toUpperCase();
  let hash = 0;
  for (let i = 0; i < domain.length; i++) hash = domain.charCodeAt(i) + ((hash << 5) - hash);
  const palette = ['#6366F1','#8B5CF6','#EC4899','#EF4444','#F97316','#10B981','#06B6D4','#3B82F6'];
  const bg = palette[Math.abs(hash) % palette.length];
  return { letter, bg };
}

function escapeHtml(str) {
  return (str || '').replace(/&/g,'&amp;').replace(/"/g,'&quot;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

let toastTimer;
function showToast(msg, duration = 3000) {
  $('toast-msg').textContent = msg;
  $('toast').classList.remove('hidden');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => $('toast').classList.add('hidden'), duration);
}

async function copySecure(text, label = 'Copié !') {
  try {
    await invoke('copy_secure', { text });
    showToast(`⧉ ${label} / Effacé dans 30s`, 3000);
  } catch {
    await navigator.clipboard.writeText(text);
    showToast(`⧉ ${label}`);
  }
}

// ══════════════════════════════════════════════════
//  NAVIGATION
// ══════════════════════════════════════════════════
function switchView(viewId) {
  document.querySelectorAll('.view').forEach(v => v.classList.remove('active'));
  document.querySelectorAll('.nav-item').forEach(n => n.classList.remove('active'));
  $(viewId).classList.add('active');
  document.querySelector(`[data-view="${viewId}"]`)?.classList.add('active');
}

document.querySelectorAll('.nav-item').forEach(item => {
  item.addEventListener('click', e => {
    e.preventDefault();
    const v = item.dataset.view;
    if (!v) return; // items d'action (ex: Verrouiller) gérés séparément
    switchView(v);
    if (v === 'view-health') loadHealth();
  });
});

// ── Verrouillage manuel ────────────────────────────────────────────────
// Purge le coffre déchiffré et la clé maître côté Rust (et coupe la session
// servie à l'extension navigateur), puis revient à l'écran de connexion.
on('nav-lock', 'click', async e => {
  e.preventDefault();
  try { await invoke('lock_vault'); } catch (_) {}
  entries = [];
  isUnlocked = false;
  editingId = null;
  $('master-pass').value = '';
  $('login-err').textContent = '';
  switchView('view-vault');
  $('screen-app').classList.remove('active');
  $('screen-login').classList.add('active');
  setLoginMode('open');
  $('master-pass').focus();
});

// ══════════════════════════════════════════════════
//  LOGIN
// ══════════════════════════════════════════════════
let loginMode = 'open'; // 'open' | 'create'

// Auto-charge le dernier chemin au démarrage (ou le chemin par défaut)
invoke('get_last_vault_path').then(p => {
  if (p) {
    $('vault-path').value = p;
  } else {
    invoke('get_default_vault_path').then(d => { $('vault-path').value = d; }).catch(() => {});
  }
}).catch(() => {
  invoke('get_default_vault_path').then(d => { $('vault-path').value = d; }).catch(() => {});
});

// Bouton Parcourir
on('browse-btn', 'click', async () => {
  if (loginMode === 'create') {
    const path = await saveDialog({
      title: 'Créer un nouveau coffre',
      filters: [{ name: 'Coffre', extensions: ['vault', 'enc', '*'] }],
      defaultPath: 'coffre.vault',
    }).catch(() => null);
    if (path) $('vault-path').value = path;
  } else {
    const path = await openDialog({
      title: 'Ouvrir un coffre',
      multiple: false,
      filters: [{ name: 'Coffre', extensions: ['vault', 'enc', '*'] }],
    }).catch(() => null);
    if (path) $('vault-path').value = typeof path === 'string' ? path : path?.path ?? '';
  }
});

on('ltab-open', 'click', () => setLoginMode('open'));
on('ltab-create', 'click', () => setLoginMode('create'));


function setLoginMode(mode) {
  loginMode = mode;
  $('ltab-open').classList.toggle('active', mode === 'open');
  $('ltab-create').classList.toggle('active', mode === 'create');
  $('create-warning').classList.toggle('hidden', mode === 'open');
  $('login-btn').textContent = mode === 'open' ? 'Déverrouiller' : 'Créer le coffre';
  $('login-err').textContent = '';
}

on('toggle-pass', 'click', () => {
  const inp = $('master-pass');
  inp.type = inp.type === 'password' ? 'text' : 'password';
});

on('login-btn', 'click', async () => {
  const password = $('master-pass').value;
  const path = $('vault-path').value;
  $('login-err').textContent = '';
  if (!password) { $('login-err').textContent = 'Entrez votre mot de passe maître.'; return; }

  try {
    const cmd = loginMode === 'open' ? 'unlock_vault' : 'init_vault';
    entries = await invoke(cmd, { password, path });
    vaultPath = path;
    isUnlocked = true;
    $('master-pass').value = '';
    $('sett-path').textContent = path;
    $('screen-login').classList.remove('active');
    $('screen-app').classList.add('active');
    renderVault();
    updateHealthBadge();
    checkLicenseStatus();
    checkVaultVersion(); // Propose migration si coffre v1
  } catch(e) {
    if (e === 'NOT_FOUND') setLoginMode('create');
    else if (e === 'VAULT_EXISTS') {
      setLoginMode('open');
      $('login-err').textContent = 'Un coffre existe déjà à cet emplacement. Utilisez "Ouvrir le coffre".';
    }
    else if (e === 'VAULT_LIMIT_REACHED') {
      $('login-err').textContent = '';
      showUpgradeModal('vaults');
    }
    else $('login-err').textContent = e;
  }
});

$('master-pass').addEventListener('keydown', e => { if (e.key === 'Enter') $('login-btn').click(); });

// ══════════════════════════════════════════════════
//  VAULT — Render
// ══════════════════════════════════════════════════
// Construction du HTML d'une carte (string pure — assemblée en un seul innerHTML)
function entryCardHtml(entry) {
  const icon = entry.url ? letterIcon(entry.url) : { letter: '◆', bg: '#6366F1' };
  const id = escapeHtml(entry.id);
  return `<div class="entry-card">
      <div class="ec-head">
        <div class="ec-favicon">
          <span class="ec-favicon-letter" style="background:${escapeHtml(icon.bg)}">${escapeHtml(icon.letter)}</span>
        </div>
        <div class="ec-info">
          <div class="ec-title" title="${escapeHtml(entry.title)}">${escapeHtml(entry.title)}</div>
          <div class="ec-user" title="${escapeHtml(entry.username)}">${escapeHtml(entry.username) || '—'}</div>
        </div>
      </div>
      <div class="ec-pass-row">
        <span class="ec-pass" id="ep-${id}">••••••••••••</span>
        <button class="icon-btn" title="Afficher" data-action="reveal" data-id="${id}">◉</button>
      </div>
      <div class="ec-url" title="${escapeHtml(entry.url)}">${escapeHtml(domainFromUrl(entry.url)) || '—'}</div>
      <div class="ec-actions">
        <button class="ec-btn" data-action="copy-user" data-id="${id}">Copier ID</button>
        <button class="ec-btn" data-action="copy-pass" data-id="${id}">Copier MDP</button>
        <button class="ec-btn" data-action="edit" data-id="${id}">Modifier</button>
        <button class="ec-btn danger" data-action="delete" data-id="${id}" data-title="${escapeHtml(entry.title)}">Suppr.</button>
      </div>
    </div>`;
}

function renderVault(filter = '') {
  const grid = $('entries-grid');
  const filtered = filter
    ? entries.filter(e =>
        e.title.toLowerCase().includes(filter) ||
        e.username.toLowerCase().includes(filter) ||
        e.url.toLowerCase().includes(filter))
    : entries;

  $('vault-empty').classList.toggle('hidden', filtered.length > 0);
  // Un seul set innerHTML => un seul reflow (au lieu de N appendChild) et zéro
  // ré-attachement de listeners (la délégation ci-dessous s'en charge une fois pour toutes).
  grid.innerHTML = filtered.map(entryCardHtml).join('');
}

// Délégation d'événements — UN listener attaché une seule fois sur la grille.
// Lookup par id dans le tableau `entries` (pas de JSON dans le DOM).
$('entries-grid').addEventListener('click', async (ev) => {
  const btn = ev.target.closest('[data-action]');
  if (!btn) return;
  const action = btn.dataset.action;
  const id = btn.dataset.id;
  const entry = entries.find(e => e.id === id);

  if (action === 'reveal') {
    const span = $(`ep-${id}`);
    if (!span) return;
    span.textContent = span.textContent.includes('•') ? (entry?.password ?? '') : '••••••••••••';
  }
  else if (action === 'copy-user') {
    if (entry) await copySecure(entry.username, 'Identifiant copié');
  }
  else if (action === 'copy-pass') {
    if (entry) await copySecure(entry.password, 'Mot de passe copié');
  }
  else if (action === 'edit') {
    if (entry) openEntryModal(entry);
  }
  else if (action === 'delete') {
    if (!confirm(`Supprimer "${btn.dataset.title}" ?`)) return;
    try {
      entries = await invoke('delete_entry', { id });
      renderVault($('search-input').value.toLowerCase().trim());
    } catch(e) { alert(e); }
  }
});

// Search — débounce léger : une frappe rapide ne déclenche qu'un seul rendu
let searchTimer;
on('search-input', 'input', () => {
  const val = $('search-input').value.toLowerCase().trim();
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => renderVault(val), 80);
});

// ══════════════════════════════════════════════════
//  ENTRY MODAL (Add / Edit)
// ══════════════════════════════════════════════════
on('add-btn', 'click', () => openEntryModal(null));
on('m-cancel', 'click', closeEntryModal);
on('m-toggle-pass', 'click', () => {
  const inp = $('m-pass');
  inp.type = inp.type === 'password' ? 'text' : 'password';
});
on('m-gen-btn', 'click', () => {
  $('m-gen-opts').classList.toggle('hidden');
});
on('m-gen-len', 'input', () => { $('m-len-val').textContent = $('m-gen-len').value; });
on('m-gen-confirm', 'click', async () => {
  const length  = parseInt($('m-gen-len').value);
  const upper   = $('m-opt-upper').checked;
  const lower   = $('m-opt-lower').checked;
  const digits  = $('m-opt-digits').checked;
  const symbols = $('m-opt-symbols').checked;
  try {
    const p = await invoke('generate_password_options', { length, upper, lower, digits, symbols });
    $('m-pass').value = p;
    $('m-pass').type = 'text';
    renderStrength(passwordStrength(p), 'm-strength-fill', 'm-strength-lbl');
    $('m-gen-opts').classList.add('hidden');
  } catch(e) { $('m-err').textContent = e; }
});
$('m-pass').addEventListener('input', () => {
  renderStrength(passwordStrength($('m-pass').value), 'm-strength-fill', 'm-strength-lbl');
});

function openEntryModal(entry) {
  editingId = entry?.id ?? null;
  $('modal-title').textContent = entry ? 'Modifier l\'entrée' : 'Nouvelle entrée';
  $('m-title').value    = entry?.title    ?? '';
  $('m-url').value      = entry?.url      ?? '';
  $('m-user').value     = entry?.username ?? '';
  $('m-pass').value     = entry?.password ?? '';
  $('m-pass').type      = 'password';
  $('m-err').textContent = '';
  $('m-gen-opts').classList.add('hidden');
  renderStrength(passwordStrength($('m-pass').value), 'm-strength-fill', 'm-strength-lbl');
  $('entry-modal').classList.remove('hidden');
}
function closeEntryModal() { $('entry-modal').classList.add('hidden'); editingId = null; }

on('m-save', 'click', async () => {
  const title    = $('m-title').value.trim();
  const url      = $('m-url').value.trim();
  const username = $('m-user').value.trim();
  const password = $('m-pass').value;
  if (!title || !password) {
    $('m-err').textContent = 'Le titre et le mot de passe sont obligatoires.'; return;
  }
  try {
    if (editingId) {
      entries = await invoke('update_entry', { id: editingId, title, username, password, url });
    } else {
      entries = await invoke('add_entry', { title, username, password, url });
    }
    closeEntryModal();
    renderVault($('search-input').value.toLowerCase().trim());
    updateHealthBadge();
  } catch(e) {
    if (e === 'LIMIT_REACHED') {
      closeEntryModal();
      showUpgradeModal();
    } else {
      $('m-err').textContent = e;
    }
  }
});

// ══════════════════════════════════════════════════
//  GÉNÉRATEUR
// ══════════════════════════════════════════════════
on('gen-len', 'input', () => { $('len-val').textContent = $('gen-len').value; });

on('gen-btn', 'click', async () => {
  const length  = parseInt($('gen-len').value);
  const upper   = $('opt-upper').checked;
  const lower   = $('opt-lower').checked;
  const digits  = $('opt-digits').checked;
  const symbols = $('opt-symbols').checked;
  try {
    const pwd = await invoke('generate_password_options', { length, upper, lower, digits, symbols });
    $('gen-out').value = pwd;
    renderStrength(passwordStrength(pwd), 'strength-fill', 'strength-lbl');
  } catch(e) { alert(e); }
});

on('gen-copy', 'click', async () => {
  const v = $('gen-out').value;
  if (!v) return;
  await copySecure(v, 'Mot de passe copié');
});

// ══════════════════════════════════════════════════
//  SANTÉ
// ══════════════════════════════════════════════════
async function loadHealth() {
  if (!isUnlocked) return;
  try {
    const h = await invoke('get_vault_health');
    $('cnt-weak').textContent  = h.weak.length;
    $('cnt-dupes').textContent = h.duplicates.length;
    $('cnt-old').textContent   = h.old.length;
    const total = h.weak.length + h.duplicates.length + h.old.length;
    $('health-badge').classList.toggle('hidden', total === 0);
    renderHealthList(h);
  } catch(e) { console.error(e); }
}

function renderHealthList(h) {
  const list = $('health-list');
  list.innerHTML = '';
  const add = (entry, tag, label) => {
    const el = document.createElement('div');
    el.className = 'health-item';
    el.innerHTML = `
      <div class="hi-info">
        <strong>${escapeHtml(entry.title)}</strong>
        <span>${escapeHtml(entry.username || entry.url || '—')}</span>
      </div>
      <span class="hi-tag ${tag}">${label}</span>`;
    list.appendChild(el);
  };
  h.weak.forEach(e => add(e, 'weak', '⚠︎ Faible'));
  h.duplicates.forEach(e => add(e, 'dupe', '↻ Doublon'));
  h.old.forEach(e => add(e, 'old', '● Ancien'));
  if (!list.children.length) {
    list.innerHTML = '<p style="color:var(--muted);text-align:center;padding:40px">✓ Coffre en parfaite santé !</p>';
  }
}

async function updateHealthBadge() {
  if (!isUnlocked) return;
  try {
    const h = await invoke('get_vault_health');
    const total = h.weak.length + h.duplicates.length + h.old.length;
    $('health-badge').classList.toggle('hidden', total === 0);
  } catch { /* silent */ }
}

on('health-refresh', 'click', loadHealth);

// ══════════════════════════════════════════════════
//  PARAMÈTRES — Import CSV
// ══════════════════════════════════════════════════
on('export-csv-btn', 'click', async () => {
  try {
    const csv = await invoke('export_csv');
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `kyber-export-${new Date().toISOString().slice(0,10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
    $('export-status').textContent = '✓ Export téléchargé.';
    $('export-status').style.color = 'var(--green)';
  } catch(e) {
    if (e === 'PRO_REQUIRED') {
      showUpgradeModal();
    } else {
      $('export-status').textContent = '✗ ' + e;
      $('export-status').style.color = 'var(--red)';
    }
  }
});

on('import-bw',  'click', () => triggerImport('bitwarden'));
on('import-1p',  'click', () => triggerImport('onepassword'));
on('import-gen', 'click', () => triggerImport('generic'));

function triggerImport(format) {
  importFormat = format;
  $('file-input').click();
}

$('file-input').addEventListener('change', async () => {
  const file = $('file-input').files[0];
  if (!file) return;
  const csv_content = await file.text();
  try {
    entries = await invoke('import_csv', { csv_content, format: importFormat });
    renderVault();
    updateHealthBadge();
    $('import-status').textContent = `✓ ${entries.length} entrées importées avec succès.`;
    $('import-status').style.color = 'var(--green)';
    showToast(`✓ ${entries.length} entrées importées`);
  } catch(e) {
    if (e === 'LIMIT_REACHED') {
      $('import-status').textContent = '✗ Limite de 10 mots de passe atteinte. Passez à Pro pour importer davantage.';
      $('import-status').style.color = 'var(--red)';
      showUpgradeModal();
    } else {
      $('import-status').textContent = '✗ ' + e;
      $('import-status').style.color = 'var(--red)';
    }
  }
  $('file-input').value = '';
});

// ══════════════════════════════════════════════════
//  SCANNER POPUP
// ══════════════════════════════════════════════════
on('sp-close', 'click', () => $('scan-popup').classList.add('hidden'));

listen('scanner-detected', event => {
  if (!isUnlocked) return;
  // Ne pas afficher si une modale Kyber est déjà ouverte
  if (!$('entry-modal').classList.contains('hidden')) return;
  if (!$('upgrade-modal').classList.contains('hidden')) return;

  const context = event.payload.context;
  // Ignorer les champs de Kyber lui-même (double sécurité côté JS)
  if (!context || context.toLowerCase().includes('kyber')) return;

  $('sp-ctx-val').textContent = context || 'Application inconnue';

  const ctx = context.toLowerCase();
  // Tokenise le titre de fenêtre sur les séparateurs courants (Chrome, Edge, Firefox)
  const ctxParts = ctx.split(/[\s\-—–|·\/\\]+/).filter(p => p.length > 2);

  const matches = entries.filter(e => {
    const domain = domainFromUrl(e.url).toLowerCase();
    const domainRoot = domain.split('.')[0]; // "google" depuis "google.com"
    const titleLower = e.title.toLowerCase();
    const titleWords = titleLower.split(/\s+/).filter(w => w.length > 2);

    if (titleLower.length > 2 && ctx.includes(titleLower)) return true;
    if (domain.length > 3 && ctx.includes(domain)) return true;
    if (domainRoot.length > 3 && ctxParts.some(p => p === domainRoot || p.includes(domainRoot) || domainRoot.includes(p))) return true;
    if (titleWords.some(w => w.length > 3 && ctxParts.includes(w))) return true;
    return false;
  });

  const matchesEl = $('sp-matches');
  matchesEl.innerHTML = '';

  if (matches.length > 0) {
    $('sp-generate').classList.add('hidden');
    matches.forEach(m => {
      const el = document.createElement('div');
      el.className = 'sp-match';
      el.innerHTML = `
        <div class="sp-match-info">
          <strong>${escapeHtml(m.title)}</strong>
          <span>${escapeHtml(m.username)}</span>
        </div>
        <div class="sp-match-btns">
          <button class="sp-autofill" data-id="${escapeHtml(m.id)}">Auto-remplir</button>
          <button class="sp-copy" data-id="${escapeHtml(m.id)}">⧉</button>
        </div>`;

      // Auto-fill : ferme popup + délai + tape le mdp (lookup depuis entries)
      el.querySelector('.sp-autofill').addEventListener('click', async () => {
        $('scan-popup').classList.add('hidden');
        const entry = entries.find(e => e.id === m.id);
        if (entry) {
          try { await invoke('autofill_password', { password: entry.password }); }
          catch(e) { console.error('autofill:', e); }
        }
      });

      // Copie sécurisée (lookup depuis entries)
      el.querySelector('.sp-copy').addEventListener('click', async () => {
        const entry = entries.find(e => e.id === m.id);
        if (entry) {
          await copySecure(entry.password, `MDP "${entry.title}" copié`);
          $('scan-popup').classList.add('hidden');
        }
      });

      matchesEl.appendChild(el);
    });
  } else {
    $('sp-generate').classList.remove('hidden');
  }

  $('scan-popup').classList.remove('hidden');
});

// ══════════════════════════════════════════════════
//  COFFRE DE FICHIERS
// ══════════════════════════════════════════════════
function setFileStatus(msg, isError = false) {
  const el = $('file-status');
  el.textContent = msg;
  el.className = 'file-status' + (isError ? ' file-status-err' : ' file-status-ok');
  el.classList.remove('hidden');
  setTimeout(() => el.classList.add('hidden'), 6000);
}

on('encrypt-file-btn', 'click', async () => {
  const sourcePath = await openDialog({
    title: 'Choisir un fichier à chiffrer',
    multiple: false,
    filters: [{ name: 'Tous les fichiers', extensions: ['*'] }],
  }).catch(() => null);
  if (!sourcePath) return;

  const src = typeof sourcePath === 'string' ? sourcePath : sourcePath?.path ?? '';
  const defaultDest = src + '.kyber';

  const destPath = await saveDialog({
    title: 'Enregistrer le fichier chiffré',
    filters: [{ name: 'Fichier Kyber chiffré', extensions: ['kyber'] }],
    defaultPath: defaultDest,
  }).catch(() => null);
  if (!destPath) return;

  try {
    await invoke('encrypt_file_cmd', { sourcePath: src, destPath });
    setFileStatus(`✓ Fichier chiffré → ${destPath}`);
    showToast('✓ Fichier chiffré avec succès');
  } catch(e) {
    setFileStatus('✗ ' + e, true);
  }
});

on('encrypt-folder-btn', 'click', async () => {
  const folderPath = await openDialog({
    title: 'Choisir un dossier à chiffrer',
    directory: true,
    multiple: false,
  }).catch(() => null);
  if (!folderPath) return;

  const src = typeof folderPath === 'string' ? folderPath : folderPath?.path ?? '';
  const folderName = src.split(/[\\/]/).pop() || 'dossier';

  const destPath = await saveDialog({
    title: 'Enregistrer le dossier chiffré',
    filters: [{ name: 'Fichier Kyber chiffré', extensions: ['kyber'] }],
    defaultPath: folderName + '.kyber',
  }).catch(() => null);
  if (!destPath) return;

  try {
    await invoke('encrypt_folder_cmd', { folderPath: src, destPath });
    setFileStatus(`✓ Dossier chiffré → ${destPath}`);
    showToast('✓ Dossier chiffré avec succès');
  } catch(e) {
    setFileStatus('✗ ' + e, true);
  }
});

on('decrypt-file-btn', 'click', async () => {
  const sourcePath = await openDialog({
    title: 'Choisir un fichier .kyber à déchiffrer',
    multiple: false,
    filters: [{ name: 'Fichier Kyber chiffré', extensions: ['kyber'] }],
  }).catch(() => null);
  if (!sourcePath) return;

  const src = typeof sourcePath === 'string' ? sourcePath : sourcePath?.path ?? '';
  // Destination = même dossier que le .kyber
  const destDir = src.substring(0, Math.max(src.lastIndexOf('/'), src.lastIndexOf('\\')) + 1) || '.';

  try {
    const result = await invoke('decrypt_file_cmd', { sourcePath: src, destDir });
    setFileStatus(`✓ "${result.name}" restauré → ${result.path}`);
    showToast(`✓ "${result.name}" déchiffré`);
  } catch(e) {
    setFileStatus('✗ ' + e, true);
  }
});

// ══════════════════════════════════════════════════
//  MIGRATION v1 → v2
// ══════════════════════════════════════════════════
async function checkVaultVersion() {
  if (!vaultPath) return;
  try {
    const isV1 = await invoke('is_vault_v1', { path: vaultPath });
    if (isV1) {
      $('migrate-banner').classList.remove('hidden');
      $('sett-vault-version').textContent = '⚠︎ Format v1 / Argon2id seul (sans Kyber1024)';
      $('sett-vault-version').style.color = 'var(--yellow)';
    } else {
      $('sett-vault-version').textContent = '✓ Format v2 / Kyber1024 + Argon2id + HKDF';
      $('sett-vault-version').style.color = 'var(--green)';
    }
  } catch { /* silent */ }
}

on('migrate-btn', 'click', () => {
  $('migrate-pass').value = '';
  $('migrate-err').textContent = '';
  $('migrate-modal').classList.remove('hidden');
});

on('migrate-dismiss', 'click', () => $('migrate-banner').classList.add('hidden'));
on('migrate-cancel', 'click', () => $('migrate-modal').classList.add('hidden'));
$('migrate-pass').addEventListener('keydown', e => { if (e.key === 'Enter') $('migrate-confirm').click(); });

on('migrate-confirm', 'click', async () => {
  const password = $('migrate-pass').value;
  if (!password) { $('migrate-err').textContent = 'Entrez votre mot de passe maître.'; return; }
  $('migrate-err').textContent = '';
  $('migrate-confirm').disabled = true;
  $('migrate-confirm').textContent = 'Migration…';
  try {
    await invoke('migrate_to_v2', { password });
    $('migrate-modal').classList.add('hidden');
    $('migrate-banner').classList.add('hidden');
    showToast('✓ Coffre migré vers Kyber1024 (v2) / Protection post-quantique activée !', 6000);
  } catch(e) {
    $('migrate-err').textContent = typeof e === 'string' ? e : 'Erreur lors de la migration.';
  } finally {
    $('migrate-confirm').disabled = false;
    $('migrate-confirm').textContent = 'Migrer';
    $('migrate-pass').value = '';
  }
});

// Fermeture des modales par touche Échap
document.addEventListener('keydown', e => {
  if (e.key !== 'Escape') return;
  if (!$('entry-modal').classList.contains('hidden')) { closeEntryModal(); return; }
  if (!$('upgrade-modal').classList.contains('hidden')) { $('upgrade-modal').classList.add('hidden'); return; }
  if (!$('migrate-modal').classList.contains('hidden')) { $('migrate-modal').classList.add('hidden'); return; }
  if (!$('update-modal').classList.contains('hidden')) { closeUpdateModal(); return; }
  if (!$('scan-popup').classList.contains('hidden')) { $('scan-popup').classList.add('hidden'); }
});

// Clic sur le fond des modales pour fermer
$('entry-modal').addEventListener('click', e => { if (e.target === $('entry-modal')) closeEntryModal(); });
$('upgrade-modal').addEventListener('click', e => { if (e.target === $('upgrade-modal')) $('upgrade-modal').classList.add('hidden'); });
$('migrate-modal').addEventListener('click', e => { if (e.target === $('migrate-modal')) $('migrate-modal').classList.add('hidden'); });
$('update-modal').addEventListener('click', e => { if (e.target === $('update-modal')) closeUpdateModal(); });

// Bouton "Générer & Sauvegarder" dans la popup
on('sp-gen-len', 'input', () => { $('sp-len-val').textContent = $('sp-gen-len').value; });
on('sp-gen-btn', 'click', async () => {
  const length  = parseInt($('sp-gen-len').value);
  const upper   = $('sp-opt-upper').checked;
  const lower   = $('sp-opt-lower').checked;
  const digits  = $('sp-opt-digits').checked;
  const symbols = $('sp-opt-symbols').checked;
  try {
    const p = await invoke('generate_password_options', { length, upper, lower, digits, symbols });
    const ctx = $('sp-ctx-val').textContent.trim();
    $('scan-popup').classList.add('hidden');
    // Utilise le nom complet de l'app comme titre ; URL laissée vide (optionnelle)
    openEntryModal({ title: ctx || 'Nouveau', url: '', username: '', password: p });
    $('m-pass').type = 'text';
    renderStrength(passwordStrength(p), 'm-strength-fill', 'm-strength-lbl');
  } catch(e) { console.error(e); }
});
