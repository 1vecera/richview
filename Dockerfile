# Build from the repository root. This creates local artifacts; it does not deploy.
FROM rust:1.99.0-bookworm AS build
WORKDIR /src
RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.129 --locked
COPY . .
RUN cargo run --release --locked -p richview-build

# The binary carries its own report runtime (ARC-07); the container needs nothing else.
FROM debian:bookworm-slim
RUN groupadd --gid 10001 richview && useradd --uid 10001 --gid 10001 --no-create-home richview \
    && mkdir /data && chown richview:richview /data
COPY --from=build /src/target/release/richview /usr/local/bin/richview
WORKDIR /data
ENV RICHVIEW_BIND=0.0.0.0 RICHVIEW_DATA_DIR=/data PORT=8080
USER richview
EXPOSE 8080
CMD ["richview", "server"]
