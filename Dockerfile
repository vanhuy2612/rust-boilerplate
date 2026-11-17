# Minimal image: a fully static musl binary on top of an empty (scratch) image.
# Final image = binary + public/ only (no shell, no libc, no package manager).
#
# Build: docker build -t rust-boilerplate .
# Run:   docker run --rm -p 3000:3000 --env-file .env rust-boilerplate
#
# Behind an HTTPS-inspecting proxy (e.g. Menlo Security, Kaspersky), apk and cargo fail
# with "server certificate not trusted". Pass those root CAs (PEM bundle) as an optional
# build secret:
#   docker build --secret id=extra-ca,src=.certs/extra-ca.pem -t rust-boilerplate .
# The CA is only trusted inside the build stage; it never reaches the final image.
#
# Heap-profiling image (live /debug/memory + /debug/pprof/heap, Linux/jemalloc):
#   docker build --build-arg CARGO_FEATURES=heap-profiling --build-arg STRIP=none \
#     -t rust-boilerplate:profiling .
#   docker run --rm -p 3000:3000 -e DEBUG_ENDPOINTS=true \
#     -e _RJEM_MALLOC_CONF=prof:true,prof_active:true,lg_prof_sample:19 \
#     rust-boilerplate:profiling
# STRIP=none keeps the symbol table so heap dumps show function names (bigger binary).

# App name must match [package].name in Cargo.toml
ARG APP_NAME=rust-boilerplate

############################
# Build stage
############################
FROM rust:alpine AS build
ARG APP_NAME
# Comma-separated cargo features, e.g. "heap-profiling". Empty = default build.
ARG CARGO_FEATURES=""
# Symbol stripping for the release binary: "symbols" (smallest) or "none" (profiling).
ARG STRIP=symbols
WORKDIR /app

# Optional extra root CA (see header), appended to the CA bundle that apk and cargo use.
# It is trusted in the SAME RUN as each download step on purpose: a secret's content is
# not part of Docker's cache key, so a separate "add CA" layer cached from a build without
# the secret would be reused silently and the download would fail with a TLS error.
ARG TRUST_EXTRA_CA='if [ -s /run/secrets/extra-ca ]; then { echo; cat /run/secrets/extra-ca; } >> /etc/ssl/certs/ca-certificates.crt; fi'

# make: required to compile jemalloc (tikv-jemalloc-sys). gcc + musl are already in rust:alpine.
RUN --mount=type=secret,id=extra-ca,required=false \
    eval "$TRUST_EXTRA_CA" && \
    apk add --no-cache make || { \
      echo >&2; \
      echo "apk failed. If the log says 'server certificate not trusted', the network inspects HTTPS:" >&2; \
      echo "  docker build --secret id=extra-ca,src=.certs/extra-ca.pem -t rust-boilerplate ." >&2; \
      echo "  (or: docker compose build, which passes .certs/extra-ca.pem automatically)" >&2; \
      exit 1; \
    }

# Size-oriented release settings, applied only to the Docker build
# (local `cargo build --release` stays fast to compile):
#   - fat LTO + 1 codegen unit: removes dead code across crates, smaller & faster binary
#   - strip symbols: drops debug/symbol tables (biggest single size win)
# opt-level stays at 3 so benchmark performance is not traded for size.
# No RUSTFLAGS needed: the musl toolchain in rust:alpine already links statically
# (crt-static is on by default), and forcing it via RUSTFLAGS breaks proc-macro crates.
ENV CARGO_PROFILE_RELEASE_LTO=fat \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_STRIP=${STRIP}

RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    --mount=type=secret,id=extra-ca,required=false \
    eval "$TRUST_EXTRA_CA" && \
    cargo build --locked --release ${CARGO_FEATURES:+--features "$CARGO_FEATURES"} && \
    cp ./target/release/$APP_NAME /server

############################
# Final stage
############################
FROM scratch AS final

# The app reads "./public/images.jpeg" relative to the working directory.
WORKDIR /
COPY --from=build /server /server
COPY public /public

# scratch has no /etc/passwd, so run as a numeric non-root UID:GID.
USER 10001:10001

EXPOSE 3000
ENTRYPOINT ["/server"]
