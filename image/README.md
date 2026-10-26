# Raspberry Pi image

Raspberry Pi OS Lite (64-bit) with Printr ready to use, for the Pi 3, 4, 5 and Zero 2 W, built with
[pi-gen](https://github.com/RPi-Distro/pi-gen). It is attached to every release
(`printr-X.Y.Z-raspberrypi-arm64.img.xz`).

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
| `51-printr-network.rules` | polkit: the service may drive NetworkManager (hotspot, Wi-Fi) |
| `printr-portal.conf` | Hotspot dnsmasq: every name leads to Printr (captive portal) |
| `build.sh` | Puts it all together and runs pi-gen in Docker |

## Building

```bash
bash image/build.sh                      # builds printr with cross, then the image
bash image/build.sh --binary path/to/printr --out directory
```

Docker is required and, on an x86 PC, ARM emulation (`qemu-user-static`). Allow from 20 minutes to
over an hour. The result: `target/image/printr-X.Y.Z-raspberrypi-arm64.img.xz`.
