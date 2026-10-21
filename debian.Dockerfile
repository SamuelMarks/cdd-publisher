FROM rust:trixie AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates nodejs npm python3 twine cargo && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/cdd-publisher /usr/local/bin/cdd-publisher
ENTRYPOINT ["cdd-publisher"]
