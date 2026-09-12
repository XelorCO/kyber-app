// Vérifie que le chiffrement de fichiers de l'EXTENSION (extension/filecrypto.js,
// format KYBP) est bit-à-bit compatible avec celui du SITE
// (kyber-site/lib/kyberfile.ts) : un fichier chiffré d'un côté se déchiffre de
// l'autre, dans les deux sens.
//
// Exécuter depuis le dépôt du site (pour résoudre hash-wasm/mlkem/fflate) :
//   node ../Kyber/extension/test/crosscheck.mjs
// ou (chemins absolus) :
//   node "C:/.../Kyber/extension/test/crosscheck.mjs"   avec CWD = kyber-site
//
// Node 22+ requis (type stripping natif pour l'import .ts).

import { pathToFileURL } from "node:url";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const EXT_MODULE = pathToFileURL(join(HERE, "..", "filecrypto.js")).href;
// kyber-site est le dépôt frère de kyber-app
const SITE_ROOT = resolve(HERE, "..", "..", "..", "Kyber-site");
const SITE_KYBERFILE = pathToFileURL(join(SITE_ROOT, "lib", "kyberfile.ts")).href;

// filecrypto.js lit globalThis.hashwasm.argon2id (fourni dans le navigateur par
// vendor/argon2.js). En Node on le branche sur le paquet hash-wasm du site.
globalThis.hashwasm = await import(
  pathToFileURL(join(SITE_ROOT, "node_modules", "hash-wasm", "dist", "index.esm.js")).href
);

const ext = await import(EXT_MODULE);
const site = await import(SITE_KYBERFILE);

let failures = 0;
const ok = (n) => console.log(`  OK  ${n}`);
const ko = (n, e) => {
  failures++;
  console.error(`  KO  ${n} — ${e}`);
};

function eqBytes(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

// Données de test : binaire pseudo-aléatoire + un peu de texte compressible.
const data = new Uint8Array(512 * 1024);
for (let i = 0; i < data.length; i++) data[i] = (i * 31 + 7) % 256;
const password = ext.generateStrongPassword();
console.log(`Mot de passe de test : ${password.length} caractères`);
console.log(`Module extension : ${EXT_MODULE}`);
console.log(`Module site      : ${SITE_KYBERFILE}\n`);

// 1. Extension chiffre → Site déchiffre
try {
  const blob = await ext.encryptKyberFile(data, "note.txt", password);
  const { meta, data: out } = await site.decryptKyberFile(blob, password);
  if (meta.name !== "note.txt") ko("ext→site : nom", meta.name);
  else if (!eqBytes(out, data)) ko("ext→site : données", "octets différents");
  else ok("ext chiffre → site déchiffre (données + nom intacts)");
} catch (e) {
  ko("ext→site", e.message || e);
}

// 2. Site chiffre → Extension déchiffre
try {
  const blob = await site.encryptKyberFile(data, "note.txt", password);
  const { meta, data: out } = await ext.decryptKyberFile(blob, password);
  if (meta.name !== "note.txt") ko("site→ext : nom", meta.name);
  else if (!eqBytes(out, data)) ko("site→ext : données", "octets différents");
  else ok("site chiffre → ext déchiffre (données + nom intacts)");
} catch (e) {
  ko("site→ext", e.message || e);
}

// 3. En-tête binaire identique en taille/structure (mêmes constantes)
try {
  const a = await ext.encryptKyberFile(new Uint8Array([1, 2, 3]), "x", password);
  const b = await site.encryptKyberFile(new Uint8Array([1, 2, 3]), "x", password);
  const magicA = Array.from(a.slice(0, 5));
  const magicB = Array.from(b.slice(0, 5));
  if (JSON.stringify(magicA) !== JSON.stringify(magicB)) ko("magic+version", `${magicA} vs ${magicB}`);
  else if (a.length !== b.length) ko("taille en-tête", `${a.length} vs ${b.length} (petit fichier)`);
  else ok("magic KYBP + version + taille de conteneur identiques");
} catch (e) {
  ko("structure", e.message || e);
}

// 4. Mauvais mot de passe rejeté par les deux
try {
  const blob = await ext.encryptKyberFile(data, "note.txt", password);
  let threw = false;
  try {
    await ext.decryptKyberFile(blob, password.slice(0, -1) + "Z");
  } catch {
    threw = true;
  }
  if (!threw) ko("ext : mauvais mdp", "aucune erreur");
  else ok("ext rejette un mauvais mot de passe");
} catch (e) {
  ko("mauvais mdp", e.message || e);
}

console.log("");
if (failures) {
  console.error(`${failures} test(s) en échec.`);
  process.exit(1);
}
console.log("Compatibilité extension <-> site : OK");
