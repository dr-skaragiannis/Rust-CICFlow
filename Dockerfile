# Multi-stage Dockerfile for CICFlowMeter Rust
FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    libpcap-dev \
    build-essential \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY benches ./benches
COPY tests ./tests
COPY examples ./examples

RUN cargo build --release

# Final runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libpcap0.8 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/cicflowmeter /usr/local/bin/cicflowmeter

WORKDIR /data
ENTRYPOINT ["/usr/local/bin/cicflowmeter"]
CMD ["--help"]
