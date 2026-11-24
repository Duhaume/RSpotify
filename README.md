# RSpotify

Une enceinte **Spotify Connect** sur Raspberry Pi, entièrement dans **Docker**,
avec en option un **réseau Wi-Fi créé par le Pi** et une **page de réglages**
ouverte depuis n'importe quel navigateur.

Inspiré de [raspotify](https://github.com/dtcooper/raspotify) : même lecteur
([librespot](https://github.com/librespot-org/librespot)), mêmes réglages par
défaut, même règle de redémarrage, mais rangé dans des conteneurs.

## Ce qui tourne sur le Pi

| Conteneur | Rôle |
|---|---|
| `raspotify` | le lecteur Spotify Connect (paquet raspotify, avec Avahi pour être vu par Spotify) |
| `raspotify-hotspot` | le point d'accès Wi-Fi (hostapd + dnsmasq), en veille tant qu'il n'est pas activé |
| `raspotify-reglages` | la page de réglages, sur `http://<nom-du-pi>.local:8080` |

## Installation rapide (sur le Pi)

Il faut Raspberry Pi OS (64 bits conseillé), un accès SSH et une enceinte
branchée (jack ou carte son USB). Si raspotify est déjà installé, retirez-le
d'abord (`sudo apt remove raspotify`), sinon deux enceintes apparaissent dans Spotify.

```sh
git clone https://github.com/<votre-compte>/RSpotify.git
cd RSpotify/docker
sudo ./install.sh
```

Le script installe Docker si besoin, compile les images, démarre les conteneurs
puis affiche l'adresse de la page de réglages et son code d'accès. Dans
l'application Spotify, choisissez ensuite l'enceinte dans « Appareils ».

Autres commandes : `sudo ./install.sh logs`, `sudo ./install.sh stop`,
`sudo ./install.sh remove`.

## Régler

Ouvrez `http://<nom-du-pi>.local:8080` depuis un ordinateur ou un téléphone du
même réseau. Le navigateur demande le code affiché à l'installation (nom
d'utilisateur : n'importe lequel). Vous y changez le nom de l'enceinte, la
qualité, le volume de départ, la normalisation et le point d'accès Wi-Fi ;
« Enregistrer et appliquer » redémarre ce qu'il faut.

Les mêmes réglages sont dans des fichiers texte du dossier `docker/` :
`raspotify.conf` (lecteur, mêmes noms que `/etc/raspotify/conf`), `hotspot.env`
(point d'accès) et `.env` (port et code de la page).

## Le point d'accès Wi-Fi (option)

Activé, le Pi crée son propre réseau ; la page de réglages y est sur
`http://192.168.50.1:8080`. Spotify ayant besoin d'internet, reliez le Pi par
câble (eth0) ou par une deuxième clé Wi-Fi : la carte qui crée le réseau ne peut
pas rester connectée à votre box en même temps.

## Compiler les images

- **Sur le Pi** : `docker compose build` (l'installation le fait toute seule).
- **Sur un ordinateur, pour le Pi** : `./build-images.sh --pi` (ajoutez
  `--32bits` pour un système 32 bits) crée `raspotify-images-arm64.tar.gz`.
  Sur le Pi : `gunzip -c raspotify-images-arm64.tar.gz | docker load` puis
  `sudo ./install.sh images`.
- **Compiler et envoyer directement** :
  `./build-images.sh --pi --envoyer pi@raspberrypi.local`.

La première compilation pour le Pi sur un ordinateur demande l'émulation QEMU
(`docker run --privileged --rm tonistiigi/binfmt --install all`) ; le script
propose de l'installer.

## Les deux fichiers docker compose

| Fichier | Quand l'utiliser |
|---|---|
| `docker/docker-compose.yml` | le Pi compile les images : `docker compose up -d --build` |
| `docker/docker-compose.images.yml` | images déjà chargées : `docker compose -f docker-compose.images.yml up -d` |

## L'application de configuration (facultative)

Une application de bureau (Rust, eframe/egui) prépare les mêmes fichiers avec
une interface en français, compile les images et peut les envoyer au Pi par SSH.

```sh
cargo run --release
cargo run --release -- --export ~/raspotify-docker   # écrit les fichiers sans ouvrir la fenêtre
```

## Organisation du dépôt

| Chemin | Contenu |
|---|---|
| `docker/` | Dockerfiles, scripts de démarrage, page de réglages, fichiers compose, `install.sh`, `build-images.sh` |
| `src/` | l'application de configuration |
| `MECHANICS.md`, `mechanics.json` | les réglages et règles de raspotify repris ici, avec leur origine |

## Attention avant de publier

Les fichiers `docker/.env`, `docker/hotspot.env` et `docker/raspotify.conf` du
dépôt sont des modèles sans secret. Une fois remplis sur le Pi (code de la page
de réglages, mot de passe Wi-Fi, jeton d'accès), **ne les renvoyez pas sur
GitHub**. Si besoin : `git update-index --assume-unchanged docker/.env docker/hotspot.env docker/raspotify.conf`.

## Licence et remerciements

Sous licence MIT (voir `LICENSE`). Ce dépôt ne contient pas le programme de
raspotify : l'image le télécharge à la compilation. Merci à
[raspotify](https://github.com/dtcooper/raspotify) et
[librespot](https://github.com/librespot-org/librespot), tous deux sous
licence MIT. Spotify est une marque de Spotify AB ; ce projet n'y est pas lié.
