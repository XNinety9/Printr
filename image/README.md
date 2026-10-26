# Raspberry Pi image

Raspberry Pi OS Lite (64-bit) with Printr ready to use, for the Pi 3, 4, 5 and Zero 2 W, built with
[pi-gen](https://github.com/RPi-Distro/pi-gen). It is attached to every release
(`printr-X.Y.Z-raspberrypi-arm64.img.xz`), with `printr-imager.json`, which lists it in Raspberry Pi
Imager when its content repository is set to
`https://github.com/XNinety9/Printr/releases/latest/download/printr-imager.json`.

## What it contains

- Printr in `/opt/printr`, as a systemd service (`deploy/printr.service`), on port 80:
  `http://printr.local` (avahi);
- the printer's udev rule (`/dev/tm88`) and the `usblp` driver;
- Barnum, for the offline horoscope;
- the **first network connection** (`PRINTR_WIFI_SETUP=1`): with no network a minute after boot,
  Printr opens the "Printr-XXXX" hotspot and prints a ticket to connect to it;
- an `/etc/printr.env` limited to what is specific to the image: everything else is set in the app.

No user account is enabled and SSH is off: Raspberry Pi Imager's customization (gear icon) creates
an account, enables SSH, and can also provide the Wi-Fi right away.

## Files

| File | Purpose |
|---|---|
| `config` | pi-gen settings (name, locale, time zone, Wi-Fi country, stages) |
| `stage-printr/` | Stage added after Lite: packages, files, service |
| `printr.env` | Service configuration in the image |
| `image.conf` | Service drop-in: the right to listen on port 80, and nothing else |
| `51-printr-network.rules` | polkit: the service may drive NetworkManager and start its hotspot |
| `printr-hotspot.service` | The hotspot: hostapd (plain WPA2) and dnsmasq, started by Printr |
| `hotspot-dnsmasq.conf` | Hotspot DHCP and DNS: every name leads to Printr (captive portal) |
| `build.sh` | Puts it all together and runs pi-gen in Docker |

## Building

```bash
bash image/build.sh                      # builds printr with cross, then the image
bash image/build.sh --binary path/to/printr --out directory
```

Docker is required and, on an x86 PC, ARM emulation (`qemu-user-static`). Allow from 20 minutes to
over an hour. The result: `target/image/printr-X.Y.Z-raspberrypi-arm64.img.xz`.

On GitHub, the `Image` workflow builds it on an ARM runner: the release workflow calls it and attaches the
image to the release, and it can be run by hand for any branch (the image is then a workflow artifact):

```bash
gh workflow run image.yml -f ref=version-2.0
```
