# syntax=docker/dockerfile:1.7
FROM oven/bun:1.4.3-debian@sha256:923bedb355f7746666659f71e724998159a5c8c54bd1cb1030e1975a0698415b AS bun
FROM rust:1.99.1-trixie@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137 AS build
ARG TARGETARCH
ARG ERPX_SOURCE_KEY
WORKDIR /workspace/erpx
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN apt-get update && apt-get install -y --no-install-recommends python3-venv pkg-config libglib2.0-dev libexpat1-dev libjpeg-dev libpng-dev libwebp-dev libheif-dev libexif-dev liblcms2-dev zlib1g-dev && rm -rf /var/lib/apt/lists/*
COPY scripts/testing/provision-native-worker.ts scripts/testing/native-worker-toolchain.ts scripts/testing/native-worker-link-flags.ts scripts/testing/cargo-build-profile.ts ./scripts/testing/
COPY fixtures/files/native-toolchain/sources.json ./fixtures/files/native-toolchain/sources.json
RUN mkdir -p /cache && bun --no-env-file scripts/testing/provision-native-worker.ts --prefix=/cache/erpx-native-vips-8.18.7
ENV PKG_CONFIG_PATH=/cache/erpx-native-vips-8.18.7/lib/pkgconfig
COPY . .
RUN test -n "$ERPX_SOURCE_KEY" && test -n "$TARGETARCH" && test "$(pkg-config --modversion vips)" = 8.18.7 && test "$(pkg-config --modversion lcms2)" = 2.19 \
    && RUSTFLAGS='-C link-arg=-Wl,-rpath,/usr/local/lib' cargo build --locked --release -p erpx-service --no-default-features --features private-field-api,files-scan-worker --bin erpx-server \
    && RUSTFLAGS='-C link-arg=-Wl,-rpath,/usr/local/lib' cargo build --locked --release -p erpx-server-files --features preview-provider --bin erpx-render-preview \
    && install -D -m 0755 target/release/erpx-server /out/erpx-server \
    && install -D -m 0755 target/release/erpx-render-preview /out/erpx-render-preview \
    && printf '%s\n%s\n' "$ERPX_SOURCE_KEY" "$TARGETARCH" > /out/build-identity \
    && mkdir -p /out/manifest-path && find migrations/postgres -type d -exec chmod 0755 {} + && find migrations/postgres -type f -exec chmod 0644 {} +
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"

FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4 AS runtime-libraries
RUN apt-get update && apt-get install -y --no-install-recommends libglib2.0-0t64 libexpat1 libjpeg62-turbo libpng16-16t64 libwebp7 libwebpmux3 libwebpdemux2 libheif1 libexif12 liblcms2-2 zlib1g libheif-examples libheif-plugin-libde265 poppler-utils ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /cache/erpx-native-vips-8.18.7/lib/libvips.so.42.20.7 /usr/local/lib/libvips.so.42
COPY --from=build /cache/erpx-native-vips-8.18.7/lib/liblcms2.so.2.0.19 /tmp/erpx-lcms2.so
COPY --from=build /cache/erpx-native-vips-8.18.7/share/licenses/lcms2/LICENSE /usr/share/licenses/lcms2/LICENSE
COPY --from=build /out/erpx-server /usr/local/bin/erpx-server
COPY --from=build /out/erpx-render-preview /usr/local/bin/erpx-render-preview
RUN target="$(ldconfig -p | awk '/liblcms2.so.2 .*=>/ {print $NF; exit}')" && test -n "$target" && cp /tmp/erpx-lcms2.so "$(readlink -f "$target")" && cmp /tmp/erpx-lcms2.so "$target" && rm /tmp/erpx-lcms2.so \
    && ldconfig && ldd /usr/local/bin/erpx-server && ldd /usr/local/bin/erpx-render-preview && ! ldd /usr/local/bin/erpx-server | grep -q 'not found' && ! ldd /usr/local/bin/erpx-render-preview | grep -q 'not found' \
    && mkdir -p /out-libs/usr/local/lib && for f in $(ldd /usr/local/bin/erpx-server | awk '/=> \/[^ ]+/ {print $3}'); do cp -L "$f" /out-libs/usr/local/lib/; done

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:3ee00291cf386ec52a552e6a858d555debd74776eb4bfa53fa3df2cd6677515e AS api
ARG TARGETARCH
ARG ERPX_SOURCE_KEY
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"
WORKDIR /workspace/erpx
COPY --from=runtime-libraries /out-libs/ /
COPY --from=runtime-libraries /usr/local/lib/libvips.so.42 /usr/local/lib/libvips.so.42
COPY --from=runtime-libraries /usr/lib/ /usr/lib/
COPY --from=runtime-libraries /usr/share/licenses/lcms2/LICENSE /usr/share/licenses/lcms2/LICENSE
COPY --from=runtime-libraries /usr/bin/heif-convert /usr/bin/heif-convert
COPY --from=runtime-libraries /usr/bin/pdftoppm /usr/bin/pdftoppm
COPY --from=runtime-libraries /usr/local/bin/erpx-render-preview /usr/local/bin/erpx-render-preview
COPY --from=build /out/erpx-server /usr/local/bin/erpx-server
COPY --from=build /workspace/erpx/migrations/postgres /workspace/erpx/migrations/postgres
COPY --from=build /out/manifest-path /workspace/erpx/src/composition
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/erpx-server"]

FROM runtime-libraries AS worker
ARG TARGETARCH
ARG ERPX_SOURCE_KEY
LABEL org.opencontainers.image.revision="$ERPX_SOURCE_KEY" org.erpx.target-architecture="$TARGETARCH"
WORKDIR /workspace/erpx
COPY --from=build /workspace/erpx/migrations/postgres /workspace/erpx/migrations/postgres
COPY --from=build /out/manifest-path /workspace/erpx/src/composition
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/erpx-server"]
