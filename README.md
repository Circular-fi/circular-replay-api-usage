# circular-replay-api-usage

Rust client and LiteSVM replay helpers for the **Circular Pro replay API** ([documentation](https://pro.circular.fi/documentation)).

## API

| | |
|---|---|
| Base URL | `https://pro.circular.fi` — set `CIRCULAR_REPLAY_URL` in `.env` |
| Auth | `X-API-KEY: <key>` on every `/v2/replay/*` call |
| Compression | Client sends `Accept-Encoding: zstd, gzip` |

Endpoints covered by `CircularReplayClient`:

| Method | Path | Example |
|---|---|---|
| `GET` | `/v2/replay/status` | `status` |
| `GET` | `/v2/replay/tx/{signature}` | `fetch_replay`, `compare_rpc`, `replay_from_api` |
| `GET` | `/v2/replay/slot/{slot}` | `replay_slot` |
| `GET` | `/v2/replay/slot/{slot}/entries` | `slot_entries` |

Responses are wrapped as `{ success, data, … }`; the client unwraps `data`. Replay payloads include `pre`/`post`, `programs_pre`, `execution`, `epoch_schedule`, `rent`, etc. Examples default to **`program_details=true`** so fixtures are self-contained for LiteSVM.

## Setup

```bash
cp .env.example .env
# set CIRCULAR_REPLAY_API_KEY
cargo build
```

Environment: `CIRCULAR_REPLAY_URL`, `CIRCULAR_REPLAY_API_KEY`, `RPC_URL` (for `compare_rpc` only).

Optional mainnet ELF overrides: `programs/` or `REPLAY_PROGRAMS_DIR`.

## Examples

```bash
export CIRCULAR_REPLAY_API_KEY=your_key

cargo run --example fetch_replay
cargo run --example compare_rpc
cargo run --example status
cargo run --example replay_from_api -- --signature '<base58>' --bonus-account <pubkey>
cargo run --example replay_from_file -- -f fixtures/replay-circular-….json
cargo run --example replay_slot -- --slot 465589364 --account <pubkey>
cargo run --example slot_entries -- --slot 465589364
```

## Library

```rust
use circular_replay::{CircularReplayClient, CircularReplayQuery};

let client = CircularReplayClient::new("https://pro.circular.fi", api_key)?;

let status = client.status().await?;
let snapshot = client.replay(signature, &CircularReplayQuery::default()).await?;
```

## Layout

```
fixtures/     replay-circular-*.json
examples/     fetch_replay, compare_rpc, status, replay_from_api,
              replay_from_file, replay_slot, slot_entries
programs/     optional .so overrides (Memo, Token, …)
src/api/      CircularReplayClient (/v2/replay/*)
src/replay/   snapshot, LiteSVM, RPC compare
```
