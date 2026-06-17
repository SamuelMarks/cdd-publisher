FROM rust:1.80-alpine as builder
WORKDIR /app
RUN apk add --no-cache musl-dev pkgconfig openssl-dev
COPY . .
RUN cargo build --release

FROM alpine:latest
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/cdd-publisher /usr/local/bin/cdd-publisher
ENTRYPOINT ["cdd-publisher"]
