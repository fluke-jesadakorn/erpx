# Transitional development toolchain: TypeScript services/jobs remain active.
# Manifest-list digests cover linux/amd64 and linux/arm64.
FROM node:26.11.1-trixie@sha256:32fa97f3363975684b08bf4e8a68a47c7905175cc20275b50b974bbd02aba731 AS node
FROM oven/bun:1.4.3-debian@sha256:923bedb355f7746666659f71e724998159a5c8c54bd1cb1030e1975a0698415b AS bun
FROM rust:1.99.1-trixie@sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137

COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN apt-get update && apt-get install -y --no-install-recommends lsof procps libheif-examples libheif-plugin-libde265 poppler-utils \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add --toolchain 1.99.1 rustfmt clippy \
    && test "$(node --version)" = v26.11.1 \
    && test "$(bun --version)" = 1.4.3 \
    && rustc --version | grep -q '^rustc 1\.99\.1 '

ENV ERPX_NODE_BIN=/usr/local/bin/node ERPX_BUN_BIN=/usr/local/bin/bun
WORKDIR /workspace
