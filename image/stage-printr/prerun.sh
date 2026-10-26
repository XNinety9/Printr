#!/bin/bash -e
# shellcheck disable=SC2154  # ROOTFS_DIR: provided by pi-gen

if [ ! -d "${ROOTFS_DIR}" ]; then
	copy_previous
fi
