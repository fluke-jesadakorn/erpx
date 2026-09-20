#!/bin/sh
set -eu

# Build one pinned Samba version from Alpine's maintained APK recipe. Run in
# the isolated build VM; copy only the signed APK repository into dev/prod.
version=${SAMBA_VERSION:-4.24.2}
work=${SAMBA_BUILD_ROOT:-/var/tmp/erpx-samba-apk}

doas apk add --no-cache alpine-sdk git sudo
doas addgroup alpine abuild 2>/dev/null || true
doas chown alpine:abuild /var/cache/distfiles
doas chmod 775 /var/cache/distfiles
if [ ! -d "$work/aports/.git" ]; then
  doas mkdir -p "$work"
  doas chown "$(id -u):$(id -g)" "$work"
  git clone --depth 1 --filter=blob:none --sparse \
    https://gitlab.alpinelinux.org/alpine/aports.git "$work/aports"
  git -C "$work/aports" sparse-checkout set main/samba
fi

cd "$work/aports/main/samba"
sed -i "s/^pkgver=.*/pkgver=$version/; s/^pkgrel=.*/pkgrel=0/" APKBUILD
# Samba 4.24 already contains Alpine's historical uintptr_t compatibility
# change; retaining the old patch makes the package fail closed at prepare.
sed -i '/musl_uintptr\.patch/d' APKBUILD
if ! find "$HOME/.abuild" -name '*.rsa' -type f 2>/dev/null | grep -q .; then
  abuild-keygen -a -n
fi
key=$(find "$HOME/.abuild" -name '*.rsa.pub' -type f | head -1)
doas cp "$key" /etc/apk/keys/
abuild checksum
abuild -r

printf 'Built Samba %s APKs under %s/packages\n' "$version" "$work"
