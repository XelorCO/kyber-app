# Politique de sécurité

## Signaler une vulnérabilité

**N'ouvrez pas d'issue publique pour une faille de sécurité.**

Écrivez à **contact@kyber-security.fr** avec :

- une description du problème et de son impact ;
- les étapes de reproduction (ou un PoC) ;
- la version de Kyber et le système d'exploitation concernés.

Vous recevrez un accusé de réception sous quelques jours. Une fois le correctif
prêt et diffusé, votre contribution sera créditée dans le [CHANGELOG](CHANGELOG.md)
si vous le souhaitez.

## Versions supportées

Seule la dernière version publiée reçoit des correctifs de sécurité. L'updater
intégré maintient les installations à jour automatiquement.

## Modèle de menace

Kyber protège vos secrets **au repos** : quelqu'un qui obtient votre fichier
`.vault` ne doit rien pouvoir en tirer sans votre passphrase.

Chaîne : `passphrase → Argon2id(64 Mio, t=4, p=1) → ML-KEM-1024 → HKDF-SHA256 → AES-256-GCM`.

Propriétés :

- **Confidentialité + intégrité** via AES-256-GCM (chiffrement authentifié) ;
  toute altération du fichier est détectée.
- **Coût du bruteforce** via Argon2id (paramètres mémoire-durs).
- **Résistance quantique** portée par AES-256 (~128 bits post-Grover) et
  Argon2id.
- **Nonces** tirés d'`OsRng`, frais à chaque écriture.
- **Clés en mémoire** enveloppées dans `MasterKey` (`ZeroizeOnDrop`).

### Le rôle réel de ML-KEM-1024

La clé secrète ML-KEM est **scellée sous `seed_key = Argon2id(passphrase)`**.
Un attaquant qui casse la passphrase récupère donc aussi le secret partagé
ML-KEM : **la couche post-quantique n'ajoute pas de marge contre un bruteforce
de passphrase.** Elle apporte :

- une liaison KEM supplémentaire dans la dérivation de clé (défense en
  profondeur) ;
- la préparation du partage de clé hybride (ML-KEM + X25519).

Nous le documentons explicitement plutôt que de présenter Kyber comme
« inviolable grâce au post-quantique ».

## Limites connues (hors périmètre)

- **Appareil compromis** : un keylogger ou un malware actif capture la
  passphrase à la frappe. Kyber ne s'en protège pas.
- **Passphrase faible** : toute la sécurité en dépend. Utilisez le générateur
  de phrase secrète.
- **Perte du mot de passe maître** : le coffre est **définitivement
  irrécupérable**. Aucune porte dérobée, aucune récupération par email — c'est
  un choix de conception.
- **Extension navigateur** : le presse-papier n'est pas auto-effacé après une
  copie ; la correspondance de domaine est indicative et ne bloque pas le
  remplissage sur un autre site. Voir [`extension/README.md`](extension/README.md).
- **Binaire du native host non signé** (contrairement à l'app principale) :
  même niveau de confiance qu'un exécutable local classique.
- **Métadonnées** : la taille du fichier `.vault` et sa date de modification
  ne sont pas masquées.

## Ce qui a déjà été audité (auto-audits)

- Chaîne crypto (AES-256-GCM + Argon2id + ML-KEM), fraîcheur des nonces,
  zéroïsation — voir les 7 tests de `crypto.rs`.
- `filelock.rs` : protection zip-slip, limite anti zip-bomb.
- `session_bridge.rs` : jeton en temps constant, lectures bornées,
  nettoyage du fichier de session, permissions 0600 — 4 tests unitaires.
- Frontend : `escapeHtml` sur tous les attributs HTML dynamiques, aucune
  police externe (pas de fuite d'IP, offline strict).

Un audit externe indépendant reste souhaitable et bienvenu.
