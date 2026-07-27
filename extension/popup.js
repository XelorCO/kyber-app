const views = {
  loading: document.getElementById("view-loading"),
  notInstalled: document.getElementById("view-not-installed"),
  locked: document.getElementById("view-locked"),
  unlocked: document.getElementById("view-unlocked"),
};

function showView(name) {
  Object.values(views).forEach((v) => v.classList.add("hidden"));
  views[name].classList.remove("hidden");
}

function sendToBackground(type, extra = {}) {
  return new Promise((resolve) => chrome.runtime.sendMessage({ type, ...extra }, resolve));
}

function setTierBadge(tier) {
  const badge = document.getElementById("tier-badge");
  if (!tier) {
    badge.classList.add("hidden");
    return;
  }
  badge.textContent = tier === "pro" ? "PRO" : tier === "famille" ? "FAMILLE" : tier.toUpperCase();
  badge.classList.remove("hidden");
}

// Affiché uniquement quand le coffre vient d'une session déjà ouverte dans
// l'app de bureau (pont local), pour que l'utilisateur comprenne pourquoi
// aucun mot de passe ne lui a été demandé.
function setLiveBadge(isLive) {
  const badge = document.getElementById("conn-badge");
  badge.textContent = "Connecté à l'app";
  badge.title = "Coffre déjà ouvert dans l'app Kyber / connexion automatique, sans mot de passe.";
  badge.classList.toggle("hidden", !isLive);
}

function errorMessage(code) {
  const known = {
    NOT_FOUND: "Coffre introuvable à ce chemin.",
    NATIVE_HOST_UNAVAILABLE: "Impossible de contacter l'app Kyber. Est-elle installée ?",
    "Mot de passe incorrect.": "Mot de passe incorrect.",
    "Coffre invalide ou corrompu.": "Ce fichier n'est pas un coffre Kyber valide.",
  };
  return known[code] || code || "Erreur inconnue.";
}

let allEntries = [];
let currentTabDomain = "";
let currentTabId = null;
const revealedIds = new Set();

function domainOf(url) {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

function matchesCurrentSite(entry) {
  const d = domainOf(entry.url);
  return d && currentTabDomain && (d === currentTabDomain || currentTabDomain.endsWith("." + d));
}

// Même palette et même hash que l'avatar-lettre de l'app desktop (ui/main.js
// `letterIcon`) : aucun appel réseau, un simple hash déterministe du domaine.
const AVATAR_PALETTE = ["#6366F1", "#8B5CF6", "#EC4899", "#EF4444", "#F59E0B", "#10B981", "#A78BFA", "#3B82F6"];

function avatarFor(entry) {
  const domain = domainOf(entry.url) || entry.title || "?";
  const letter = domain[0]?.toUpperCase() || "?";
  let hash = 0;
  for (let i = 0; i < domain.length; i++) hash = domain.charCodeAt(i) + ((hash << 5) - hash);
  const bg = AVATAR_PALETTE[Math.abs(hash) % AVATAR_PALETTE.length];
  return { letter, bg };
}

function renderEntries(filter = "") {
  const container = document.getElementById("entries");
  const f = filter.toLowerCase().trim();
  let list = f
    ? allEntries.filter(
        (e) =>
          e.title.toLowerCase().includes(f) ||
          e.username.toLowerCase().includes(f) ||
          e.url.toLowerCase().includes(f)
      )
    : allEntries;

  // Les entrées du site actuellement ouvert remontent en premier.
  list = [...list].sort((a, b) => Number(matchesCurrentSite(b)) - Number(matchesCurrentSite(a)));

  if (list.length === 0) {
    container.innerHTML = `<div class="entry-empty">${f ? "Aucun résultat." : "Aucune entrée."}</div>`;
    return;
  }

  container.innerHTML = list
    .map((e) => {
      const match = matchesCurrentSite(e);
      const { letter, bg } = avatarFor(e);
      const revealed = revealedIds.has(e.id);
      const id = escapeHtml(e.id);
      return `
      <div class="entry-row" data-id="${id}">
        <div class="avatar" style="--avatar-bg:${bg}">${escapeHtml(letter)}</div>
        <div class="entry-info">
          <div class="entry-title">
            ${escapeHtml(e.title)}
            ${match ? '<span class="match-badge">◆ ce site</span>' : ""}
          </div>
          <div class="entry-user">${revealed ? escapeHtml(e.password) : escapeHtml(e.username || domainOf(e.url))}</div>
        </div>
        <button class="icon-btn" data-action="reveal" data-id="${id}" title="${revealed ? "Masquer" : "Voir le mot de passe"}">${revealed ? "○" : "◉"}</button>
        <button class="icon-btn" data-action="copy-user" data-id="${id}" title="Copier l'identifiant">⧉</button>
        <button class="icon-btn" data-action="copy-pass" data-id="${id}" title="Copier le mot de passe">⚿</button>
        <button class="entry-fill" data-action="fill" data-id="${id}">Remplir</button>
      </div>`;
    })
    .join("");

  container.querySelectorAll("[data-action]").forEach((btn) => {
    btn.addEventListener("click", () => onEntryAction(btn.dataset.action, btn.dataset.id));
  });
}

function escapeHtml(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

let toastTimer = null;
function showToast(text, ms = 1600) {
  const el = document.getElementById("toast");
  el.textContent = text;
  el.classList.remove("hidden");
  el.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    el.classList.remove("show");
    setTimeout(() => el.classList.add("hidden"), 200);
  }, ms);
}

