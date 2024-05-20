FROM rust:1.78 AS builder
WORKDIR /app
ARG TARGETPLATFORM
RUN rustup target add $([ $(echo $TARGETPLATFORM | cut -d / -f 2) = "arm64" ] && echo aarch64 || echo x86_64)-unknown-linux-musl

COPY build.rs .
COPY Cargo.* .
COPY src src

RUN --mount=type=cache,target=/app/target cargo build \
    --release  \
    --target=$([ $(echo $TARGETPLATFORM | cut -d / -f 2) = "arm64" ] && echo aarch64 || echo x86_64)-unknown-linux-musl
RUN --mount=type=cache,target=/app/target \
    cp /app/target/$([ $(echo $TARGETPLATFORM | cut -d / -f 2) = "arm64" ] && echo aarch64 || echo x86_64)-unknown-linux-musl/release/rust_bin_reloader /bin/rust_bin_reloader

FROM alpine
COPY --from=builder /bin/rust_bin_reloader /bin/rust_bin_reloader
ENTRYPOINT ["/bin/rust_bin_reloader"]