# circular-replay-api-usage

Rust client and LiteSVM replay helpers for the **Circular Replay API**.

## API

| | |
|---|---|
| Base URL | `http://localhost:8080` — set `CIRCULAR_REPLAY_URL` in `.env` |
| Auth | `X-Api-Key: <key>` on every `/v1/*` call |
| Compression | Client sends `Accept-Encoding: zstd, gzip` |

Endpoints covered by the client (`CircularReplayClient`):

| Method | Path | Auth | Example |
|---|---|---|---|
| `GET` | `/status` | no | `status` |
| `GET` | `/v1/replay/{signature}` | yes | `fetch_replay`, `compare_rpc` |
| `POST` | `/v1/replay` | yes | `replay_from_api` (supports up to 100 `bonus_accounts`) |
| `POST` | `/v1/replay-slot` | yes | `replay_slot` |
| `POST` | `/v1/slot-entries` | yes | `slot_entries` |

Replay responses include `pre`/`post`, `programs_pre`, `execution`, `epoch_schedule`, `rent`, etc. Program bytecode mode is `ref` | `inline` | `none` ([`ProgramsMode`]); the examples default to **`inline`** so saved fixtures are self-contained and replayable without `/v1/programs` hydration.

## Setup

```bash
cp .env.example .env
# set CIRCULAR_REPLAY_API_KEY
cargo build
```

Environment: `CIRCULAR_REPLAY_URL`, `CIRCULAR_REPLAY_API_KEY`, `RPC_URL` (for `compare_rpc` only).

Optional mainnet ELF overrides: `programs/` or `REPLAY_PROGRAMS_DIR`. Constants: `SPL_TOKEN_PROGRAM`, `SPL_MEMO_PROGRAM`, …

## `fetch_replay` — download a snapshot and save under `fixtures/`

By default writes **`fixtures/replay-circular-<first 16 chars of signature>.json`** (full replay JSON). Reuse that file with `compare_rpc -f …`.

```bash
export CIRCULAR_REPLAY_API_KEY=your_key

cargo run --example fetch_replay
cargo run --example fetch_replay -- --signature '<base58>'
cargo run --example fetch_replay -- -o fixtures/my-replay.json
cargo run --example fetch_replay -- --no-write

# API flags (default: alts + spl enabled)
cargo run --example fetch_replay -- --signature '<base58>' --alts --spl
cargo run --example fetch_replay -- --signature '<base58>' --no-alts --no-spl

# override host (otherwise .env / default localhost:8080)
cargo run --example fetch_replay -- --circular-url http://localhost:8080
```

## `compare_rpc` — LiteSVM vs `getTransaction`

Loads a snapshot (from the API or `-f`), prepares LiteSVM without RPC sysvars, runs the tx. **The transaction bytes still come from `getTransaction`** (logs / compute-units comparison only).

```bash
export CIRCULAR_REPLAY_API_KEY=your_key

# fetch from API then compare
cargo run --example compare_rpc -- --signature '<base58>'

# from a fixture saved by fetch_replay (filename = first 16 chars of signature)
cargo run --example compare_rpc -- -f fixtures/replay-circular-4HqcGERuLJKvnKda.json

# custom RPC
RPC_URL=https://your-rpc.example.com cargo run --example compare_rpc -- --signature '<base58>'

cargo run --example compare_rpc -- --signature '<base58>' --alts --spl
cargo run --example compare_rpc -- --circular-url http://localhost:8080
```

## Endpoint examples

All default to `CIRCULAR_REPLAY_URL` (`.env` / `http://localhost:8080`) and `CIRCULAR_REPLAY_API_KEY`.

```bash
# GET /status — queryable slot window (no API key)
cargo run --example status
cargo run --example status -- -q

# POST /v1/replay — with optional bonus accounts (up to 100)
cargo run --example replay_from_api -- --signature '<base58>'
cargo run --example replay_from_api -- --signature '<base58>' \
    --bonus-account <pubkey> --bonus-account <pubkey>
cargo run --example replay_from_api -- --signature '<base58>' --programs ref -o fixtures/my.json

# inspect a saved fixture offline (no gateway call)
cargo run --example replay_from_file -- -f fixtures/replay-circular-4HqcGERuLJKvnKda.json

# POST /v1/replay-slot — account states at the start of a slot (end of slot N = ask N+1)
cargo run --example replay_slot -- --slot 461775917 \
    --account Harmonic8SoAfZixJqUxxdSeyWzr6q7XFjuiXS5Z2zPg

# POST /v1/slot-entries — signatures of a slot, grouped by entry
cargo run --example slot_entries -- --slot 461775917
```

## Library

```rust
use circular_replay::{CircularReplayClient, CircularReplayQuery, CircularReplayRequest};

let client = CircularReplayClient::new(base_url, api_key)?;

// GET /v1/replay/{signature}
let snapshot = client.replay(signature, &CircularReplayQuery::default()).await?;

// POST /v1/replay with bonus accounts
let req = CircularReplayRequest::new(signature)
    .with_bonus_accounts(vec![pubkey_a, pubkey_b]);
let snapshot = client.replay_with_accounts(&req).await?;
```

## API errors

JSON body `{ "error": "<code>", "message": "…" }`. Common codes: `unauthorized`, `transaction_not_found`, `slot_not_yet_indexed` (503), `slot_purged` (410), `too_many_requests` (429).

## Layout

```
fixtures/     replay-circular-*.json
examples/     fetch_replay, compare_rpc, status, replay_from_api,
              replay_from_file, replay_slot, slot_entries
programs/     optional .so overrides (Memo, Token, …)
src/api/      CircularReplayClient (GET/POST replay, replay-slot, slot-entries, status)
src/replay/   snapshot, LiteSVM, RPC compare
```
