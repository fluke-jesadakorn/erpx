# syntax=docker/dockerfile:1.7
# One compiled modular-monolith binary serves both private API and worker.
FROM rust:1.99.1-trixie@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137 AS build
ARG TARGETARCH
ARG ERPX_SOURCE_KEY
WORKDIR /workspace/erpx
COPY . .
RUN test -n "$ERPX_SOURCE_KEY" && test -n "$TARGETARCH" && cargo build --locked --release -p erpx-service --no-default-features --features private-field-api --bin erpx-server \
    && install -D -m 0755 target/release/erpx-server /out/erpx-server
RUN printf "%s\n%s\n" "$ERPX_SOURCE_KEY" "$TARGETARCH" > /out/build-identity && mkdir -p /out/manifest-path \
    && find migrations/postgres -type d -exec chmod 0755 {} + \
    && find migrations/postgres -type f -exec chmod 0644 {} +
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:3ee00291cf386ec52a552e6a858d555debd74776eb4bfa53fa3df2cd6677515e AS runtime
ARG TARGETARCH
ARG ERPX_SOURCE_KEY
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"
WORKDIR /workspace/erpx
COPY --from=build /out/erpx-server /usr/local/bin/erpx-server
# The binary resolves its migration inventory relative to CARGO_MANIFEST_DIR.
COPY --from=build /workspace/erpx/migrations/postgres /workspace/erpx/migrations/postgres
COPY --from=build /out/manifest-path /workspace/erpx/src/composition
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/erpx-server"]

