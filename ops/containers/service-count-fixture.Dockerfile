# Test-only image: both local base tags must be built from the same reviewed
# source and architecture. The production runtime target never contains seed.
FROM erpx-service-count-build:local AS fixture-build
ARG ERPX_SOURCE_KEY
ARG TARGETARCH
RUN test "$(sed -n 1p /out/build-identity)" = "$ERPX_SOURCE_KEY" && test "$(sed -n 2p /out/build-identity)" = "$TARGETARCH" \
    && cargo build --locked --release -p erpx-service --no-default-features --features private-development-api --example native-field-service-development \
    && install -D -m 0755 target/release/examples/native-field-service-development /out/native-field-service-development
FROM erpx-local-verification:local
ARG ERPX_SOURCE_KEY
ARG TARGETARCH
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"
WORKDIR /workspace/erpx
COPY --from=fixture-build /out/erpx-server /usr/local/bin/erpx-server
COPY --from=fixture-build /out/native-field-service-development /usr/local/bin/native-field-service-development
COPY --from=fixture-build /workspace/erpx/migrations/postgres /workspace/erpx/migrations/postgres
COPY --from=fixture-build /out/manifest-path /workspace/erpx/src/composition
