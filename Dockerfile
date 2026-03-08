# ─── Stage 1 : build ──────────────────────────────────────────────────────
FROM rust:1.87-slim AS builder

# Dépendances système pour sqlx / OpenSSL
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache des dépendances Cargo (layer séparé)
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main(){}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs

# Build réel
COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release

# ─── Stage 2 : image finale ───────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/chetaku /usr/local/bin/chetaku
COPY --from=builder /app/migrations /migrations

EXPOSE 8080
ENV PORT=8080

CMD ["chetaku"]
