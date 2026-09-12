# Changelog

Toutes les évolutions notables de Kyber. Format inspiré de
[Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versionnage
[SemVer](https://semver.org/lang/fr/).

## [2.0.0] — non publié

### Changé — Kyber passe en open source

- **Licence Apache-2.0.** Le code de l'application est public.
- **Suppression complète du système de licence** (`license.rs`, vérification
  Ed25519, activation de clé). Un éventuel `~/.kyber/license.key` est désormais
  ignoré ; il peut être supprimé sans effet.
- **Suppression de toutes les limites freemium** : mots de passe illimités,
  coffres illimités, export CSV et extension navigateur pour tout le monde.
- Extension : l'écran « licence requise » disparaît ; le chiffrement de
  fichiers autonome (format `KYBP`, par mot de passe) est ajouté au popup.
- Dépendances retirées (inutilisées après le retrait des licences) :
  `ed25519-dalek`, `base64`, `pqcrypto-dilithium`.
- Documentation de dépôt : `LICENSE`, `NOTICE`, `README`, `ARCHITECTURE.md`,
  `SECURITY.md`, `CONTRIBUTING.md`, CI GitHub, commentaires rustdoc sur les
  modules.

### Note

Les coffres `.vault` (v1 et v2) et les fichiers `.kyber` existants sont
inchangés et restent lisibles.

## [1.3.0] — 2026-08-31

### Ajouté
- Rappel de rotation des mots de passe (seuil 30 jours), activable par coffre,
  avec régénération en un clic.
- Extension : remplissage multi-champs pour les formulaires de changement de
  mot de passe (épargne le champ « mot de passe actuel »).

### Sécurité
- Durcissement `session_bridge` / `native_host` : lectures réseau bornées,
  comparaison de jeton en temps constant, nettoyage de `session.json`,
  permissions 0600 (Unix), zéroïsation des clés intermédiaires.
- `filelock` : limite anti zip-bomb.

## [1.2.0] — 2026-07-06

### Ajouté
- Thème sombre graphite, nouveau logo, interface sans emoji.
- Popup de mise à jour au démarrage.
- Extension navigateur compagnon (beta) : native messaging + pont de session
  live (récupère le coffre déjà ouvert dans l'app sans redemander le mot de
  passe) + verrouillage manuel.

### Performance
- Rendu du coffre optimisé (un seul `innerHTML`, délégation d'évènements,
  recherche debouncée) ; profil release LTO.

## [1.1.0] — 2026-06

### Ajouté
- Mises à jour automatiques signées (`tauri-plugin-updater`, minisign).
- Migration assistée des coffres v1 → v2 (intégration ML-KEM-1024).

## [1.0.0] — 2026-06

Première version : coffre `.vault` chiffré (Argon2id + ML-KEM-1024 + HKDF +
AES-256-GCM), générateur, analyse de sécurité, auto-remplissage natif,
chiffrement de fichiers et dossiers (`.kyber`), import/export CSV.
