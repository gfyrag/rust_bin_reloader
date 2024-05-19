FROM rust:1.78 AS builder
WORKDIR /app
ARG TARGETPLATFORM
COPY build.rs .
COPY Cargo.* .
COPY src src
RUN rustup target add $([ $(echo $TARGETPLATFORM | cut -d / -f 2) = "arm64" ] && echo aarch64 || echo x86_64)-unknown-linux-musl
RUN cargo build --release --target=$([ $(echo $TARGETPLATFORM | cut -d / -f 2) = "arm64" ] && echo aarch64 || echo x86_64)-unknown-linux-musl
RUN cp /app/target/$([ $(echo $TARGETPLATFORM | cut -d / -f 2) = "arm64" ] && echo aarch64 || echo x86_64)-unknown-linux-musl/release/rust_bin_reloader /bin/rust_bin_reloader

FROM scratch
COPY --from=builder /bin/rust_bin_reloader /bin/rust_bin_reloader
ENTRYPOINT ["/bin/rust_bin_reloader"]