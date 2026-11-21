# Test-only fixture, never included in either runtime image.
ARG ERPX_FILES_BUILD_IMAGE
FROM ${ERPX_FILES_BUILD_IMAGE} AS fixture-build
ARG ERPX_SOURCE_KEY
ARG TARGETARCH
RUN test "$(sed -n 1p /out/build-identity)" = "$ERPX_SOURCE_KEY" && test "$(sed -n 2p /out/build-identity)" = "$TARGETARCH" \
    && RUSTFLAGS='-C link-arg=-Wl,-rpath,/usr/local/lib' cargo build --locked --release -p erpx-service --no-default-features --features private-development-api --example native-field-service-development \
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
