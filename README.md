# Aerodrome Substreams (Base)

Stream Aerodrome Finance on Base: swaps, liquidity, governance, rewards.

## Quickstart (5 min)

### 0) Requirements
- macOS/Linux
- `substreams` CLI installed:
  ```bash
  curl -L https://get.streamingfast.io/substreams | bash
  substreams version
  ```

(Optional) StreamingFast API key in `SF_API_TOKEN`

Base endpoint: `base-mainnet.streamingfast.io:443`

### 1) Run blocks module
```bash
substreams run \
  -e base-mainnet.streamingfast.io:443 \
  substreams.yaml map_blocks \
  --start-block 10000000 --stop-block +1000 \
  --final-blocks-only
```

### 2) Sample output
```json
{
  "block": {
    "number": 36000000,
    "hash": "0x4fe3c6dc269e55be5b92720052af33a58ba8e5f67dfd2e66cc00676eacd9b1e9",
    "timestamp": "2024-01-15T10:30:00Z",
    "gasUsed": "21000000",
    "transactions": [
      {
        "hash": "0xa000251ee3f28db910c058135cf09694506279f94fbf4f7306d72bbd14a227d3",
        "from": "0x9cc9e01ed66b3c5ffbeb33029b6b08663b6de88b",
        "to": "0x9ea798232fe94c1ae7defb1cc7443560e275e652",
        "value": "0x071e772967aa",
        "gasUsed": "21000",
        "status": "SUCCEEDED"
      }
    ]
  }
}
```

## Modules

| Name | Kind | Input | Output | Description |
|------|------|-------|--------|-------------|
| `map_blocks` | map | `sf.ethereum.type.v2.Block` | `sf.ethereum.type.v2.Block` | Process Base blockchain blocks |

Protobufs in `proto/aerodrome/*.proto`. Generated types under `pb/`.

## Contract addresses (Base)

- **Router**: `0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43`
- **Factory**: `0x420DD381b31aEf6683db6B902084cB0FFECe40Da`
- **Voter**: `0x16613524e02ad97eDfeF371bC883F2F5d6C480A5`
- **AERO**: `0x940181a94A35A4569E4529A3CDfB74e38FD98631`
- **Gauge Factory**: `0x35f35cA5B132CaDf2916BaB57639128eAC5bbcb5`

## Choosing start blocks

Aerodrome deployed around block `10000000`. For full history:

```bash
--start-block 10000000
```

For tailing:

```bash
--start-block -100000
```

## Pipe to a sink

### Postgres:
```bash
substreams-sink-sql \
  --db-url postgres://user:pass@localhost:5432/aero \
  --schema-file db/schema.sql \
  -e base-mainnet.streamingfast.io:443 \
  substreams.yaml map_blocks
```

### Files (NDJSON):
```bash
substreams-sink-files \
  --output ./out \
  -e base-mainnet.streamingfast.io:443 \
  substreams.yaml map_blocks
```

## Etherscan helpers
```bash
export ETHERSCAN_API_KEY=YOUR_KEY
curl "https://api.etherscan.io/v2/api?chainid=8453&module=account&action=txlist&address=0xcF77a3B...&page=1&offset=10&sort=desc&apikey=$ETHERSCAN_API_KEY"
```

## Troubleshooting

- **Empty output**: wrong start range or module name → try a larger `--start-block` window.
- **Permission denied**: set `SF_API_TOKEN` if required by your endpoint.
- **Decode issues**: ensure ABIs match the listed contracts/versions.

## License

MIT
