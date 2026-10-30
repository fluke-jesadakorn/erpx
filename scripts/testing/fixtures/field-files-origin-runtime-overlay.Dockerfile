# Verification-only assembly: reuse the already compiled runtime binaries and
# replace only libvips with the freshly provisioned relative-RUNPATH library.
ARG ERPX_PROVISION_IMAGE
ARG ERPX_API_IMAGE
ARG ERPX_WORKER_IMAGE
FROM ${ERPX_PROVISION_IMAGE} AS provision
FROM ${ERPX_API_IMAGE} AS api
ARG ERPX_SOURCE_KEY
ARG TARGETARCH
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"
COPY --from=provision /cache/erpx-native-vips-8.18.7/lib/libvips.so.42.20.7 /usr/local/lib/libvips.so.42
FROM ${ERPX_WORKER_IMAGE} AS worker
ARG ERPX_SOURCE_KEY
ARG TARGETARCH
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"
COPY --from=provision /cache/erpx-native-vips-8.18.7/lib/libvips.so.42.20.7 /usr/local/lib/libvips.so.42
