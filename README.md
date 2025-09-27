# Aerodrome Substreams for Base

This is a Substreams package for streaming Aerodrome Finance data on the Base blockchain.

## Features

- **Swap Events**: Track token swaps on Aerodrome
- **Liquidity Events**: Monitor liquidity additions and removals
- **Governance Events**: Track voting and delegation activities
- **Reward Events**: Monitor reward distributions

## Usage

### Building

```bash
substreams build
```

### Running

```bash
# Test with Base mainnet
substreams run -e base-mainnet.streamingfast.io:443 substreams.yaml map_aerodrome_swaps --start-block 1000000 --stop-block +10
```

### Testing with Etherscan API

Use the provided Etherscan API key to test with real Base data:

```bash
# Get recent transactions for Aerodrome Router
curl "https://api.etherscan.io/v2/api?chainid=8453&module=account&action=txlist&address=0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43&startblock=0&endblock=99999999&page=1&offset=10&sort=desc&apikey=8X4YIZCEESWC88D8SNY16JH1SQ6FT2E2KK"

# Get recent transactions for Aerodrome Factory
curl "https://api.etherscan.io/v2/api?chainid=8453&module=account&action=txlist&address=0x420DD381b31aEf6683db6B902084cB0FFECe40Da&startblock=0&endblock=99999999&page=1&offset=10&sort=desc&apikey=8X4YIZCEESWC88D8SNY16JH1SQ6FT2E2KK"

# Get recent transactions for Aerodrome Voter
curl "https://api.etherscan.io/v2/api?chainid=8453&module=account&action=txlist&address=0x16613524e02ad97eDfeF371bC883F2F5d6C480A5&startblock=0&endblock=99999999&page=1&offset=10&sort=desc&apikey=8X4YIZCEESWC88D8SNY16JH1SQ6FT2E2KK"
```

## Contract Addresses

The following Aerodrome Finance contract addresses are used on Base:

- **Router**: `0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43`
- **Factory**: `0x420DD381b31aEf6683db6B902084cB0FFECe40Da`
- **Voter**: `0x16613524e02ad97eDfeF371bC883F2F5d6C480A5`
- **AERO Token**: `0x940181a94A35A4569E4529A3CDfB74e38FD98631`
- **Gauge Factory**: `0x35f35cA5B132CaDf2916BaB57639128eAC5bbcb5`

## Data Structures

### AerodromeSwap
- Block number, transaction hash, log index
- Sender, recipient addresses
- Token in/out addresses and amounts
- Price impact and pool address
- Timestamp

### AerodromeLiquidity
- Block number, transaction hash, log index
- User address and pool address
- Token A/B addresses and amounts
- Liquidity amount and action type
- Timestamp

### AerodromeGovernance
- Block number, transaction hash, log index
- Voter address and proposal ID
- Vote power and action type
- Timestamp

### AerodromeRewards
- Block number, transaction hash, log index
- User address and gauge address
- Reward token and amount
- Timestamp

## Publishing

```bash
substreams registry login
substreams registry publish
```

## License

MIT
