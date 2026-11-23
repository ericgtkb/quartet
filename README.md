# Quartet

Quartet is a toy system of four services, one per language: Python, Rust, Go, and TypeScript. Each part does the kind of work its language suits best, and they are connected into one running system. The services simulate a world of small sci-fi creatures called **quartetlings**. The creatures learn their behavior through multi-agent reinforcement learning, and you watch them live in a 3D browser view.

> **Status:** the system described here is built and running; see [What's built](#whats-built) and [Sample runs](#sample-runs). Run `just dev`, open <http://localhost:5173>, and start a performance or a training run, or run the Linux stack with `docker compose up --build`. What remains is in [To do](#to-do).

## Why the names

A quartet is a group of four musicians, and here the four players are the languages. Each service is named for an element of music, played by one language:

- **Melody** (Python) is the tune that learns.
- **Harmony** (Rust) is the world that everything sounds against.
- **Rhythm** (Go) keeps the beat.
- **Timbre** (TypeScript) is the tone color: how the performance looks to you.

Melody, harmony, and rhythm are the three elements of music most often named. Timbre is the one that often goes unmentioned but is always heard, much like the frontend. Every quartet plays from a shared **score**, so the contracts all four parts follow, the gRPC `.proto` files, live in `score/`.

The theme carries into the world. Quartetlings sing as soprano, alto, tenor, and bass, the four voices of a vocal quartet. Their genomes are measured in tempo, pitch, and dynamics. A live world is a **performance**, training is a **rehearsal**, and a simulation tick is a **beat**.

## The ensemble

| Part      | Language   | Role                                                                                                           |
| --------- | ---------- | -------------------------------------------------------------------------------------------------------------- |
| `score`   | Protobuf   | The internal gRPC contracts shared by `harmony`, `melody`, and `rhythm`.                                       |
| `harmony` | Rust       | The world engine: a standalone gRPC service for deterministic simulation, spatial queries, and aggregation.    |
| `melody`  | Python     | The learning: PyTorch PPO training against `harmony`, and policy inference serving.                            |
| `rhythm`  | Go         | The conductor: drives the beat loop, runs concurrent worlds, and turns internal gRPC into the public REST API. |
| `timbre`  | TypeScript | The stage: a Vue + TresJS 3D view of live worlds, plus dashboards and controls.                                |

Each service is a separate process that owns its own job. No language is embedded in another.

## Architecture

```text
                       REST (JSON) + Server-Sent Events
  timbre (Vue + TresJS) <──────────────────────────────> rhythm (Go)
                                                          │        │
                                       gRPC: Stage, Census│        │gRPC: Act, Train
                                                          ▼        ▼
                                              harmony (Rust) <──── melody (Python)
                                                      gRPC: Rehearse
                                              (bidirectional stream, training only)
```

There are three flows through the system.

### Live path: watching a performance

A running world is called a **performance**. `rhythm` runs each performance in its own goroutine. On every **beat** (one simulation tick, about 10 to 20 Hz):

1. `rhythm` sends the current observations to `melody.Act`. `melody` runs batched policy inference and returns one action per quartetling.
2. `rhythm` sends those actions to `harmony.Step`. `harmony` advances the world and returns the next observations, a world snapshot, and the beat's events.
3. `rhythm` encodes the snapshot as JSON once and fans it out to every subscribed browser over Server-Sent Events. Each subscriber has a bounded buffer, so a slow client drops old frames instead of stalling the performance.

`timbre` renders the stream and interpolates between beats for smooth motion.

Each live world also keeps a **census**. After every beat, `harmony` folds the beat's stats into a 10-beat window and keeps the last 720 windows (8 minutes at 15 beats/s) without allocating. `GetCensus` adds the current per-voice aggregates, 16-bin histograms of each trait and of energy, and a 20 × 20 density grid. `timbre` polls it every 2 seconds while a performance is open.

A performance can also use a **builtin policy** computed inside `harmony` (`random`, `forage`, or `choir`). Then step 1 is skipped and `StepWorld` carries only the policy name. With a trained policy, `CreateWorld` returns the initial observations and the observation version, and `rhythm` calls `Act` once while creating the performance. An unknown or incompatible checkpoint is therefore rejected with a 400 instead of failing on the first beat. A checkpoint that is still training is reloaded whenever its file changes, so a performance picks up newer weights.

### Training path: rehearsals

RL needs millions of environment steps, so training cannot make one network round trip per step. Instead, `melody` opens a **rehearsal**: one bidirectional gRPC stream to `harmony`. `harmony` runs a batch of independent worlds, for example 64 worlds with 128 agents each, for the lifetime of that stream.

- The first message configures the rehearsal (number of worlds, seed, world parameters). `harmony` replies with the first observations.
- Each later message carries the actions for every agent in every world. `harmony` steps all the worlds in parallel (rayon) and replies with the next observations, rewards, and `terminated`/`truncated` flags.
- Autoreset is same-step: a dead (terminated) or retired (truncated) quartetling's slot is refilled within the step, and the returned observation already belongs to the newcomer. Every slot is always alive, so there are no alive masks.
- Tensors travel as packed little-endian `bytes` with an explicit shape and dtype. `melody` reads them with `numpy.frombuffer` and hands them to `torch.from_numpy`, with no per-element decoding.
- Transport is TCP, the same in development and production.

Each round trip carries thousands of agent-steps, so the network overhead is spread across all of them. See [Rehearsal throughput](#rehearsal-throughput) for measurements. Embedding Rust in Python is not an option.

The same `harmony` core crate runs both the live worlds and the rehearsal worlds, so training and the live world cannot drift apart.

### Control path: training from the UI

`timbre` starts and watches training runs through `rhythm`'s REST API. `rhythm` calls `melody.StartTraining`, and `melody` starts the run as a child process (`quartet-melody train --json`), so training never competes with the inference server's event loop. The child prints one JSON line per PPO update; `melody` keeps those metrics and `WatchTraining` replays them, then streams new ones. `rhythm` relays them as Server-Sent Events, one gRPC watch per browser, which is fine at one event per update (about every half second). One run trains at a time. Stopping sends SIGTERM: the run finishes its update, saves a checkpoint, and ends `stopped`.

Every run is a directory under `melody/runs/<run id>/` with `checkpoint.pt`, `run.json` (config, observation version, seed, Git revision, status, latest metrics), and `metrics.jsonl`. Files are replaced atomically. Any run with a checkpoint is a policy, including runs started with `just train` and runs from earlier sessions.

### Why REST and not gRPC to the browser

Browsers can't speak native gRPC, because `fetch` doesn't give access to HTTP/2 trailers. gRPC-Web and Connect work around this, but REST plus Server-Sent Events is the most conventional option, and it can be debugged with `curl` and browser devtools. It also gives `rhythm` real work as a gateway: it turns the internal binary tensors and protos into a resource-oriented public API that suits the frontend.

- **REST/JSON** for commands and queries: performances, policies, training runs, and census.
- **Server-Sent Events** for the two live streams: performance snapshots and training metrics. Both only flow from server to browser, so WebSocket isn't needed. Go can serve SSE with only the standard library, and browsers support it natively through `EventSource`.
- **No API schema or codegen.** `rhythm` is a plain [chi](https://github.com/go-chi/chi) app with hand-written JSON types, and `timbre` mirrors them in one hand-written TypeScript module. The API is small, so a spec and generators would cost more than they save. The tradeoff is that Go and TypeScript types can drift apart. To prevent that, endpoint changes update both sides in the same change, and Go handler tests pin the JSON shapes.

### Public API

All routes are under `/api`. JSON uses camelCase.

| Method   | Path                             | Purpose                                                                                           |
| -------- | -------------------------------- | ------------------------------------------------------------------------------------------------- |
| `GET`    | `/api/healthz`                   | Health of `rhythm` and its upstreams                                                              |
| `GET`    | `/api/ensemble`                  | Version, runtime, and status of each service                                                      |
| `GET`    | `/api/performances`              | List live performances                                                                            |
| `POST`   | `/api/performances`              | Start one: `policy` (builtin or run id), `slots`, `seed`, `beatsPerSecond`, `evolve` (default true) |
| `GET`    | `/api/performances/{id}`         | Performance details, including `policyKind` (`builtin` or `checkpoint`)                           |
| `DELETE` | `/api/performances/{id}`         | Stop a performance                                                                                |
| `GET`    | `/api/performances/{id}/stream`  | SSE: `frame` per beat, `end` when stopped                                                         |
| `GET`    | `/api/performances/{id}/census`  | `harmony`'s census: per-voice stats, trait and energy histograms, density grid, recent history    |
| `GET`    | `/api/policies`                  | Builtins, then trained checkpoints (builtins only if `melody` is down)                            |
| `GET`    | `/api/training-runs`             | List training runs, newest first                                                                  |
| `POST`   | `/api/training-runs`             | Start one: `totalSteps`, `worlds`, `slots`, `seed` (0 or omitted = default); 409 if one is running |
| `GET`    | `/api/training-runs/{id}`        | Training run status, config, and latest metrics                                                   |
| `POST`   | `/api/training-runs/{id}/stop`   | Ask a run to stop (202); it saves a checkpoint and its stream ends `stopped`                      |
| `GET`    | `/api/training-runs/{id}/stream` | SSE: `metrics` per PPO update (history first), `end` with the final status                        |

## Rehearsal throughput

Measured with `just bench --worlds 1,4,16,64,256 --slots 64 --streams 1,2 --steps 200` on an Apple M5 Pro (18 cores), with a release build of harmony on the same machine over TCP. Each step sends random actions for every agent and decodes all results into NumPy arrays; 10 warm-up steps aren't timed. These are environment-only numbers: no policy runs.

| Worlds | Agents | ms/step | Agent-steps/s |
| -----: | -----: | ------: | ------------: |
| 1      | 64     | 0.14    | 449,000       |
| 4      | 256    | 0.25    | 1,015,000     |
| 16     | 1,024  | 0.35    | 2,958,000     |
| 64     | 4,096  | 0.68    | 5,996,000     |
| 256    | 16,384 | 2.28    | 7,188,000     |

- **Batch size dominates.** A round trip costs about 0.14 ms, so batching more agents per step raises throughput until simulation and serialization take over, around 16k agents.
- **Concurrent streams don't help.** Splitting the worlds across 2 streams changed throughput by less than ±10%, because harmony already steps a batch's worlds in parallel. Use one stream.
- **Moving observations to MPS** (`--device mps`) cuts throughput to about 1.6M, 3.8M, and 4.9M agent-steps/s at 16, 64, and 256 worlds. That's still far more than PPO on a small MLP will consume, so the environment won't be the training bottleneck.
- **Unix sockets were tried and dropped.** They were slightly faster for tiny batches but about 4× slower for large ones on macOS, whose local sockets default to 8 KiB buffers. Production uses TCP, so everything else does too.

## Sample runs

All runs below were on an Apple M5 Pro (18 cores) on 2026-11-23, with release builds of harmony over TCP on the same machine. They are single runs (one training seed), so treat small differences as noise.

### Training

`just train` with the defaults: 16 worlds × 64 slots, 128-beat rollouts (131k agent-steps per update), 50M agent-steps, seed 1, PyTorch on MPS. It ran at about 253k agent-steps/s and took 3.3 minutes; the rehearsal is not the bottleneck at about 6M agent-steps/s (see [Rehearsal throughput](#rehearsal-throughput)). Selected updates from the run's `metrics.jsonl` (rates are per rollout; deaths and motes per 1,000 beats of a 64-slot world):

| Update | Agent-steps | Deaths / 1k beats | Lifespan (beats) | Mean energy | Chord rate | Motes / 1k beats | Entropy | Approx. KL | Explained var. |
| -----: | ----------: | ----------------: | ---------------: | ----------: | ---------: | ---------------: | ------: | ---------: | -------------: |
| 1      | 0.1M        | 190.4             | 89               | 0.643       | 0.0384     | 440              | 3.52    | 0.0046     | −0.01          |
| 10     | 1.3M        | 244.1             | 239              | 0.578       | 0.0135     | 822              | 3.20    | 0.0058     | 0.24           |
| 25     | 3.3M        | 100.6             | 480              | 0.916       | 0.0004     | 1,721            | 2.61    | 0.0033     | 0.47           |
| 50     | 6.6M        | 16.6              | 2,114            | 1.531       | 0          | 2,972            | 2.01    | 0.0035     | 0.56           |
| 100    | 13.1M       | 14.2              | 2,052            | 1.618       | 0          | 3,530            | 1.45    | 0.0021     | 0.73           |
| 200    | 26.2M       | 11.2              | 2,163            | 1.666       | 0          | 3,501            | 0.79    | 0.0045     | 0.77           |
| 381    | 49.9M       | 7.8               | 2,516            | 1.675       | 0          | 3,647            | 0.41    | 0.0001     | 0.78           |

Lifespan counts only lives that ended in the rollout, so once deaths are rare it is dominated by retirements at 3,000 beats. The run's record says `git_revision: "unknown"` because the repository had no commits yet.

### Evaluation

`just evaluate ppo-default-50m` runs the builtins in harmony's baseline probe and the trained policy in a rehearsal under the same protocol: default 64-slot worlds without evolution, 500 warm-up beats, then 2,000 measured beats over 8 worlds. The trained policy samples its actions, as it does live.

| Policy            | Deaths / 1k beats | Mean energy | Chord rate | Motes / 1k beats |
| ----------------- | ----------------: | ----------: | ---------: | ---------------: |
| `random`          | 367.0             | 0.552       | 0.0006     | 372              |
| `forage`          | 95.6              | 1.179       | 0          | 2,862            |
| `choir`           | 80.9              | 0.870       | 0.179      | 919              |
| PPO, 50M steps    | 8.6               | 1.667       | 0          | 3,657            |

- **The learned policy forages far better than the handwritten one,** with about a tenth of `forage`'s deaths and higher energy, but it **never sings.** The random initial policy sings (chord rate about 0.04), and the rate falls to zero within about 3M steps. Under the current balance, foraging well is worth more than coordinating chords: the local optimum this README warned about. Getting quartets to form is a reward and balance question (a larger chord bonus, scarcer motes, or a chord term in the reward), and it is still open.
- **Episode return is not a useful curve here.** The per-beat reward is `10 × Δenergy`, so a life's undiscounted return telescopes to `10 × (final − starting energy)`, minus 1 on death: every death scores exactly −11, whatever happened before it. Discounting still rewards delaying death, so learning works, but the dashboards chart lifespan, deaths, energy, and chord rate instead.
- **Live inference keeps up.** Performances driven by this checkpoint through `Act` (MPS, one forward pass per beat) held 15 beats/s with 64 slots and about 58 of the requested 60 beats/s with 256 slots.

### Evolution

The deterministic probe `cargo run --release -p quartet-harmony --example evolution forage 20000` runs 8 evolving 64-slot worlds with `forage` for 20,000 beats. Per-voice trait means at the start and the end:

| Voice   | Tempo (speed)  | Pitch (hearing) | Dynamics (body) |
| ------- | -------------- | --------------- | --------------- |
| soprano | 1.628 → 1.783  | 18.26 → 18.07   | 0.699 → 0.663   |
| alto    | 1.355 → 1.566  | 15.66 → 13.31   | 0.904 → 0.752   |
| tenor   | 1.145 → 1.299  | 13.58 → 12.10   | 1.122 → 0.952   |
| bass    | 0.963 → 0.990  | 11.88 → 11.00   | 1.299 → 1.266   |

Births were 43 and deaths 84 per 1,000 beats (95.6 without evolution), mean generation 9.7 (max 20), and 18 of each world's founder lineages survived. Foragers select for speed and small bodies, since metabolism scales with body size. Hearing range has no cost, so it drifts.

Live, three 64-slot performances ran with seed 1 at 60 beats/s for about 12,000 beats. Their census windows covered beats 4,810–12,010:

| Performance             | Births / 1k | Deaths / 1k | Mean energy | Max generation | Lineages alive |
| ----------------------- | ----------: | ----------: | ----------: | -------------: | -------------: |
| PPO 50M, evolving       | 26.5        | 5.3         | 1.615       | 9              | 9              |
| `forage`, evolving      | 42.4        | 73.2        | 1.206       | 14             | 21             |
| `forage`, no evolution  | 0           | 92.1        | 1.137       | 0              | 64             |

- Under `forage`, sopranos evolved to the edge of the trained range (tempo 1.90 of a maximum 1.92, pitch 20.9 of 21.6). Basses barely reproduced (mean generation 0.1), because a foraging bass seldom reaches the 1.5 energy a parent needs.
- Under the trained policy every voice reproduces (mean generation 6–7). Basses got faster (tempo 0.95 → 1.10) and smaller (dynamics 1.30 → 1.13), and one founder lineage in seven survived.

## Quartetlings

Quartetlings live on a continuous 2D plane, which `timbre` renders in 3D. Each one has a **voice**, one of soprano, alto, tenor, or bass, which sets its base traits. Each one also has a small genome that varies those traits slightly.

```text
Quartetling
  id          uint64
  voice       SOPRANO | ALTO | TENOR | BASS   fixed role; sets base traits
  genome
    tempo     max speed (and metabolic cost)
    pitch     hearing radius: how far it perceives others and motes
    dynamics  body size: strength vs. energy cost
  state
    position, heading, velocity
    energy    dies at zero
    breath    0..1; singing uses it, resting restores it
    age       retired at 3000 beats
    singing   whether it sang this beat
    in_chord  whether that song was part of a chord
  lineage
    parent_id   0 for founders
    generation  0 for founders; parent's generation + 1
    lineage     the founder it descends from (its own id for founders)
```

The voices trade off against each other. Sopranos are fast and hear far but are fragile. Basses are slow but sturdy and efficient.

### World mechanics

- **Motes** are drifting energy particles. Quartetlings eat them to gain energy.
- **Static** is a set of slowly moving noise fields that drain the energy of anything inside them.
- **Singing** costs energy and **breath**. A quartetling needs breath to sing; resting restores it, so it can sing about a third of the time.
- **Chords**: when three or four different voices sing within range of each other on the same beat, every member gets an energy bonus larger than the cost of singing. A full four-voice chord pays about twice as much as a three-voice one.
- **Balance**: everything costs energy, even standing still. A stationary three-voice chord can't sustain its singers, but a well-synchronized four-voice quartet can, so food and chords both matter. With the built-in baselines on a 64-slot world, `forage` loses about 97 quartetlings per 1,000 beats and `choir` about 80, while `random` loses 360. The constants live at the top of `harmony/core/src/world.rs`.
- **Reproduction** (performances only, see [Evolution](#evolution)): a well-fed quartetling passes a mutated copy of its genome to an offspring.

The chord mechanic is what makes the system interesting to watch. Foraging alone is the easy local optimum. The better strategy, which the agents have to discover, is to find partners of other voices and coordinate when they sing. If training works, you should see mixed-voice groups, real quartets, form on the stage.

### Learning setup

- **Framework:** PyTorch only. Local runs use Apple Silicon's `mps` device. Containers run Linux on CPU. The device is chosen with `--device auto|mps|cpu`, and `auto` prefers MPS when it's available. CUDA is out of scope for now. Training speed is not a goal yet: correct and readable comes first.
- **Algorithm:** PPO with one policy shared by every agent. The voice is an input to the policy, and a centralized critic is optional (MAPPO-style). It is written CleanRL-style so the training loop reads top to bottom.
- **Agent slots:** each world has a fixed number of agent slots, so tensor shapes never change. When a quartetling dies or is retired, its slot reports `terminated` or `truncated` and is filled by a newly spawned quartetling. Slot *i* always has voice *i* mod 4.
- **Observation:** egocentric and fixed-size. It covers the agent's own traits and state, its k nearest neighbors (relative position, voice, singing), its nearest motes, and distance to static.
- **Action:** continuous steering (turn, thrust) plus a binary sing.
- **Reward:** change in energy, a chord bonus, and a death penalty.
- **Scale target:** about 64 to 256 agents per world and a small MLP policy.
- **Baselines:** `random`, `forage`, and `choir`, all implemented in `harmony`. They let the full stack run before any training and give a reference point for learned behavior.
- **No RL framework.** PPO is written by hand from PyTorch alone, in one training file that reads top to bottom. It is adapted from CleanRL's [`ppo_continuous_action.py`](https://github.com/vwxyzjn/cleanrl/blob/master/cleanrl/ppo_continuous_action.py) (MIT; keep the credit in a header comment), with details from [The 37 Implementation Details of PPO](https://iclr-blog-track.github.io/2022/03/25/ppo-implementation-details/): orthogonal init, advantage normalization, value clipping, learning-rate annealing, and gradient-norm clipping. CleanRL is a reference to read, not a library to import. Stable-Baselines3 doesn't fit (single-agent, no mixed action spaces), and RLlib and TorchRL were judged too heavy or too abstract for a readable loop.
- **Batch:** one rehearsal stream (more streams don't help; see [Rehearsal throughput](#rehearsal-throughput)). The PPO batch dimension is worlds × slots.
- **Policy head:** a Normal for steering (turn and thrust, squashed or clipped to [-1, 1]) plus a Bernoulli for sing (sent as 0 or 1). The action log-probability is the sum of the two.
- **Advantages:** GAE resets at each slot's `terminated` or `truncated`. With same-step autoreset there is no final observation, so truncation (retirement at 3000 beats) is treated like termination for bootstrapping. Retirement is rare, so the bias is small.
- **Checkpoints** record the config, `OBSERVATION_VERSION`, seed, and Git revision. melody refuses to load a checkpoint whose observation version differs from harmony's: `Act` returns `FAILED_PRECONDITION`, and `rhythm` answers 400.
- **Hyperparameters** live in one `Config` dataclass in `melody/src/melody/train.py`, and every field is also a `quartet-melody train` flag. The defaults are CleanRL's continuous-control ones, except for the batch: 1,024 agents × 128 beats = 131k agent-steps per update, in 16 minibatches. The MLP is 2 × 128 tanh with separate actor and critic, and the entropy coefficient is 0.001 for the sing head's sake.
- **Verification:** a smoke test that PPO learns a trivial bandit task (`melody/tests/test_train.py`), plus learned policies compared against the `forage` and `choir` baselines on deaths, mean energy, and chord rate (`just evaluate`; see [Sample runs](#sample-runs)).

### Evolution

RL learns *behavior*, and evolution changes *bodies*. To keep the two from interfering, they run in different places:

- **Rehearsals don't evolve.** Each spawned quartetling's genome is drawn at random from a fixed range around its voice's base traits (±20%, domain randomization). The policy sees its own genome in its observation, so it learns to act well for any body in that range, and the training distribution never shifts under PPO. harmony rejects `evolve` in a rehearsal.
- **Performances evolve** (`evolve`, on by default when starting a performance). When a slot is vacated, by a death or by retirement at 3,000 beats, harmony picks a random **eligible parent of that slot's voice**: one at least 300 beats old, not about to retire, with energy ≥ 1.5. The parent pays 0.5 energy. The offspring appears within 3 units of it with a fresh 1.0 energy, `parent_id`, `generation + 1`, and the parent's `lineage` (the founder it descends from). Its genome is the parent's, with each trait scaled by `1 + 0.03 · N(0, 1)` and clipped to the trained range. With no eligible parent, a random founder fills the slot, so a world can't go extinct. Slot *i* still always holds voice *i* mod 4.
- **Retirements count as vacancies,** not only deaths. A good policy rarely dies (about 9 per 1,000 beats for the trained one), so filling only deaths would give selection almost nothing to act on.
- **Census tracks it.** `harmony` records per-voice trait means, mean and maximum generation, births, and surviving founder lineages in every 10-beat window. `timbre` charts them under Evolution in the census panel, and the HUD shows the deepest generation.

The constants are at the top of `harmony/core/src/world.rs` (`MATURITY`, `REPRO_ENERGY`, `REPRO_COST`, `OFFSPRING_RADIUS`) and in `voice.rs` (`MUTATION`, `GENOME_SPREAD`). A world without `evolve` uses its random stream exactly as before evolution existed, so seeds and the baselines are unchanged.

Out of scope, as designed: learning and evolving at the same time (online RL in live worlds), sexual reproduction (for example, chords producing offspring), voices that mutate, and a lineage-tree UI. Bodies also can't evolve beyond the range the policy saw in training, and foraging sopranos reach that edge within about 12,000 beats (see [Sample runs](#sample-runs)).

## Running

There are two supported ways to run the stack:

- **Local development (macOS, native).** `just dev` runs all four services as native processes (harmony as a release build, since training runs started from the UI drive it). `just train` runs a training run on MPS against its own release harmony, and `just evaluate <run ids>` compares runs with the builtin baselines. `melody` has to run natively here, because Docker on macOS runs Linux containers in a VM that has no access to Metal or MPS.
- **Containers (Linux, CPU).** Each service has a Dockerfile, and `compose.yaml` runs the full stack. `melody`'s image installs CPU-only PyTorch wheels from the PyTorch CPU index, using a `sys_platform == 'linux'` marker in `tool.uv.sources`, so the image doesn't pull in CUDA. macOS keeps the default PyPI wheels, which include MPS. Training also works in the container, just more slowly.

```sh
docker compose up --build        # build all four images and start the stack
open http://localhost:8000       # timbre (Caddy), which proxies /api to rhythm
docker compose logs -f melody    # follow one service
docker compose down              # stop; training runs persist in the melody-runs volume
```

Docker builds use the repository root as the build context, because every service generates code from `score/` at build time (`.dockerignore` keeps local builds, environments, runs, and generated code out). The images:

| Service   | Build stage                                    | Runtime image                                       | Listens on       |
| --------- | ---------------------------------------------- | --------------------------------------------------- | ---------------- |
| `harmony` | `rust:1.100.0-slim-trixie`, protox in build.rs | `debian:trixie-…-slim`, non-root                    | `0.0.0.0:50051`  |
| `melody`  | `python:3.14.10-slim-trixie` + uv, grpcio-tools | same Python image, non-root; runs in `/data/runs`  | `0.0.0.0:50052`  |
| `rhythm`  | `golang:1.27.4-trixie` + Debian `protoc`        | `distroless/static-debian13:nonroot`                | `0.0.0.0:8080`   |
| `timbre`  | `node:26.13.1-trixie-slim` + pnpm, `vite build` | `caddy:2.11.9-alpine` (static files, `/api` proxy, SSE flushing) | `:80` → host `8000` |

Only `timbre` publishes a port; the other services talk over the compose network by service name.

## Repository layout

Each part has its own README with its layout and commands: [score](score/README.md), [harmony](harmony/README.md), [melody](melody/README.md), [rhythm](rhythm/README.md), and [timbre](timbre/README.md).

```text
quartet/
├── score/          Internal gRPC contracts (package `quartet.score`)
├── harmony/        Rust world engine: simulation core and gRPC server
├── melody/         Python learning service: PPO training and inference
├── rhythm/         Go conductor: beat loops and the public REST + SSE API
├── timbre/         TypeScript frontend: 3D stage and dashboards
├── scripts/        Native run scripts: dev, bench, train, evaluate (via with-harmony.sh)
├── compose.yaml    The Linux CPU stack in containers
└── justfile        Cross-project commands: gen, check, test, dev, bench, train, evaluate
```

## Glossary

| Term        | Meaning                                                                          |
| ----------- | -------------------------------------------------------------------------------- |
| quartetling | One creature (agent).                                                            |
| voice       | A quartetling's role: soprano, alto, tenor, or bass.                             |
| performance | One live world, run by `rhythm` and shown in `timbre`.                           |
| rehearsal   | One training session: a stream of batched, parallel worlds in `harmony`.         |
| beat        | One simulation tick.                                                             |
| chord       | Three or more distinct voices singing together within range; grants a bonus.     |
| mote        | An energy particle.                                                              |
| static      | A hazard field that drains energy.                                               |
| census      | Aggregated world statistics computed by `harmony` (population, traits, density). |

Musical names are used for the components and the domain. Standard ML and systems terms, such as policy, checkpoint, and observation, are used for technical concepts.

## What's built

Everything in the original plan, in the order it was built:

- **Contracts and skeleton.** `score/` holds every internal gRPC contract, generated per language (`just gen`; protox in Rust's `build.rs`) and never committed. Each service has a standard gRPC health check, and `GET /api/ensemble` reports every service's version and runtime end to end.
- **World on stage.** harmony's deterministic world (SplitMix64, fixed slots, same-step respawn), three builtin policies, and live performances that rhythm drives beat by beat. Snapshots fan out to browsers over SSE as struct-of-arrays frames, encoded once per beat, with bounded drop-oldest subscriber buffers. timbre renders them as instanced three.js meshes, interpolated between beats, in light and dark themes.
- **Rehearsal.** One bidirectional `Rehearse` stream runs a rayon-parallel batch of worlds for training, with packed little-endian tensors and a Gymnasium-style client (`melody.rehearsal`). `just bench` measures it; see [Rehearsal throughput](#rehearsal-throughput).
- **Learning.** Hand-written CleanRL-style PPO (`melody/train.py`) on MPS or CPU. It writes checkpoints with config, seed, observation version, and Git revision. `Act` serves them to live performances in one batched forward pass per beat, and melody refuses version mismatches. `just train` trains, and `just evaluate` compares with the baselines.
- **Insight.** harmony's census (per-voice aggregates, trait and energy histograms, a density grid, and a bounded 10-beat history) behind `GET /api/performances/{id}/census`. Training runs can be started, watched, and stopped from the UI, with live curves over SSE. The charts are hand-written SVG with hover and keyboard readouts and table views.
- **Evolution.** Reproduction with mutation in performances, lineage tracking, and trait-drift, generation, and lineage charts. See [Evolution](#evolution).
- **Containers.** A Dockerfile per service and `compose.yaml` for the Linux CPU stack. See [Running](#running).

## To do

- **Population-based training or self-play between voices.** For example, train one policy per voice, or evolve PPO hyperparameters across a population of runs. This is also one way to approach the open question from [Sample runs](#sample-runs): learned policies don't sing yet.
- **Make quartetlings look like creatures on the stage.** Each body is currently a cone pointing along its heading (`timbre/src/stage/StageScene.ts`). Aim for a simple, recognizable creature rather than a detailed model, balanced against the frontend's rendering cost:
  - A few low-poly parts at most, such as a rounded body plus a fin or tail and eyes. Each part is one instanced mesh shared by all slots, never one mesh per creature, so draw calls stay constant as slots grow.
  - Motion, such as a tail sway or bob, comes from per-instance values (phase from the slot index, speed from velocity), updated in the existing per-frame matrix pass or a vertex shader, without skeletal animation or per-creature objects.
  - The heading stays readable at a glance, and parts keep the per-voice colors from the theme tokens.
  - Measure frame time on the largest supported slot count before and after, and keep the scene smooth on an integrated GPU.
