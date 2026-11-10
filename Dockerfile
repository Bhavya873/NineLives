# Static deploy of the Nine Lives web game (SPEC 11.7): build web/dist, serve it with nginx.
# No game server or API. Railway builds this file; the container listens on $PORT.
#
#   docker build -t ninelives .
#   docker run --rm -p 8080:8080 -e PORT=8080 ninelives

FROM rust:1-bookworm AS build
ARG NODE_MAJOR=22
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY crates crates
COPY configs configs
COPY courses courses
COPY models models
# wasm-bindgen-cli must match the wasm-bindgen crate in Cargo.lock.
# Rustup 1.28+ no longer installs toolchains on demand, so add the wasm target explicitly.
RUN rustup target add wasm32-unknown-unknown \
    && version="$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/version = "\(.*\)"/\1/p')" \
    && cargo install wasm-bindgen-cli --version "$version" --locked
COPY web web
WORKDIR /app/web
RUN npm ci && npm run wasm:release && npm run build && npm run size

FROM nginx:1.27-alpine
# nginx expands ${PORT} in templates at start-up (the official image's envsubst step).
ENV PORT=8080
COPY deploy/nginx.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /app/web/dist /usr/share/nginx/html
EXPOSE 8080