function onEntryAction(action, id) {
  const entry = allEntries.find((e) => e.id === id);
  if (!entry) return;
  if (action === "copy-user") return copyToClipboard(entry.username, "Identifiant copié");
  if (action === "copy-pass") return copyToClipboard(entry.password, "Mot de passe copié");
  if (action === "fill") return fillEntry(entry);
  if (action === "reveal") {
    if (revealedIds.has(id)) revealedIds.delete(id);
    else revealedIds.add(id);
    renderEntries(document.getElementById("search").value);
  }
}

async function copyToClipboard(text, label = "Copié") {
  try {
    await navigator.clipboard.writeText(text ?? "");
    showToast(`⧉ ${label} !`);
  } catch {
    showToast("Impossible de copier.");
  }
}

// Fonctions injectées telles quelles dans la page active (aucun script persistant
// n'est laissé sur la page — chrome.scripting les exécute une fois et c'est tout).
function isVisibleField(el) {
  if (!el.offsetParent && getComputedStyle(el).position !== "fixed") return false;
  const style = getComputedStyle(el);
  if (style.display === "none" || style.visibility === "hidden" || Number(style.opacity) === 0) return false;
  const rect = el.getBoundingClientRect();
  return rect.width > 2 && rect.height > 2;
}

function setNativeValue(el, value) {
  const proto = Object.getPrototypeOf(el);
  const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
  if (setter) setter.call(el, value);
  else el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
}

