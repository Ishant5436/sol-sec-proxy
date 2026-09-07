# Build stage
FROM rust:1.80-slim AS builder

WORKDIR /usr/src/sol-sec-proxy
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > benches/proxy_benchmark.rs && \
    cargo build --release && \
    rm -rf src benches

# Copy actual source code
COPY src ./src
COPY benches ./benches

# Build release binary
RUN touch src/main.rs src/lib.rs && cargo build --release --bin sol-sec-proxy

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

# Create unprivileged user
RUN groupadd -g 10001 proxy && useradd -u 10001 -g proxy -s /bin/false proxyuser

WORKDIR /app
COPY --from=builder /usr/src/sol-sec-proxy/target/release/sol-sec-proxy /app/sol-sec-proxy

USER proxyuser:proxy

EXPOSE 8899
ENV PROXY_HOST=0.0.0.0
ENV PROXY_PORT=8899
ENV CU_SAFETY_BUFFER_PERCENT=15
ENV MAX_CU_LIMIT=1400000
ENV RPC_TIMEOUT_MS=5000

ENTRYPOINT ["/app/sol-sec-proxy"]
