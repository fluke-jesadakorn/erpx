# Test-only seed overlay: the seed and migrations come from the reviewed base;
# the exact final API server binary supplies the qualification identity check.
ARG ERPX_FILES_SEED_IMAGE
ARG ERPX_FILES_API_IMAGE
FROM ${ERPX_FILES_API_IMAGE} AS runtime
FROM ${ERPX_FILES_SEED_IMAGE}
ARG ERPX_SOURCE_KEY
ARG TARGETARCH
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"
COPY --from=runtime /usr/local/bin/erpx-server /usr/local/bin/erpx-server
