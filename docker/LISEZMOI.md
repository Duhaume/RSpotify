# Raspotify dans Docker — tout sur le Pi

Le Raspberry Pi devient une enceinte Spotify Connect. Tout tourne sur le Pi,
dans trois conteneurs Docker :

| Conteneur | Rôle |
|---|---|
| `raspotify` | le lecteur Spotify Connect (le même programme que raspotify) |
| `raspotify-hotspot` | le réseau Wi-Fi créé par le Pi (en option, éteint par défaut) |
| `raspotify-reglages` | la page de réglages, ouverte depuis n'importe quel navigateur |

## Installer

Sur le Pi, dans ce dossier :

```sh
sudo ./install.sh
```

Docker est installé s'il manque, les conteneurs sont construits et démarrés,
puis l'adresse de la page de réglages et son code s'affichent, par exemple
`http://raspberrypi.local:8080`. Le navigateur demande le code (nom
d'utilisateur : n'importe lequel).

Autres commandes : `sudo ./install.sh logs`, `sudo ./install.sh stop`,
`sudo ./install.sh remove`.

## Compiler les images

- **Sur le Pi** : `docker compose build` (l'installation le fait aussi toute seule).
- **Sur un ordinateur, pour le Pi** : `./build-images.sh --pi` (ajoutez `--32bits`
  pour un système 32 bits) crée `raspotify-images-arm64.tar.gz`. Sur le Pi :
  `gunzip -c raspotify-images-arm64.tar.gz | docker load`, puis
  `sudo ./install.sh images` (ou `docker compose -f docker-compose.images.yml up -d`).
- **Compiler et envoyer directement** : `./build-images.sh --pi --envoyer pi@raspberrypi.local`.

Deux fichiers docker compose : `docker-compose.yml` compile les images sur le Pi,
`docker-compose.images.yml` utilise des images déjà chargées.

## Régler

Ouvrez la page de réglages depuis un téléphone ou un ordinateur du même réseau :
nom de l'enceinte, qualité, volume de départ, point d'accès Wi-Fi (nom, mot de
passe, canal…). « Enregistrer et appliquer » redémarre ce qu'il faut.

## Le point d'accès Wi-Fi (option)

Activé, le Pi crée son propre réseau ; les téléphones s'y connectent et voient
l'enceinte, et la page de réglages est alors sur `http://192.168.50.1:8080`.
Spotify a besoin d'internet : reliez le Pi par câble (eth0) ou par une deuxième
clé Wi-Fi. La carte qui crée le réseau ne peut pas rester connectée à votre box.

## Fichiers

| Fichier | Rôle |
|---|---|
| `raspotify.conf` | réglages du lecteur (mêmes noms que `/etc/raspotify/conf`) |
| `hotspot.env` | réglages du point d'accès (`AP_ENABLED=yes` pour l'allumer) |
| `.env` | port et code de la page de réglages |
| `docker-compose.yml` | les trois conteneurs, compilés sur le Pi |
| `docker-compose.images.yml` | les mêmes, avec des images déjà compilées |
| `build-images.sh` | compilation des images (sur le Pi ou pour le Pi) |
| `install.sh` | installation en une commande |
