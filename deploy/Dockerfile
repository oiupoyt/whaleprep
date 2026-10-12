# WhalePrep — single static binary + SQLite file, no other services.
# Build:  docker build -f deploy/Dockerfile -t whaleprep .
# Run:    docker run -p 8080:8080 -v whaleprep-data:/data \
#           -e PYQ_DB=/data/whaleprep.db whaleprep
# Google: add -e GOOGLE_CLIENT_ID=... -e GOOGLE_CLIENT_SECRET=...
FROM rust:1.99-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libsqlite3-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
COPY content ./content
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    libsqlite3-0 ca-certificates curl && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 10001 app
USER app
WORKDIR /app
COPY --from=build /app/target/release/whaleprep ./whaleprep
COPY --from=build /app/static ./static
COPY --from=build /app/content ./content
VOLUME /data
ENV PYQ_DB=/data/whaleprep.db PYQ_ADDR=0.0.0.0:8080 RUST_LOG=info
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
  CMD curl -fsS http://127.0.0.1:8080/ || exit 1
CMD ["./whaleprep"]