function injectedFill(username, password) {
  // Une page malveillante pourrait placer un champ mot de passe caché avant
  // le vrai, pour capter un autofill "au premier trouvé" (technique connue
  // de détournement des gestionnaires de mots de passe). On ne remplit donc
  // jamais un champ que l'utilisateur ne peut pas réellement voir.
  function isVisible(el) {
    if (!el.offsetParent && getComputedStyle(el).position !== "fixed") return false;
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden" || Number(style.opacity) === 0) return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 2 && rect.height > 2;
  }
  function setNativeValue(el, value) {
    const proto = Object.getPrototypeOf(el);
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    if (setter) setter.call(el, value);
    else el.value = value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  const passwordField = Array.from(document.querySelectorAll('input[type="password"]:not([disabled])')).find(isVisible);
  if (!passwordField) return { ok: false, error: "Aucun champ mot de passe visible trouvé sur cette page." };

  let usernameField = Array.from(
    document.querySelectorAll(
      'input[type="email"], input[autocomplete="username"], input[name*="user" i], input[name*="email" i], input[id*="user" i], input[id*="email" i]'
    )
  ).find(isVisible);

  if (!usernameField && passwordField.form) {
    const inputs = Array.from(passwordField.form.querySelectorAll("input")).filter(isVisible);
    const idx = inputs.indexOf(passwordField);
    for (let i = idx - 1; i >= 0; i--) {
      if (inputs[i].type === "text" || inputs[i].type === "email") {
        usernameField = inputs[i];
        break;
      }
    }
  }

  if (usernameField && username) setNativeValue(usernameField, username);
  setNativeValue(passwordField, password);
  return { ok: true };
}

// Variante générateur : ne touche qu'au champ mot de passe (on ne veut pas
// écraser un identifiant déjà saisi juste parce qu'on a généré un nouveau mdp).
function injectedFillPasswordOnly(password) {
  function isVisible(el) {
    if (!el.offsetParent && getComputedStyle(el).position !== "fixed") return false;
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden" || Number(style.opacity) === 0) return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 2 && rect.height > 2;
  }
  function setNativeValue(el, value) {
    const proto = Object.getPrototypeOf(el);
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    if (setter) setter.call(el, value);
    else el.value = value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }
  const passwordField = Array.from(document.querySelectorAll('input[type="password"]:not([disabled])')).find(isVisible);
  if (!passwordField) return { ok: false, error: "Aucun champ mot de passe visible trouvé sur cette page." };
  setNativeValue(passwordField, password);
  return { ok: true };
}

async function fillEntry(entry) {
  const statusEl = document.getElementById("fill-status");
  statusEl.textContent = "";
  if (!currentTabId) return;
  try {
    const [{ result } = {}] = await chrome.scripting.executeScript({
      target: { tabId: currentTabId },
      func: injectedFill,
      args: [entry.username, entry.password],
    });
    if (result?.ok) {
      window.close();
    } else {
      statusEl.textContent = result?.error || "Impossible de remplir cette page.";
    }
  } catch {
    statusEl.textContent = "Cette page ne permet pas le remplissage automatique.";
  }
}

// ══════════════════════════════════════════════════
//  GÉNÉRATEUR — même algorithme que l'app desktop
//  (rejection sampling anti-biais, mêmes jeux de caractères et le même
//  ordre upper→lower→digits→symbols que generate_password_options côté
//  Rust) mais 100% côté navigateur via crypto.getRandomValues (CSPRNG).
// ══════════════════════════════════════════════════
function generatePassword(length, upper, lower, digits, symbols) {
  let charset = "";
  if (upper) charset += "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
  if (lower) charset += "abcdefghijklmnopqrstuvwxyz";
  if (digits) charset += "0123456789";
  if (symbols) charset += "!@#$%^&*()_+-=[]{}|;:,.<>?";
  if (!charset) return { ok: false, error: "Sélectionnez au moins un type de caractère." };

  const n = charset.length;
  const threshold = (256 - (256 % n)) & 0xff;
  const result = [];
  const buf = new Uint8Array(64);
  while (result.length < length) {
    crypto.getRandomValues(buf);
    for (let i = 0; i < buf.length && result.length < length; i++) {
      if (buf[i] < threshold) result.push(charset[buf[i] % n]);
    }
  }
  return { ok: true, password: result.join("") };
}

function passwordStrength(pwd) {
  let s = 0;
  if (pwd.length >= 8) s += 10;
  if (pwd.length >= 12) s += 15;
  if (pwd.length >= 16) s += 10;
  if (pwd.length >= 24) s += 5;
  if (/[A-Z]/.test(pwd)) s += 15;
  if (/[a-z]/.test(pwd)) s += 10;
  if (/[0-9]/.test(pwd)) s += 15;
  if (/[^A-Za-z0-9]/.test(pwd)) s += 20;
  return Math.min(s, 100);
}

function renderStrength(score) {
  const fill = document.getElementById("strength-fill");
  const lbl = document.getElementById("strength-lbl");
  fill.style.width = score + "%";
  if (score < 30) { fill.style.background = "#EF4444"; lbl.textContent = "Très faible"; lbl.style.color = "#EF4444"; }
  else if (score < 50) { fill.style.background = "#F59E0B"; lbl.textContent = "Faible"; lbl.style.color = "#F59E0B"; }
  else if (score < 70) { fill.style.background = "#FBBF24"; lbl.textContent = "Moyen"; lbl.style.color = "#FBBF24"; }
  else if (score < 90) { fill.style.background = "#10B981"; lbl.textContent = "Fort"; lbl.style.color = "#10B981"; }
  else { fill.style.background = "#6366F1"; lbl.textContent = "Très fort"; lbl.style.color = "#6366F1"; }
}

document.getElementById("gen-len").addEventListener("input", (e) => {
  document.getElementById("len-val").textContent = e.target.value;
});

document.getElementById("gen-btn").addEventListener("click", () => {
  const length = parseInt(document.getElementById("gen-len").value, 10);
  const upper = document.getElementById("opt-upper").checked;
  const lower = document.getElementById("opt-lower").checked;
  const digits = document.getElementById("opt-digits").checked;
  const symbols = document.getElementById("opt-symbols").checked;
  const statusEl = document.getElementById("gen-status");
  statusEl.textContent = "";

  const res = generatePassword(length, upper, lower, digits, symbols);
  if (!res.ok) {
    statusEl.textContent = res.error;
    return;
  }
  document.getElementById("gen-out").value = res.password;
  renderStrength(passwordStrength(res.password));
});

document.getElementById("gen-copy").addEventListener("click", () => {
  const v = document.getElementById("gen-out").value;
  if (!v) return;
  copyToClipboard(v, "Mot de passe copié");
});

document.getElementById("gen-fill-btn").addEventListener("click", async () => {
  const statusEl = document.getElementById("gen-status");
  const pwd = document.getElementById("gen-out").value;
  statusEl.textContent = "";
  if (!pwd) {
    statusEl.textContent = "Générez d'abord un mot de passe.";
    return;
  }
  if (!currentTabId) return;
  try {
    const [{ result } = {}] = await chrome.scripting.executeScript({
      target: { tabId: currentTabId },
      func: injectedFillPasswordOnly,
      args: [pwd],
    });
    if (result?.ok) {
      showToast("✦ Mot de passe inséré sur la page");
    } else {
      statusEl.textContent = result?.error || "Impossible de remplir cette page.";
    }
  } catch {
    statusEl.textContent = "Cette page ne permet pas le remplissage automatique.";
  }
});

// ══════════════════════════════════════════════════
//  ONGLETS Coffre / Générateur
// ══════════════════════════════════════════════════
function switchTab(name) {
  document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t.dataset.tab === name));
  document.getElementById("tabpanel-vault").classList.toggle("hidden", name !== "vault");
  document.getElementById("tabpanel-generator").classList.toggle("hidden", name !== "generator");
}
document.getElementById("tab-vault").addEventListener("click", () => switchTab("vault"));
document.getElementById("tab-generator").addEventListener("click", () => switchTab("generator"));

