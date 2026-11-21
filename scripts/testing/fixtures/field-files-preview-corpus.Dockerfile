# Test-only overlay on the exact verified worker; never use as a runtime image.
ARG ERPX_FILES_BUILD_IMAGE
ARG ERPX_FILES_WORKER_IMAGE
FROM ${ERPX_FILES_BUILD_IMAGE} AS corpus-build
ARG ERPX_SOURCE_KEY
ARG TARGETARCH
COPY rust-preview-provider.test.rs /workspace/erpx/tests/provider/files/rust-preview-provider.test.rs
RUN test "$(sed -n 1p /out/build-identity)" = "$ERPX_SOURCE_KEY" && test "$(sed -n 2p /out/build-identity)" = "$TARGETARCH" \
    && RUSTFLAGS='-C link-arg=-Wl,-rpath,/usr/local/lib' cargo test --locked --release -p erpx-server-files --features preview-provider --test preview_provider --no-run \
    && set -- target/release/deps/preview_provider-* \
    && for f do if test -f "$f" && test -x "$f"; then install -D -m 0755 "$f" /out/preview-provider-test; fi; done \
    && test -x /out/preview-provider-test
FROM ${ERPX_FILES_WORKER_IMAGE} AS corpus-worker
FROM corpus-worker
ARG ERPX_SOURCE_KEY
ARG TARGETARCH
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"
COPY --from=corpus-build /out/preview-provider-test /usr/local/bin/preview-provider-test
COPY --from=corpus-build /workspace/erpx/fixtures/files/preview /workspace/erpx/fixtures/files/preview
COPY --from=corpus-build /out/manifest-path /workspace/erpx/packages/server-files
COPY --from=corpus-worker /usr/local/bin/erpx-render-preview /workspace/erpx/target/release/erpx-render-preview
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/preview-provider-test"]
