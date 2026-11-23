# Quartet Agent Instructions

Instructions for coding agents working in this repository. Read [README.md](README.md) first for the system design, the glossary, and the open questions.

These rules add to the user's global guidelines for each language (tooling, style, testing, dependencies) and don't replace them. Follow both. Where they conflict, this file wins for this repository.

> **Status:** the planned system is built (README, What's built): live performances with builtin or trained policies and evolution stream to a 3D stage, PPO trains against rehearsals from the CLI or the UI, harmony's census and training curves have dashboards, and the stack runs in Linux containers. The open items are in README, To do. When you change any of these, update this file in the same change.

## Project shape

Quartet is a polyglot monorepo. Each subproject is an independent build with its own native toolchain:

| Directory  | Language   | Toolchain                               | Role                                      |
| ---------- | ---------- | --------------------------------------- | ----------------------------------------- |
| `score/`   | Protobuf   | protoc (per-language generators)        | Internal gRPC contracts                   |
| `harmony/` | Rust       | Cargo workspace, tonic                  | Simulation, aggregation, gRPC server      |
| `melody/`  | Python     | uv, PyTorch (MPS), grpcio, pytest, ruff | Rehearsal client, PPO training, inference |
| `rhythm/`  | Go         | Go modules, chi, grpc-go, golangci-lint | Beat loop, fan-out, REST + SSE API        |
| `timbre/`  | TypeScript | pnpm, Vue + Vite, TresJS, vue-tsc       | 3D stage, dashboards, controls            |

Keep each subproject's work in its own language. Don't move a responsibility to another language because it is convenient. The point of the project is that each language does the work it suits. Every service is a standalone process. Don't embed one language in another (no PyO3, cgo, or WASM bridges between subprojects).

### Documentation layout

The root README's "Repository layout" stays one level deep: one line per top-level entry. Each part (`score/` and the four subprojects) has a README with the same sections: a one-paragraph role, `## Layout` (a tree of its directories and main modules with one-line roles), and `## Commands` (its native commands). When you add, move, or remove a module or directory, update that part's Layout in the same change.

### Subproject naming

Package, distribution, and binary names use the `quartet-<part>` prefix. The name used inside the code is just `<part>`.

| Part      | Package / distribution                             | Name in code                   | Executable               |
| --------- | -------------------------------------------------- | ------------------------------ | ------------------------ |
| `harmony` | `quartet-harmony` (core), `quartet-harmony-server` | `harmony` (via `[lib] name`)   | `quartet-harmony-server` |
| `melody`  | `quartet-melody`                                   | `melody`                       | `quartet-melody`         |
| `rhythm`  | `github.com/ericgtkb/quartet/rhythm`               | `rhythm/internal/...` packages | `quartet-rhythm`         |
| `timbre`  | `quartet-timbre` (`"private": true`)               | n/a                            | n/a (static build)       |

## Cross-project rules

### Contracts

- `score/` is the only place where internal (service-to-service) types and RPCs are defined. Don't define the same shapes by hand in any language.
- Internal service-to-service calls use gRPC, defined in `score/*.proto` with proto package `quartet.score`. The `quartet.` prefix keeps wire-level names such as `/quartet.score.HarmonyService/Step` and the Go registry's full names unlikely to clash. Every generator uses the repository root as its include path, so imports are written `import "score/world.proto";`.
- Every `.proto` file sets `option go_package = "github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score";`.
- The package has no version suffix. Everything is built from one commit and deployed together, so breaking changes are allowed as long as every consumer is updated in the same change. If a service is ever deployed on its own, introduce versioned packages and a breaking-change check at that point.
- The public browser API is REST/JSON plus Server-Sent Events, served by `rhythm` and listed in the README's "Public API" table. It has no schema file and no codegen. Go types (`rhythm/internal/api`) and TypeScript types (`timbre/src/api/types.ts`) are written by hand. Any API change must update the Go types, the TypeScript types, and the README table together. JSON field names are camelCase.
- Generated code is **never committed**. Every generated directory is Git-ignored, and nothing in it is edited by hand. `just gen` regenerates everything, and the `check`, `test`, and `dev` recipes run it first. Docker builds generate code inside the image.
- Generated code lives in these locations:

  | Part      | Location                                                                 | Generator                                                                                                                                             |
  | --------- | ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
  | `harmony` | Server crate's `OUT_DIR`, included as `crate::generated::quartet::score` | `build.rs` with `tonic-prost-build`. Prefer `protox` (pure Rust) so no system `protoc` is needed.                                                     |
  | `melody`  | `melody/src/melody/generated/`                                           | `grpcio-tools` (bundles `protoc`), with `--pyi_out` for type stubs                                                                                    |
  | `rhythm`  | `rhythm/internal/generated/quartet/score/`                               | `protoc` with `protoc-gen-go` and `protoc-gen-go-grpc`. The plugins are pinned as `go tool` deps; `protoc` comes from Homebrew or the distro package. |

- These generator invocations were verified with a throwaway proto, run from the subproject directory:

  ```sh
  # melody: output under generated/quartet/ so files land in generated/quartet/score/
  OUT=src/melody/generated/quartet
  uv run python -m grpc_tools.protoc -I .. \
    --python_out=$OUT --pyi_out=$OUT --grpc_python_out=$OUT ../score/*.proto
  sed -i '' -E 's/^from score import /from melody.generated.quartet.score import /' \
    $OUT/score/*.py $OUT/score/*.pyi   # GNU sed: -i without ''
  # then create __init__.py in generated/, generated/quartet/, generated/quartet/score/

  # rhythm: module= strips the prefix so files land in internal/generated/quartet/score/
  protoc -I .. \
    --go_out=internal/generated --go_opt=module=github.com/ericgtkb/quartet/rhythm/internal/generated \
    --go-grpc_out=internal/generated --go-grpc_opt=module=github.com/ericgtkb/quartet/rhythm/internal/generated \
    ../score/*.proto
  ```

- Python's protobuf generator writes absolute imports based on the proto *file path*, not the package (`from score import world_pb2`). Writing the output under `generated/quartet/` and rewriting those imports is what produces `melody.generated.quartet.score`. Keep both steps if you change the generator.
- Not committing generated code means a fresh clone needs `just gen` before the IDE and `go build` work, and `go install` from a remote won't work. That's acceptable for an application.
- When you change a contract, update every consumer in the same change.

### Simulation invariants

- `harmony`'s core crate is the single implementation of the world. Both performances (live) and rehearsals (training) run it inside the `harmony` server. Never reimplement world logic in Python, Go, or TypeScript.
- The simulation is deterministic: a seeded RNG, a fixed timestep, and no wall-clock time or iteration over unordered maps inside a step. The same seed and actions must produce the same trajectory.
- The core crate does no I/O. Networking lives in the server crate.
- Each world has a fixed number of agent slots, so tensor shapes never change. A dead quartetling's slot reports `done` and respawns.
- Observation and action layouts are versioned. If you change an observation's size or meaning, bump the observation version and make `melody` refuse to load checkpoints trained on a different version.

### Service boundaries

- Only `rhythm` is reachable from the browser. `harmony` and `melody` are internal gRPC services.
- Call directions are fixed: `timbre` → `rhythm`; `rhythm` → `harmony` and `melody`; `melody` → `harmony` for rehearsals only. `harmony` never calls out.
- `rhythm` is the only component that drives live beats. In a rehearsal, `melody` drives the steps over one bidirectional `Rehearse` stream.
- Send bulk tensors (observations, actions, rewards, flags) as packed little-endian `bytes` (`quartet.score.Tensor`) with an explicit shape and dtype, not as repeated proto fields. Don't put them in the public API either. `rhythm` converts snapshots into JSON frames.
- All service-to-service traffic is TCP, in development and production. Unix sockets were measured and dropped (README, Rehearsal throughput); don't reintroduce them.
- Message limits: harmony accepts and sends up to 64 MiB, and melody's rehearsal channel matches. tonic's 4 MiB default is too small for large rehearsals.

### Domain naming

Use musical names for components and domain concepts: quartetling, voice, performance, rehearsal, beat, chord, mote, static, census. Use standard terms for technical concepts: policy, checkpoint, observation, reward, episode. Don't invent new musical names for technical things.

## Per-subproject conventions

### `score/` (contracts)

- Proto files go directly in `score/`, with package `quartet.score`. On purpose, the directory doesn't mirror the package: it avoids a `score/quartet/score/` nesting, and with no buf linter nothing enforces the match.
- Generated names: Python `melody.generated.quartet.score`, Go `github.com/ericgtkb/quartet/rhythm/internal/generated/quartet/score` (package `score`), and Rust `crate::generated::quartet::score` inside the harmony server crate (`tonic::include_proto!("quartet.score")`).
- Generated Rust types belong only to the server crate. The core crate (`harmony`) must not depend on prost or tonic. The server converts between proto types and core types.
- Use one file per internal service (`harmony.proto`, `melody.proto`), plus shared message files: `world.proto` for quartetlings, world snapshots, and census, and `tensor.proto` for packed tensors. `rhythm` has no proto service, because its public API is plain REST.
- No buf. Each language generates code with its own tool (see the codegen table above), and `just gen` runs the Python and Go generators. Proto style follows the [Protobuf style guide](https://protobuf.dev/programming-guides/style/) and is checked in review, not by a linter.

### `harmony/` (Rust)

- Cargo workspace with `core/` (package `quartet-harmony`, `[lib] name = "harmony"`) and `server/` (package `quartet-harmony-server`). Use edition 2024.
- Only `server/` depends on tonic and tokio, so the crate boundary enforces the no-I/O rule for `core/`.
- Core modules: `world` (rules, stepping, and every balance constant at the top), `observe` (the versioned observation layout, documented in its module comment), `policy` (builtin baselines), `batch` (parallel worlds for rehearsals, rayon), `census` (aggregates and the per-world `CensusRecorder` history), `rng` (SplitMix64; don't swap in `rand`, whose values aren't stable across versions).
- Server modules: `stage` (live worlds by id, each with its census recorder), `rehearsal` (the `Rehearse` stream; stepping runs under `block_in_place`), `convert` (protos and tensors), `service`.
- Changing the observation layout means bumping `OBSERVATION_VERSION` and updating the `layout_size_is_stable` test. Changing balance constants means re-running the baselines with `cargo run --release -p quartet-harmony --example baselines` (or `just evaluate <runs>`, which also measures trained policies) and updating the README tables (World mechanics, Sample runs). Existing checkpoints stay loadable after balance changes but were trained on the old balance.
- Live performances stay single-threaded per world; only rehearsal batches use rayon.
- Evolution is `WorldConfig::evolve`: performances only. `WorldBatch::new` rejects it, so rehearsals never evolve and the training distribution stays fixed. Offspring refill vacated slots of their own voice (slot *i* is always voice *i* mod 4), and `Genome::mutate` clips to `Genome::bounds`, the range policies were trained on. A world without `evolve` must consume the RNG exactly as before (the `evolution_off_keeps_the_old_random_stream` test), so seeds and baselines don't shift. Re-check `cargo run --release -p quartet-harmony --example evolution` after changing the evolution constants in `world.rs`.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Module layout: `foo.rs` with child modules under `foo/`, not `foo/mod.rs`.
- Write the step loop without allocating on each step. Benchmark it with `criterion` before and after you optimize.

### `melody/` (Python)

- Python 3.14 (`requires-python = ">=3.14"`) and uv. Use the newest release of each library that is at least 7 days old: set `exclude-newer = "7 days"` under `[tool.uv]`, which needs uv 0.9.17 or later, and upgrade with `uv lock --upgrade`. Any exception must use `exclude-newer-package` and be flagged. Distribution `quartet-melody` with import package `melody` (`src/` layout). The CLI is `quartet-melody` (`melody.cli:main`), with `serve`, `train`, `evaluate`, and `bench` subcommands.
- The wheel build must include `src/melody/generated/` even though it is Git-ignored. Check this when configuring the build backend.
- PyTorch only. The device is chosen with `--device auto|mps|cpu`, where `auto` means MPS if available and CPU otherwise. Support exactly two targets: native macOS on MPS, and Linux containers on CPU. Don't add CUDA.
- On Linux, `torch` comes from the PyTorch CPU index (`explicit = true`), selected with a `sys_platform == 'linux'` marker in `tool.uv.sources`. macOS uses PyPI.
- Local MPS work always runs natively, because Docker on macOS can't reach MPS.
- Decode packed tensors with `numpy.frombuffer` → `torch.from_numpy` → `.to('mps')`. Don't decode element by element.
- `ruff check`, `ruff format` (single quotes), `pytest`.
- Keep training scripts in the CleanRL style: a readable top-to-bottom loop with all hyperparameters in one config dataclass. Don't build a framework.
- Run artifacts such as checkpoints and logs go in `melody/runs/`, which is Git-ignored. Every checkpoint records its config, observation version, seed, and Git revision.
- The inference service uses `grpc.aio` and batches all agents of a performance into one forward pass.
- `melody.rehearsal.Rehearsal` is the training environment (Gymnasium-style vector API, same-step autoreset). `melody.tensor` encodes and decodes packed tensors. `quartet-melody bench` measures throughput. It registers the standard gRPC health service (`grpc_health.v1.health.aio`).
- Learning modules: `policy` (the actor-critic and harmony's action layout), `train` (the PPO loop and its `Config`; CLI flags are generated from the dataclass, so add hyperparameters there only), `checkpoint` (run directories, atomic writes, version checks), `inference` (`PolicyStore` for `Act`, reloading changed checkpoints), `runs` (training runs as child processes for the server), `evaluate` (same protocol as harmony's `baselines` example).
- Inference runs in `asyncio.to_thread`, never on the event loop. Training runs never run inside the server process: the server spawns `python -m melody.cli train --json`. Keep the JSON-lines protocol (`{"metrics": …}` per update, then `{"status": …}`) in sync with `melody.runs`.
- Metrics and checkpoint metadata must stay JSON-safe: no NaN or infinity (Go's `encoding/json` refuses them; rhythm also maps them to 0 as a guard). Checkpoint metadata holds only plain values, because checkpoints load with `torch.load(weights_only=True)`.
- Run ids must pass `checkpoint.check_run_id` (one safe path component). They come from users through `rhythm`, so never join an unchecked id onto `runs/`.
- ruff excludes `src/melody/generated/`.

### `rhythm/` (Go)

- `gofmt`, `go vet ./...`, `golangci-lint run`, `go test -race ./...`.
- Module path `github.com/ericgtkb/quartet/rhythm`. The entry point is `cmd/quartet-rhythm/`, and all other code goes under `internal/`.
- Public API: a [chi](https://github.com/go-chi/chi) router with chi middleware (request ID, logging, recoverer, timeouts). Handlers use plain `net/http` signatures and hand-written request and response structs in `internal/api`. Validate request bodies before using them. Return errors as JSON `{ "error": "..." }` with a matching status code. Every endpoint has a handler test that pins its JSON shape. Serve SSE with the standard library (`http.ResponseController` flushes). Stream routes must not get chi's `Timeout` middleware. Encode each snapshot once per beat, not once per subscriber.
- Internal calls: `grpc-go`.
- Each performance runs in its own goroutine with a `context.Context` for cancellation. Every outbound gRPC call has a deadline. A training metrics watch is long-lived, so its deadline is 12 hours rather than `upstreamTimeout`.
- A checkpoint-policy beat is `melody.Act` on the previous observations, then `harmony.StepWorld` with those actions and `include_observations`, both under one beat deadline. Map upstream gRPC codes to HTTP statuses with `performance.GRPCStatus` (it unwraps `%w` chains) rather than matching error strings.
- The subscriber fan-out uses bounded per-subscriber buffers that drop the oldest frame. A slow client must never slow down a beat.
- Shut down gracefully on SIGINT/SIGTERM.

### `timbre/` (TypeScript)

- Package `quartet-timbre`, `"private": true`.
- TypeScript 6, not 7 (see [Known quirks](#known-quirks-and-workarounds)).
- pnpm, Vue 3 + Vite, TresJS. All API calls go through one small typed client in `src/api/`, using `fetch` for REST and `EventSource` for streams, with hand-written types in `src/api/types.ts`. Use strict TypeScript and type-check with `vue-tsc`.
- No Tailwind and no component library. Build reusable components (buttons, panels, inputs, charts, and so on) in `src/components/ui/`. Style them with scoped CSS and global design tokens (CSS custom properties) in `src/styles/tokens.css`. Components use tokens, never hard-coded colors or spacing.
- Tests run in jsdom (see [Known quirks](#known-quirks-and-workarounds) for the `localStorage` workaround).
- Light and dark themes are required. The tokens are defined for both. By default the theme follows `prefers-color-scheme`, and a toggle overrides it by setting `data-theme` on `<html>`. Scope dark values as `:root[data-theme="dark"]` and, inside `@media (prefers-color-scheme: dark)`, as `:root:not([data-theme="light"])`. Don't use a bare `[data-theme="dark"]`: it has the same specificity as `:root`, so it only works when it comes later in the file. The toggle is saved in `localStorage`, with reads and writes wrapped in try/catch. The 3D scene (background, lights, materials) also reads the theme tokens so the stage matches the UI.
- In development, the Vite dev server proxies `/api` to `rhythm`, so the browser sees one origin.
- Format with Oxfmt and lint with Oxlint. Use ESLint with `eslint-plugin-vue` for templates.
- The stage is imperative three.js inside TresJS: `src/stage/StageScene.ts` owns instanced meshes (bodies, song halos, motes, static fields) and rewrites their matrices each frame; `StageContent.vue` calls it from `useLoop().onBeforeRender`. Don't use one mesh per creature. Interpolate between beat snapshots (`src/stage/interpolate.ts`) instead of rendering only at the beat rate.
- Frames are struct-of-arrays JSON (`Frame` in `src/api/types.ts`, `performance.Frame` in Go), indexed by slot. Keep it that way: it is compact and maps directly onto instance buffers.
- The performance page is lazy-loaded so three.js stays out of the initial bundle.
- Charts are hand-written SVG components in `src/components/ui/` (`QLineChart`, `QColumns`, `QHeatmap`), with no chart library. Series colors come from `--color-series-<voice>` and `--color-series-1`, never the stage's `--color-voice-*` directly: dark mode re-steps the series colors into a darker lightness band for chart legibility, validated with the dataviz palette checks. Every chart has a hover/focus readout, and line charts have a table view. Never use a second y-axis: give different scales separate charts.
- Single quotes in TypeScript, double quotes in templates.

## Commands

The root `justfile` coordinates work across projects. It requires `just`, `protoc`, and each language's toolchain on `PATH`. Inside a single subproject, use its native tool directly. Each codegen step is a script (`melody/scripts/gen-proto.sh`, `rhythm/scripts/gen-proto.sh`), so Dockerfiles can call it without `just`.

```sh
just gen     # generate gRPC code for Python and Go (Rust generates in build.rs)
just check   # lint and type-check every subproject
just test    # run every subproject's tests
just dev     # build, then run all four services with prefixed logs (scripts/dev.sh)
just bench   # rehearsal throughput against a release build of harmony (args go to quartet-melody bench)
just train   # PPO against a release build of harmony (args go to quartet-melody train); runs land in melody/runs/
just evaluate RUN...  # builtin baselines and trained runs on the same metrics
```

`bench`, `train`, and `evaluate` start their own release harmony on a private port through `scripts/with-harmony.sh`, so they can run alongside `just dev`.

`just check` runs the linters in check mode. `pnpm lint` and `pnpm format` in `timbre` apply fixes, so don't use them to verify.

Run `just check` and `just test` before finishing a change. To check the full chain by hand, run `just dev` and then `curl localhost:5173/api/ensemble` (through the Vite proxy) or `curl 127.0.0.1:8080/api/healthz`.

There are two run targets:

- **Native macOS** (development, MPS): use the `just` recipes above.
- **Linux containers** (CPU): each subproject has a `Dockerfile`, and the root `compose.yaml` runs the stack (`docker compose up --build`, then <http://localhost:8000>). Build contexts are the repository root, so `score/` is available for code generation; the root `.dockerignore` keeps local builds, environments, runs, and generated code out. Give the user Docker commands to run rather than running them yourself. Read-only checks are fine: `docker compose config --quiet`, `docker build --check -f <part>/Dockerfile .`, and `docker buildx imagetools inspect <image>` to confirm a tag exists.

Container conventions:

- Images generate their code in the build stage with the same `scripts/gen-proto.sh` as `just gen`. Runtime images contain no compilers and run as a non-root user.
- Pin base images to exact version tags that are at least 7 days old, matching the native toolchains (Rust, Go, Python, Node, uv, pnpm). Bump them together with the native versions.
- Listen on `0.0.0.0` inside containers (set in each Dockerfile's `ENV`); services find each other by compose service name. Only `timbre` publishes a port.
- `timbre`'s runtime is Caddy (`timbre/Caddyfile`): it serves `dist/` with an SPA fallback and proxies `/api` to `rhythm` with `flush_interval -1`, so SSE frames aren't buffered. Don't put `encode` on the `/api` route.
- melody's runs live in the `melody-runs` volume at `/data/runs`. There is no Git inside the image, so checkpoints made there record `git_revision: "unknown"`.

Configuration is read from environment variables:

| Variable                | Default                 | Used by                             |
| ----------------------- | ----------------------- | ----------------------------------- |
| `QUARTET_RHYTHM_ADDR`   | `127.0.0.1:8080`        | rhythm (HTTP listen)                |
| `QUARTET_HARMONY_ADDR`  | `127.0.0.1:50051`       | harmony (listen), rhythm (target)   |
| `QUARTET_MELODY_ADDR`   | `127.0.0.1:50052`       | melody (listen), rhythm (target)    |
| `QUARTET_MELODY_DEVICE` | `auto`                  | melody (`auto`, `mps`, `cpu`)       |
| `QUARTET_MELODY_RUNS`   | `runs` (in `melody/`)   | melody (training runs, checkpoints) |
| `QUARTET_RHYTHM_URL`    | `http://127.0.0.1:8080` | timbre's Vite dev proxy for `/api`  |
| `RUST_LOG`              | `info`                  | harmony log filter                  |

Containers set listen addresses to `0.0.0.0:<port>`.

## Known quirks and workarounds

These are deliberate workarounds, not mistakes. Don't "clean them up" unless the removal condition is met. When you work near one, check whether the condition now holds; if it does, remove the workaround in a focused change and update this table. Dates are when the quirk was recorded.

| Quirk (recorded)                                                     | Where                                       | Why                                                                                                                                                                                                                                                                                                                                                                                                               | Revisit / remove when                                                                                                                                                            |
| -------------------------------------------------------------------- | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TypeScript pinned to 6 (2026-11)                                     | `timbre/package.json`                       | `vue-tsc` needs TypeScript's JS compiler API, which the native TS 7 doesn't ship                                                                                                                                                                                                                                                                                                                                  | A stable `vue-tsc` / Vue Language Tools release supports TS 7 ([vuejs/language-tools#5381](https://github.com/vuejs/language-tools/issues/5381)). Then upgrade and drop this row |
| `--no-experimental-webstorage` in Vitest (2026-11)                   | `timbre/vitest.config.ts`                   | Node 25+ defines its own `localStorage` global, which is `undefined` without `--localstorage-file` and hides jsdom's                                                                                                                                                                                                                                                                                              | Vitest's jsdom environment handles Node's Web Storage itself (or Node stops enabling it by default). Remove the flag and confirm `useTheme.spec.ts` passes                       |
| Python import rewrite after `protoc` (2026-11)                       | `melody/scripts/gen-proto.sh`               | Python's protobuf generator writes imports from the proto *file path* (`from score import …`), not the package; output goes under `generated/quartet/` and imports are rewritten to `melody.generated.quartet.score`                                                                                                                                                                                              | `grpcio-tools` gains an option for the import prefix or relative imports. Until then, keep both steps whenever the generator changes                                             |
| No minimum release age for Go modules (2026-11)                      | `rhythm/go.mod`                             | The Go toolchain has no cooldown setting. Versions are chosen by hand: the newest non-prerelease at least 7 days old (`go list -m -json mod@version` shows `Time`)                                                                                                                                                                                                                                                | Go adds a cooldown setting. Then configure it in this repo and drop the manual step                                                                                              |
| System `protoc` for Go only, unpinned (2026-11)                      | `rhythm/scripts/gen-proto.sh`               | The Go plugins are pinned as `go tool` deps, but `protoc` itself comes from Homebrew or the distro (tested with libprotoc 36.2). Python bundles `protoc` in `grpcio-tools` and Rust uses `protox`                                                                                                                                                                                                                 | Generated Go code changes or breaks across `protoc` versions. Then pin it, for example via buf as a `go tool`                                                                    |
| Generated code lives in Git-ignored directories (2026-11)            | `melody/src/melody/generated/`              | `uv_build` currently includes Git-ignored files in the wheel (verified inside a Git repo)                                                                                                                                                                                                                                                                                                                         | You change the build backend or its config. Then re-check that the wheel contains `melody/generated/`                                                                            |
| `pnpm lint` and `pnpm format` auto-fix (2026-11)                     | `timbre/package.json` (create-vue defaults) | They're convenient locally but can't verify anything. `just check` calls the tools in check mode                                                                                                                                                                                                                                                                                                                  | Not planned. Use `just check` to verify                                                                                                                                          |
| The tensor dtype enum is `Dtype`, not `DType` (2026-11)              | `score/tensor.proto`                        | prost strips enum-value prefixes derived from the enum name; `DType` becomes `D_TYPE_`, which doesn't match `DTYPE_*`, giving variants like `DtypeFloat32`                                                                                                                                                                                                                                                        | Not planned. Name new enums so their SCREAMING_SNAKE form is the value prefix                                                                                                    |
| Headless screenshots of the stage need `--use-angle=metal` (2026-11) | manual visual checks                        | With SwiftShader, headless Chrome renders WebGL but can't capture the canvas into the screenshot (blank stage). Also don't use `--virtual-time-budget`: the SSE stream never idles, so it never returns. `--screenshot` also fires before lazy routes (performance, training run) render; capture through the DevTools protocol (`--remote-debugging-port`, `Page.captureScreenshot`) after a few seconds instead | A Playwright-based visual check is added                                                                                                                                         |
| `scripts/dev.sh` targets macOS bash 3.2 (2026-11)                    | `scripts/dev.sh`                            | No `wait -n`, so if one service dies the others keep running until Ctrl-C                                                                                                                                                                                                                                                                                                                                         | The script moves to bash 4.3+ or gets replaced by a process manager                                                                                                              |

## Dependencies

- Require dependency releases to be at least 7 days old wherever the package manager supports it: uv (`exclude-newer = "7 days"`), pnpm (`minimumReleaseAge: 10080`, in minutes, in `timbre/pnpm-workspace.yaml`), and Cargo (`registry.global-min-publish-age = "7 days"` in `harmony/.cargo/config.toml`, stable since Cargo 1.100). Go modules have no equivalent: pin exact versions and check their release dates before adding or upgrading. Within that limit, prefer the newest releases. Flag any exception.
- Commit every lockfile: `uv.lock`, `Cargo.lock`, `go.sum`, and `pnpm-lock.yaml`.
- In pnpm, keep install scripts blocked except for packages that need them to build.
