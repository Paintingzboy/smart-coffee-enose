# Smart Coffee E-Nose — satu image berisi backend Rust + dashboard React.
# Railway mendeteksi file ini otomatis.

# ── 1. Build dashboard React ───────────────────────────────────────────
FROM node:22-alpine AS web
WORKDIR /web
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY frontend/ ./
RUN npm run build

# ── 2. Build backend Rust ──────────────────────────────────────────────
FROM rust:1-slim-bookworm AS api
RUN apt-get update && apt-get install -y --no-install-recommends build-essential cmake pkg-config && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY backend/Cargo.toml backend/Cargo.lock ./
# cache dependensi: build sekali dengan main kosong
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY backend/src ./src
RUN touch src/main.rs && cargo build --release

# ── 3. Image runtime kecil ─────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=api /app/target/release/enose-backend ./enose-backend
COPY --from=web /web/dist ./static
ENV STATIC_DIR=/app/static \
    RUST_LOG=enose_backend=info,tower_http=warn
EXPOSE 3000
CMD ["./enose-backend"]
