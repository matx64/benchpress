FROM rust:1.99.1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build -p benchpress-server --release --locked && cp target/release/benchpress-server /benchpress-server

FROM scratch
COPY --from=build /benchpress-server /benchpress-server
ENV BENCHPRESS_DB=/data/benchpress.db \
    BENCHPRESS_BIND=0.0.0.0:8080 \
    BENCHPRESS_LOG_FORMAT=json
EXPOSE 8080
ENTRYPOINT ["/benchpress-server"]
