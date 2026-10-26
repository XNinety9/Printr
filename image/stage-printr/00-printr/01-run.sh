#!/bin/bash -e
# shellcheck disable=SC2154  # ROOTFS_DIR, FIRST_USER_NAME: provided by pi-gen
# Printr in the image: program, service, USB printer, first network connection through a hotspot.
# Files come from files/ (image/build.sh puts the program and Barnum there).

F=files
install -d -m 2775 "${ROOTFS_DIR}/opt/printr"
install -m 775 "$F/printr" "${ROOTFS_DIR}/opt/printr/printr"
install -m 755 "$F/printr-update-guard.sh" "${ROOTFS_DIR}/opt/printr/printr-update-guard.sh"
ln -sfn /opt/printr/printr "${ROOTFS_DIR}/usr/local/bin/printr"
rm -rf "${ROOTFS_DIR}/opt/barnum"
cp -a "$F/barnum" "${ROOTFS_DIR}/opt/barnum"

# Configuration: only what is specific to the image; everything else is set in the app.
install -m 660 "$F/printr.env" "${ROOTFS_DIR}/etc/printr.env"

# Service, and its drop-in for the image (port 80, first network connection).
install -m 644 "$F/printr.service" "${ROOTFS_DIR}/etc/systemd/system/printr.service"
install -D -m 644 "$F/image.conf" "${ROOTFS_DIR}/etc/systemd/system/printr.service.d/image.conf"

# USB printer: access for the lp group, /dev/tm88 link, usblp driver.
install -m 644 "$F/70-tm-t88v.rules" "${ROOTFS_DIR}/etc/udev/rules.d/70-tm-t88v.rules"
printf 'usblp\n' > "${ROOTFS_DIR}/etc/modules-load.d/usblp.conf"

# polkit: the printr group may restart the service, the service itself may manage the network.
install -m 644 "$F/50-printr.rules" "${ROOTFS_DIR}/etc/polkit-1/rules.d/50-printr.rules"
install -m 644 "$F/51-printr-network.rules" "${ROOTFS_DIR}/etc/polkit-1/rules.d/51-printr-network.rules"

# Hotspot (hostapd and dnsmasq), started by printr when there is no network at boot.
install -m 644 "$F/printr-hotspot.service" "${ROOTFS_DIR}/etc/systemd/system/printr-hotspot.service"
install -D -m 644 "$F/hotspot-dnsmasq.conf" "${ROOTFS_DIR}/etc/printr/hotspot-dnsmasq.conf"

on_chroot <<- CHROOT
	getent group printr >/dev/null || groupadd --system printr
	getent group lp >/dev/null || groupadd --system lp
	id printr >/dev/null 2>&1 || useradd --system --gid printr --groups lp --home-dir /var/lib/printr --no-create-home --shell /usr/sbin/nologin printr
	usermod -aG printr,lp "${FIRST_USER_NAME}" 2>/dev/null || true
	chown root:printr /opt/printr /opt/printr/printr /opt/printr/printr-update-guard.sh /etc/printr.env
	systemctl enable printr avahi-daemon
	systemctl disable hostapd 2>/dev/null || true
CHROOT
