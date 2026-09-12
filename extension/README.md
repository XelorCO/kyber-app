# Kyber — Extension navigateur

Mode « compagnon » : l'extension ne stocke aucune donnée d'identifiant
elle-même. Elle parle en local à l'app Kyber installée (native messaging),
qui fait tout le travail de déchiffrement avec le vrai fichier `.vault`.

Gratuite et open source, comme le reste de Kyber — aucune licence, aucune
limite. Seule contrainte : l'app de bureau Kyber doit être installée sur la
machine (c'est elle qui détient et déchiffre le coffre).

## Connexion live à l'app

Si l'app de bureau tourne déjà ET que son coffre est déverrouillé, l'extension
se connecte automatiquement à cette session **sans redemander le mot de
passe maître**. Sinon (app fermée, ou coffre verrouillé dans l'app), elle
retombe sur la saisie manuelle habituelle — rien ne change dans ce cas.

Comment ça marche (voir `src-tauri/src/session_bridge.rs`) :
1. Au démarrage, l'app ouvre une petite socket locale sur `127.0.0.1:47732`
   (jamais exposée au réseau) et écrit le port + un jeton aléatoire dans
   `~/.kyber/session.json`.
2. Chaque fois que le coffre est déverrouillé ou modifié (ajout, suppression,
   édition, import), l'app met à jour un cache d'entrées en mémoire associé à
   ce jeton.
3. À l'ouverture du popup, l'extension (via `kyber-native-host`) lit
   `session.json`, se connecte à la socket et demande `get_entries` avec le
   jeton. Si le coffre est bien déverrouillé côté app, elle reçoit les
   entrées directement ; sinon (`LOCKED`) ou si l'app n'est pas joignable
   (`NO_SESSION`), elle affiche la vue de déverrouillage classique.
4. Un badge « ● Connecté à l'app » apparaît dans l'en-tête du popup quand la
   session vient de ce mode.
5. En mode live, l'app est la **source de vérité** : le popup se
   re-synchronise à CHAQUE ouverture. Une entrée ajoutée/modifiée dans l'app
   apparaît immédiatement, et le bouton « Verrouiller » de l'app coupe la
   session de l'extension au prochain clic (réponse `LOCKED`).
6. Si l'app devient injoignable (désinstallée, hôte natif cassé), tout cache
   résiduel de l'extension est purgé : sans l'app, aucune donnée Kyber ne
   reste accessible dans le navigateur.

**Modèle de confiance** : identique aux autres fichiers sous `~/.kyber`
(`last_vault.txt`, `vaults.json`) — protection par les permissions du compte
utilisateur courant, pas de chiffrement supplémentaire du jeton. La socket
n'écoute que sur la boucle locale (127.0.0.1), donc jamais accessible depuis
le réseau ; seul un autre processus tournant sous le même compte utilisateur
peut la contacter. `ping` (sans jeton) ne révèle que « l'app tourne » ;
`get_entries` exige le jeton exact pour toute donnée réelle.

## Installation (chargement local, non publiée sur le store)

1. **Builder le binaire natif** (si pas déjà fait) :
   ```
   cd src-tauri
   cargo build --release --bin kyber-native-host
   ```

2. **Enregistrer l'hôte natif** (une fois, ou après un rebuild) :
   ```
   powershell -File extension\install-native-host.ps1
   ```
   Ça écrit le manifeste dans `%LOCALAPPDATA%\Kyber\` et l'enregistre dans
   le registre pour Chrome et Edge.

3. **Charger l'extension** :
   - Ouvrir `edge://extensions/` (ou `chrome://extensions/`)
   - Activer « Mode développeur » (en bas à gauche)
   - Cliquer « Charger l'extension décompressée »
   - Sélectionner le dossier `Kyber/extension/`

4. **Vérifier l'ID de l'extension** affiché sur la carte : il doit être
   `ngekfdpkgjmdiepbiedbfnmkaglfoeih` (fixé via la clé publique dans
   `manifest.json`, donc stable peu importe où le dossier est placé). S'il
   diffère, relancer `install-native-host.ps1 -ExtensionId <le vrai id>`.

5. Cliquer l'icône Kyber dans la barre d'outils → déverrouiller avec le
   mot de passe maître du coffre → « Remplir » sur un champ identifiant/mdp
   d'une page.

## Fonctionnalités

- **Connexion live** : si le coffre est déjà ouvert dans l'app, l'extension
  récupère les entrées directement, sans redemander le mot de passe (voir
  section dédiée ci-dessus).
- Détection de l'app installée (`ping`).
- Déverrouillage du vrai `.vault` (coffre v1 et v2) via l'hôte natif (repli
  automatique si pas de session live).
- Liste des entrées + recherche, triée avec le site actuellement ouvert en
  premier (badge « ◆ ce site »).
- Remplissage auto (identifiant + mot de passe) sur la page active, injecté à
  la demande (`chrome.scripting`, monde isolé) — aucun script ne reste posé
  sur les pages visitées.
- Copier identifiant / copier mot de passe individuellement.
- **Générateur** de mots de passe (même algorithme que l'app : rejet du biais
  modulo, `crypto.getRandomValues`), 100 % côté navigateur.
- **Chiffrement de fichiers** (onglet « Fichier ») : chiffre/déchiffre
  n'importe quel fichier par mot de passe, entièrement dans le navigateur
  (WebCrypto + ML-KEM-1024 + Argon2id). Format `.kyber` identique à celui de
  l'app de bureau et du site — un fichier chiffré dans l'extension se
  déchiffre partout ailleurs.
- Verrouillage automatique après 15 min d'inactivité (`chrome.alarms`), en
  plus du verrouillage manuel.
- Rien n'est jamais écrit sur disque par l'extension : les entrées déchiffrées
  vivent en `chrome.storage.session` (mémoire, vidé à la fermeture du
  navigateur).
- Permissions réduites au minimum : `activeTab` + `scripting` (pas de
  `host_permissions` ni de content script permanent sur `<all_urls>`).

## Audit sécurité — trouvé et corrigé

- **Détournement d'autofill** : le remplissage ne prenait pas en compte la
  visibilité des champs — une page pourrait placer un champ mot de passe
  caché/hors-écran en premier pour capter l'autofill. Corrigé : seuls les
  champs réellement visibles (`display`, `visibility`, `opacity`, taille)
  sont candidats.
- **Code mort trompeur** : l'hôte natif exposait des commandes
  `list_entries`/`lock` supposant un process persistant, alors que
  `chrome.runtime.sendNativeMessage` relance un process neuf à chaque appel
  (Chrome ferme le pipe après une réponse) — ces commandes n'étaient jamais
  atteignables. Supprimées, le binaire est sans état, cohérent avec son usage
  réel.
- **Surface d'attaque** : remplacement du content script permanent sur toutes
  les pages (`content_scripts` + `host_permissions: <all_urls>`) par une
  injection ponctuelle via `activeTab`, uniquement au moment où l'utilisateur
  clique « Remplir ».
- **Limite de taille des messages natifs** : borne (1 Mo) côté process, en
  plus de celle déjà imposée par le navigateur.

## Limites connues

- **Presse-papier non auto-effacé** : copier un mot de passe le laisse dans le
  presse-papier jusqu'à écrasement manuel. Un effacement différé fiable
  demanderait un minuteur côté service worker, mais l'API presse-papier n'est
  pas garantie disponible dans ce contexte selon les navigateurs — pas
  implémenté pour éviter une fausse impression de sécurité si ça ne se
  déclenche pas partout.
- **Correspondance de domaine = indicative, pas bloquante** : « Remplir »
  reste possible sur une entrée d'un autre site que celui ouvert (comme la
  plupart des gestionnaires de mots de passe) ; seul le tri/badge aide à
  choisir la bonne entrée. Un blocage strict casserait des cas d'usage
  légitimes (sous-domaines, comptes multi-sites).
