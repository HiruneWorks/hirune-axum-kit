FROM --platform=$BUILDPLATFORM rust:1.99.1-bookworm AS build
ARG TARGETARCH
RUN apt-get update && apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu g++-aarch64-linux-gnu \
    && rm -rf /var/lib/apt/lists/*
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++ \
    AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar \
    CARGO_BUILD_JOBS=1
WORKDIR /source
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/source/target \
    case "$TARGETARCH" in amd64) target=x86_64-unknown-linux-gnu ;; arm64) target=aarch64-unknown-linux-gnu ;; *) exit 1 ;; esac \
    && rustup target add "$target" \
    && cargo build --locked --release --target "$target" --workspace --bins \
    && install -d /artifacts \
    && install "target/$target/release/api" "target/$target/release/worker" "target/$target/release/maintenance" "target/$target/release/manage" "target/$target/release/migration" /artifacts/

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --create-home app
COPY --from=build /artifacts/ /usr/local/bin/
USER 10001:10001
WORKDIR /home/app
ENV BIND_ADDRESS=0.0.0.0:3000
EXPOSE 3000
ENTRYPOINT ["api"]
