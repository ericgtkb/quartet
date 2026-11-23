# harmony: the Rust world engine. Build from the repository root:
#   docker build -f harmony/Dockerfile .
# protox compiles score/*.proto in build.rs, so no system protoc is needed.
FROM rust:1.100.0-slim-trixie AS build
WORKDIR /src
COPY score/ score/
COPY harmony/ harmony/
WORKDIR /src/harmony
RUN cargo build --locked --release -p quartet-harmony-server

FROM debian:trixie-20261104-slim
COPY --from=build /src/harmony/target/release/quartet-harmony-server /usr/local/bin/
USER 65532:65532
ENV QUARTET_HARMONY_ADDR=0.0.0.0:50051 RUST_LOG=info
EXPOSE 50051
ENTRYPOINT ["quartet-harmony-server"]
