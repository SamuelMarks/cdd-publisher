FROM rust:alpine AS builder
WORKDIR /app
RUN apk add --no-cache musl-dev
COPY . .
RUN cargo build --release

FROM alpine:latest
RUN apk add --no-cache ca-certificates nodejs npm python3 py3-twine cargo
COPY --from=builder /app/target/release/cdd-publisher /usr/local/bin/cdd-publisher
ENTRYPOINT ["cdd-publisher"]
