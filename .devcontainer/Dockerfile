# Transitional development toolchain: TypeScript services/jobs remain active.
# Manifest-list digests cover linux/amd64 and linux/arm64.
FROM node:26.11.1-trixie@sha256:32fa97f3363975684b08bf4e8a68a47c7905175cc20275b50b974bbd02aba731 AS node
FROM oven/bun:1.4.3-debian@sha256:923bedb355f7746666659f71e724998159a5c8c54bd1cb1030e1975a0698415b AS bun
FROM rust:1.99.1-trixie@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137

COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN apt-get update && apt-get install -y --no-install-recommends lsof procps libheif-examples libheif-plugin-libde265 poppler-utils build-essential binutils pkg-config python3-venv xz-utils libglib2.0-dev libexpat1-dev libjpeg-dev libpng-dev libwebp-dev libheif-dev libexif-dev liblcms2-dev zlib1g-dev \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add --toolchain 1.99.1 rustfmt clippy \
    && test "$(node --version)" = v26.11.1 \
    && test "$(bun --version)" = 1.4.3 \
    && rustc --version | grep -q '^rustc 1\.99\.1 '

WORKDIR /opt/erpx-native-build
COPY scripts/testing/provision-native-worker.ts scripts/testing/native-worker-toolchain.ts scripts/testing/native-worker-link-flags.ts scripts/testing/cargo-build-profile.ts ./scripts/testing/
COPY fixtures/files/native-toolchain/sources.json ./fixtures/files/native-toolchain/sources.json
RUN bun --no-env-file scripts/testing/provision-native-worker.ts --prefix=/opt/erpx-vips \
    && chmod -R a+rX /opt/erpx-vips \
    && printf '/opt/erpx-vips/lib\n' > /etc/ld.so.conf.d/erpx-vips.conf \
    && ldconfig \
    && PKG_CONFIG_PATH=/opt/erpx-vips/lib/pkgconfig pkg-config --modversion vips | grep -qx '8.18.7' \
    && env -u LD_LIBRARY_PATH python3 -c 'import ctypes; ctypes.CDLL("/opt/erpx-vips/lib/libvips.so.42"); assert "/opt/erpx-vips/lib/liblcms2.so" in open("/proc/self/maps").read(); assert ctypes.CDLL("/opt/erpx-vips/lib/liblcms2.so.2").cmsGetEncodedCMMversion() == 2190'

ENV ERPX_NODE_BIN=/usr/local/bin/node ERPX_BUN_BIN=/usr/local/bin/bun
WORKDIR /workspace
