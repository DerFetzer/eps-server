FROM rust:1.64 as builder
WORKDIR /usr/src/eps_server
COPY . .
RUN cargo install --path .

FROM debian:buster-slim
# RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/eps_server /usr/local/bin/eps_server
ENTRYPOINT ["eps_server"]
