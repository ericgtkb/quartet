# rhythm: the Go conductor and public API. Build from the repository root:
#   docker build -f rhythm/Dockerfile .
FROM golang:1.27.4-trixie AS build
# protoc for code generation; the Go plugins are pinned as go.mod tool deps.
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src/rhythm
COPY rhythm/go.mod rhythm/go.sum ./
RUN go mod download
COPY score/ /src/score/
COPY rhythm/ ./
RUN scripts/gen-proto.sh \
    && CGO_ENABLED=0 go build -trimpath -o /out/quartet-rhythm ./cmd/quartet-rhythm

FROM gcr.io/distroless/static-debian13:nonroot
COPY --from=build /out/quartet-rhythm /quartet-rhythm
ENV QUARTET_RHYTHM_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/quartet-rhythm"]
