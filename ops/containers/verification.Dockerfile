# Local verification prerequisites, layered on the separately built pinned dev image.
# Build and browser/source provisioning require explicit coordination and disk budget.
FROM postgres:18.6-trixie@sha256:979bccfde35445db618c65107d27d08c6c21339d8dc3ad4c830c6633947be0e4 AS postgres
FROM erpx-opensource-dev-toolchain:local

COPY --from=postgres /usr/lib/postgresql/18 /usr/lib/postgresql/18
COPY --from=postgres /usr/lib/*/libpq.so.5.18 /usr/local/lib/
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential binutils pkg-config python3-venv libssl-dev \
    libglib2.0-dev libexpat1-dev libjpeg-dev libpng-dev libwebp-dev \
    libheif-dev libexif-dev liblcms2-dev zlib1g-dev \
    libpq5 libheif-examples libheif-plugin-libde265 poppler-utils age \
    ca-certificates fonts-liberation libasound2t64 libatk-bridge2.0-0t64 \
    libatk1.0-0t64 libatspi2.0-0t64 libcairo2 libcups2t64 libdbus-1-3 \
    libdrm2 libgbm1 libnspr4 libnss3 libpango-1.0-0 libx11-6 libxcb1 \
    libxcomposite1 libxdamage1 libxext6 libxfixes3 libxkbcommon0 libxrandr2 \
    && rm -rf /var/lib/apt/lists/* \
    && LD_LIBRARY_PATH=/usr/local/lib /usr/lib/postgresql/18/bin/psql --version | grep -q '^psql (PostgreSQL) 18\.6' \
    && pkg-config --exists glib-2.0 gio-2.0 gobject-2.0 expat libjpeg libpng libwebp libwebpmux libwebpdemux libheif libexif lcms2 zlib

ENV LD_LIBRARY_PATH=/usr/local/lib \
    ERPX_TEST_PG_BIN=/usr/lib/postgresql/18/bin \
    ERPX_HEIF_ENC=/usr/bin/heif-enc \
    ERPX_HEIF_CONVERT=/usr/bin/heif-convert \
    ERPX_PDFTOPPM_BIN=/usr/bin/pdftoppm