- **Binaire natif non signé** : `kyber-native-host.exe` n'a pas de signature
  de code, contrairement à l'app principale. Même niveau de confiance qu'un
  exécutable local classique (même utilisateur).
- **Pont de session = une seule instance à la fois** : si deux instances de
  l'app tournent en parallèle, seule la première à démarrer obtient le port
  `47732` ; la seconde fonctionne normalement mais sans pont actif
  (l'extension retombe sur la saisie du mot de passe). `~/.kyber/session.json`
  n'est pas nettoyé à la fermeture de l'app : sans conséquence, une connexion
  à un process arrêté échoue proprement et déclenche le même repli.

## Bibliothèques embarquées (`vendor/`)

Le chiffrement de fichiers fonctionne sans build : les dépendances sont
vendues telles quelles dans `vendor/` et chargées par `filecrypto.js`.

| Fichier | Paquet | Usage |
|---|---|---|
| `vendor/mlkem/` | `mlkem` (ESM) | ML-KEM-1024 |
| `vendor/fflate.js` | `fflate` (ESM) | gzip / gunzip |
| `vendor/argon2.js` | `hash-wasm` (UMD, wasm en base64) | Argon2id |

`hash-wasm` instancie du WebAssembly → `manifest.json` déclare
`content_security_policy.extension_pages` avec `'wasm-unsafe-eval'`
(Chrome 103+). Le format produit est **KYBP**, identique à celui du site
(`kyber-site/lib/kyberfile.ts`).

Test de compatibilité (nécessite le dépôt `kyber-site` en frère, Node 22+) :

```bash
cd ../../Kyber-site        # pour résoudre hash-wasm
node ../Kyber/extension/test/crosscheck.mjs
```

Il vérifie qu'un fichier chiffré dans l'extension se déchiffre sur le site et
inversement.

## Décision produit : compagnon uniquement pour le coffre

Pour la **gestion du coffre**, l'extension exige l'app de bureau installée :
c'est elle qui détient le `.vault` et le déchiffre. Il n'y a pas de « mode
autonome » où l'extension recréerait un coffre complet dans le navigateur.

Le **chiffrement de fichiers**, lui, est autonome (WebCrypto pur, aucun appel
à l'app) : c'est une commodité indépendante du coffre, utile même sans app
lancée.

## Pas encore fait

- Enregistrement automatique de l'hôte natif par l'installeur Windows (NSIS) :
  pour l'instant le script PowerShell est manuel.
- Support Firefox (manifeste de native messaging différent) et macOS/Linux
  (chemins de registre différents).
- Icône de statut sur le bouton (verrouillé/déverrouillé) façon Bitwarden.
