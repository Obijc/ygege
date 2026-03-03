<p align="center">
  <img src="website/img/ygege-logo-text.png" alt="Logo Ygégé" width="400"/>
</p>

Indexeur haute performance pour YGG Torrent écrit en Rust

## [AVERTISSEMENT LÉGAL](DISCLAIMER-fr.md)


> **🤖 Fork "Vibe Code" — Intégration FlareSolverr**
>
> Ce fork a été modifié par **vibe coding** (assisté par IA) pour intégrer [FlareSolverr](https://github.com/FlareSolverr/FlareSolverr) comme **mécanisme de fallback** contre les challenges Cloudflare.
>
> Quand `wreq` seul n'arrive plus à passer le challenge CF (erreur 403, pas de cookie `ygg_`), ygege délègue automatiquement la résolution à un conteneur FlareSolverr, récupère les cookies de bypass, et les réinjecte dans le client `wreq` pour poursuivre le login normalement.
>
> **Fichiers modifiés :** `src/flaresolverr.rs` *(nouveau)*, `src/config.rs`, `src/auth.rs`, `src/main.rs`, `docker/compose.yml`

---

## Caractéristiques principales

- ⚡ Recherche quasi instantanée
- 🔒 Bypass Cloudflare automatisé via émulation TLS/HTTP2 ([wreq](https://crates.io/crates/wreq))
- 🛡️ **Fallback FlareSolverr** optionnel si le bypass `wreq` échoue *(nouveau)*
- 🔄 Résolution automatique du domaine actuel de YGG Torrent
- 🔁 Reconnexion transparente aux sessions expirées + cache de sessions
- 🌐 Contournement des DNS menteurs (fallback Cloudflare DNS)
- 💾 Consommation mémoire faible (~15 Mo en release sur Linux)
- 🔍 Recherche modulaire (nom, seed, leech, commentaires, date, etc.)
- 📦 Aucune dépendance externe, aucun driver de navigateur

---

### 1. Utiliser l'image Docker (GHCR)

L'image officielle est disponible sur le GitHub Container Registry :

```bash
docker pull ghcr.io/obijc/ygege:latest
```

*Note : Pour compiler l'image vous-même, consultez le **[Guide Docker complet](docs/build-docker-linux.md)**.*

### 2. Configurer et lancer

Éditez `docker/compose.yml` pour renseigner vos identifiants YGG et optionnellement activer FlareSolverr :

```yaml
services:
  ygege:
    image: ghcr.io/obijc/ygege:latest
    environment:
      YGG_USERNAME: "votre_username"
      YGG_PASSWORD: "votre_password"
      # FLARESOLVERR_URL: "http://flaresolverr:8191"   # Décommenter pour activer

  # Décommenter le bloc ci-dessous pour activer FlareSolverr :
  # flaresolverr:
  #   image: ghcr.io/flaresolverr/flaresolverr:latest
  #   ports:
  #     - "8191:8191"
```

```bash
cd docker
docker compose up -d
```

---

## Intégration FlareSolverr (fallback Cloudflare)

FlareSolverr est un service qui résout les challenges Cloudflare via un vrai navigateur headless. Ygege l'utilise **uniquement en fallback** : si le bypass `wreq` classique échoue, ygege envoie une requête à FlareSolverr, récupère les cookies de résolution, les injecte dans son client HTTP, et reprend le flow normal.

### Activer FlareSolverr

1. Dans `docker/compose.yml`, décommentez le service `flaresolverr`
2. Décommentez la ligne `FLARESOLVERR_URL` dans les env vars de `ygege`
3. Relancez : `docker compose up -d`

| Variable d'environnement | Description | Exemple |
|---|---|---|
| `FLARESOLVERR_URL` | URL du service FlareSolverr (optionnel) | `http://flaresolverr:8191` |

---

## Configuration

| Variable | Description | Défaut |
|---|---|---|
| `YGG_USERNAME` | Identifiant YGG *(obligatoire)* | — |
| `YGG_PASSWORD` | Mot de passe YGG *(obligatoire)* | — |
| `BIND_IP` | IP d'écoute | `0.0.0.0` |
| `BIND_PORT` | Port d'écoute | `8715` |
| `LOG_LEVEL` | Niveau de log (`off`, `error`, `warn`, `info`, `debug`, `trace`) | `debug` |
| `TMDB_TOKEN` | Token API TMDB (optionnel, pour recherche TMDB/IMDB) | — |
| `YGG_DOMAIN` | Forcer un domaine YGG spécifique | auto-détecté |
| `TURBO_ENABLED` | Mode turbo | `false` |
| `FLARESOLVERR_URL` | URL FlareSolverr (optionnel) | — |

---

## Intégration Prowlarr / Jackett

### Prowlarr

Copiez `ygege.yml` dans `{appdata prowlarr}/Definitions/Custom/`, puis redémarrez Prowlarr.

> [!NOTE]
> URL par défaut : `http://localhost:8715/`. En Docker Compose : `http://ygege:8715/`.

### Jackett

Copiez `ygege.yml` dans `{appdata jackett}/cardigann/definitions/`, puis redémarrez Jackett.

---

## Contournement Cloudflare — Comment ça marche

1. **Cookie magique** : Ygege injecte `account_created=true` pour désactiver le challenge CF initial
2. **Émulation TLS/HTTP2** : Via [wreq](https://crates.io/crates/wreq), reproduction fidèle du fingerprint Chrome 132
3. **Fallback FlareSolverr** *(nouveau)* : Si le cookie `ygg_` n'est pas obtenu, FlareSolverr résout le challenge via un vrai navigateur headless

> [!WARNING]
> L'émulation `wreq` ne fonctionne plus à partir de Chrome 133+ (HTTP/3). C'est la raison d'être de l'intégration FlareSolverr.

Articles recommandés :
- [TLS Fingerprinting](https://fingerprint.com/blog/what-is-tls-fingerprinting-transport-layer-security/)
- [HTTP/2 Fingerprinting](https://www.trickster.dev/post/understanding-http2-fingerprinting/)

---

## Prérequis pour la compilation locale

- Rust 1.85.0+
- OpenSSL 3+
- Dépendances de [wreq](https://crates.io/crates/wreq)

Ou utilisez simplement Docker : voir le [Guide Docker](docs/build-docker-linux.md).

---

## Documentation

- [Guide Docker complet (compilation + utilisation)](docs/build-docker-linux.md)
- [Configuration](https://ygege.lila.ws/configuration)
- [API](https://ygege.lila.ws/api)
- [FAQ](https://ygege.lila.ws/faq)
- [Guide de contribution](docs/contribution-fr.md)
