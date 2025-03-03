FROM rust:1-slim-bookworm

RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    apt-get clean

COPY ./reverse-proxy /src

RUN cd /src && \
    cargo build --release && \
    mkdir -p /build/ && \
    cp target/release/openai-reverse-proxy /build/server


FROM node:23-bookworm-slim

COPY ./client-example /src

RUN cd /src/web && \
    npm install && \
    npm run build && \
    mkdir -p /build/ && \
    cp -r build index.html /build/


FROM debian:bookworm-slim
EXPOSE 4000/tcp

RUN apt-get update && \
    apt-get install -y openssl && \
    apt-get clean

COPY --from=0 /build /srv
COPY --from=1 /build /srv/static
COPY .env /srv/

WORKDIR /srv/
ENV RUST_LOG=debug
CMD ["./server", "--server=https://api.openai.com/", "--kind=openai", "--files=./static/"]