async function boot() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  currentTabId = tab?.id ?? null;
  currentTabDomain = tab?.url ? domainOf(tab.url) : "";

  // Deux appels indépendants (cache de session + détection de l'app) :
  // en parallèle pour ouvrir le popup plus vite.
  const [existing, ping] = await Promise.all([
    sendToBackground("GET_ENTRIES"),
    sendToBackground("PING"),
  ]);

  // Principe compagnon : sans l'app installée, pas de Kyber dans le
  // navigateur — on purge aussi tout cache résiduel pour qu'aucune donnée
  // ne reste utilisable.
  if (!ping.ok) {
    if (existing.ok) await sendToBackground("LOCK");
    showView("notInstalled");
    return;
  }
  setTierBadge(ping.data.tier);

  const cameFromLive = existing.ok && existing.data.viaLiveSession;

  if (existing.ok && !cameFromLive) {
    // Déverrouillé manuellement dans l'extension : le cache fait foi
    // (l'app n'est pas forcément lancée dans ce mode).
    allEntries = existing.data.entries;
    setLiveBadge(false);
    renderEntries();
    showView("unlocked");
    return;
  }

  // Session live : l'app est la source de vérité. On re-synchronise à
  // CHAQUE ouverture du popup — les entrées ajoutées/modifiées dans l'app
  // apparaissent immédiatement, et un verrouillage côté app coupe
  // l'extension au prochain clic. Sinon (pas de cache), on tente quand
  // même la connexion live avant de demander le mot de passe.
  document.getElementById("loading-text").textContent = cameFromLive
    ? "Synchronisation avec l'app…"
    : "Recherche d'une session déjà ouverte…";
  showView("loading");
  const live = await sendToBackground("TRY_LIVE_SESSION");
  if (live.ok) {
    allEntries = live.data.entries;
    setLiveBadge(true);
    renderEntries();
    showView("unlocked");
    return;
  }

  // Le coffre a été verrouillé (ou l'app fermée) depuis la dernière fois :
  // on suit, l'extension se verrouille aussi.
  if (cameFromLive) await sendToBackground("LOCK");

  const lastPath = await sendToBackground("GET_LAST_VAULT_PATH");
  if (lastPath.ok && lastPath.data.path) {
    document.getElementById("vault-path").value = lastPath.data.path;
  }
  showView("locked");
}

document.getElementById("unlock-btn").addEventListener("click", async () => {
  const path = document.getElementById("vault-path").value.trim();
  const password = document.getElementById("master-pass").value;
  const errEl = document.getElementById("locked-err");
  errEl.textContent = "";

  if (!path || !password) {
    errEl.textContent = "Renseignez le chemin et le mot de passe.";
    return;
  }

  const btn = document.getElementById("unlock-btn");
  btn.disabled = true;
  btn.textContent = "Déverrouillage…";
  const res = await sendToBackground("UNLOCK", { path, password });
  btn.disabled = false;
  btn.textContent = "Déverrouiller";

  if (!res.ok) {
    errEl.textContent = errorMessage(res.error);
    return;
  }
  document.getElementById("master-pass").value = "";
  allEntries = res.data.entries;
  renderEntries();
  showView("unlocked");
});

document.getElementById("master-pass").addEventListener("keydown", (e) => {
  if (e.key === "Enter") document.getElementById("unlock-btn").click();
});

document.getElementById("search").addEventListener("input", (e) => renderEntries(e.target.value));

document.getElementById("lock-btn").addEventListener("click", async () => {
  await sendToBackground("LOCK");
  allEntries = [];
  revealedIds.clear();
  document.getElementById("master-pass").value = "";
  document.getElementById("gen-out").value = "";
  setLiveBadge(false);
  switchTab("vault");
  showView("locked");
});

boot();
