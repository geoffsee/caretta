FROM rust:bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libayatana-appindicator3-dev \
    libxdo-dev \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/root/.cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --features bundle-runtime \
    && mkdir -p /app/bin \
    && cp target/release/freq-ai /app/bin/
ENTRYPOINT ["/app/bin/freq-ai"]
