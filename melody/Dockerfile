# melody: PyTorch training and inference on CPU (Linux gets CPU-only torch
# wheels through the sys_platform marker in pyproject.toml). Build from the
# repository root:
#   docker build -f melody/Dockerfile .
FROM python:3.14.10-slim-trixie AS build
COPY --from=ghcr.io/astral-sh/uv:0.13.9 /uv /bin/uv
ENV UV_LINK_MODE=copy UV_COMPILE_BYTECODE=1 UV_PYTHON_DOWNLOADS=never
WORKDIR /src/melody
COPY melody/pyproject.toml melody/uv.lock melody/.python-version ./
# Dependencies first (cached), including grpcio-tools for code generation.
RUN uv sync --frozen --no-install-project
COPY score/ /src/score/
COPY melody/ ./
RUN scripts/gen-proto.sh \
    # Then drop the dev group; the generated code stays in src/.
    && uv sync --frozen --no-dev --no-editable

FROM python:3.14.10-slim-trixie
WORKDIR /app
COPY --from=build /src/melody/.venv /src/melody/.venv
ENV PATH=/src/melody/.venv/bin:$PATH \
    QUARTET_MELODY_ADDR=0.0.0.0:50052 \
    QUARTET_MELODY_DEVICE=cpu \
    QUARTET_MELODY_RUNS=/data/runs \
    HOME=/tmp
RUN mkdir -p /data/runs && chown 65532:65532 /data/runs
USER 65532:65532
VOLUME ["/data/runs"]
EXPOSE 50052
ENTRYPOINT ["quartet-melody"]
CMD ["serve"]
