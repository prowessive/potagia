FROM rust:1.99.1-slim-bookworm@sha256:f2c004e3d2d8b547e8cf7ffbc29205385f4e71d23e53e4e383065db5e9f822cd AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY magia/schema.yaml ./magia/schema.yaml
COPY magia/files ./magia/files
RUN cargo build --release --locked

FROM debian:12.12-slim@sha256:d5d3f9c23164ea16f31852f95bd5959aad1c5e854332fe00f7b3a20fcc9f635c AS runtime

ARG VERSION=0.1.0
LABEL org.opencontainers.image.title="Potagia" \
      org.opencontainers.image.description="Database-driven management application" \
      org.opencontainers.image.source="https://github.com/prowessive/potagia" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.licenses="MIT"

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl gosu \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 potagia \
    && useradd --uid 10001 --gid potagia --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin potagia

COPY --from=builder --chown=10001:10001 /build/target/release/potagia /usr/local/bin/potagia
COPY --chown=0:0 --chmod=0755 docker/entrypoint.sh /usr/local/bin/potagia-entrypoint
COPY --chown=10001:10001 LICENSE /licenses/LICENSE

USER 10001:10001
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["curl", "--fail", "--silent", "--show-error", "http://127.0.0.1:3000/"]
ENTRYPOINT ["/usr/local/bin/potagia-entrypoint"]
CMD ["/usr/local/bin/potagia"]
