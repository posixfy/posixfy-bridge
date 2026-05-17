# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/posixfy-bridge /usr/local/bin/posixfy-bridge

# Bridge must run as root for setfsuid/setfsgid to switch file operation identity.
# Access is protected by API_KEY authentication.

EXPOSE 3000

ENV LISTEN_ADDR=0.0.0.0:3000
ENV RUST_LOG=info

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

ENTRYPOINT ["posixfy-bridge"]
