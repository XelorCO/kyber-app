// Service worker — relaie les commandes du popup vers l'hôte natif (l'app
// Kyber installée) et garde les entrées déchiffrées en mémoire de session
// UNIQUEMENT (chrome.storage.session : jamais écrit sur disque, vidé à la
// fermeture du navigateur). Un seul aller-retour natif par commande, donc
// aucune dépendance à la durée de vie du service worker.

const NATIVE_HOST = "com.kyber_security.native_host";
const AUTOLOCK_ALARM = "kyber-autolock";
const AUTOLOCK_MINUTES = 15;

function callNativeHost(message) {
  return new Promise((resolve) => {
    chrome.runtime.sendNativeMessage(NATIVE_HOST, message, (response) => {
      if (chrome.runtime.lastError) {
        resolve({ ok: false, error: "NATIVE_HOST_UNAVAILABLE", detail: chrome.runtime.lastError.message });
      } else {
        resolve(response ?? { ok: false, error: "NO_RESPONSE" });
      }
    });
  });
}

async function handlePing() {
  return callNativeHost({ cmd: "ping" });
}

async function handleGetLastVaultPath() {
  return callNativeHost({ cmd: "get_last_vault_path" });
}

async function handleUnlock(path, password) {
  const res = await callNativeHost({ cmd: "unlock", path, password });
  if (res.ok) {
    await chrome.storage.session.set({
      unlocked: true,
      vaultPath: path,
      entries: res.data.entries,
      unlockedAt: Date.now(),
    });
    chrome.alarms.create(AUTOLOCK_ALARM, { delayInMinutes: AUTOLOCK_MINUTES });
  }
  return res;
}

async function handleGetEntries() {
  const state = await chrome.storage.session.get(["unlocked", "entries", "viaLiveSession"]);
  if (!state.unlocked) return { ok: false, error: "LOCKED" };
  return { ok: true, data: { entries: state.entries, viaLiveSession: !!state.viaLiveSession } };
}

// Tente de récupérer les entrées d'un coffre déjà déverrouillé dans l'app de
// bureau EN COURS D'EXÉCUTION (pont local `session_bridge` côté app), sans
// jamais redemander le mot de passe maître. Retombe silencieusement sur
// NO_SESSION si l'app n'est pas lancée ou si son coffre n'est pas ouvert —
// le popup enchaîne alors sur la saisie manuelle habituelle.
async function handleTryLiveSession() {
  const res = await callNativeHost({ cmd: "try_live_session" });
  if (res.ok) {
    await chrome.storage.session.set({
      unlocked: true,
      vaultPath: null,
      entries: res.data.entries,
      unlockedAt: Date.now(),
      viaLiveSession: true,
    });
    chrome.alarms.create(AUTOLOCK_ALARM, { delayInMinutes: AUTOLOCK_MINUTES });
  }
  return res;
}

async function handleLock() {
  await chrome.storage.session.remove(["unlocked", "vaultPath", "entries", "unlockedAt"]);
  await chrome.alarms.clear(AUTOLOCK_ALARM);
  return { ok: true, data: null };
}

// Verrouillage automatique après inactivité : le coffre déchiffré ne reste
// jamais en mémoire indéfiniment, même si l'utilisateur oublie de verrouiller.
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === AUTOLOCK_ALARM) handleLock();
});

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  (async () => {
    switch (msg?.type) {
      case "PING":
        sendResponse(await handlePing());
        break;
      case "GET_LAST_VAULT_PATH":
        sendResponse(await handleGetLastVaultPath());
        break;
      case "UNLOCK":
        sendResponse(await handleUnlock(msg.path, msg.password));
        break;
      case "GET_ENTRIES":
        sendResponse(await handleGetEntries());
        break;
      case "TRY_LIVE_SESSION":
        sendResponse(await handleTryLiveSession());
        break;
      case "LOCK":
        sendResponse(await handleLock());
        break;
      default:
        sendResponse({ ok: false, error: "UNKNOWN_MESSAGE" });
    }
  })();
  return true; // réponse asynchrone
});
