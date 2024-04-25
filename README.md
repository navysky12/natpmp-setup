# External NATPMP Mod for QBittorrent

Specific Docker mod to allow QBittorrent to listen on some port forwarded by an exotic router.

QBittorrent supports most of the UPnP and NATPMP router by default, this is only needed for weird and exotic router
like ProtonVPN router.

## How to use

Just define these environnement variables to your QBittorrent container: 

* `DOCKER_MODS=ghcr.io/fusetim/external-natpmp-qbittorrent:latest` to enable this mod
* `NATPMP_GATEWAY_IP=X.X.X.X` to define the NATPMP Gateway to use.

## License

Licensed under MIT license.
