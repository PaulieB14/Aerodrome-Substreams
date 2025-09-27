# Aerodrome Finance on Base - Transaction Analysis

## Overview
This document analyzes real transaction data from Aerodrome Finance contracts on Base to understand the event patterns and data structures needed for the substreams implementation.

## Contract Analysis Results

### 1. Router Contract (`0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43`)
**Key Functions Observed:**
- `swapExactTokensForTokens` - Token-to-token swaps
- `swapExactETHForTokens` - ETH-to-token swaps  
- `swapExactTokensForETH` - Token-to-ETH swaps
- `addLiquidityETH` - Adding liquidity with ETH
- `addLiquidity` - Adding liquidity with two tokens

**Event Patterns:**
- High transaction volume (recent blocks show multiple swaps)
- Mix of successful and failed transactions
- Various token pairs being swapped
- Complex routing through multiple pools

### 2. Factory Contract (`0x420DD381b31aEf6683db6B902084cB0FFECe40Da`)
**Key Functions Observed:**
- `createPool` - Creating new liquidity pools
- `swapExactETHForTokens` - Some swap functions routed through factory

**Event Patterns:**
- Lower transaction volume (pool creation is less frequent)
- Pool creation involves tokenA, tokenB, and stable flag
- Some failed transactions (likely due to pool already existing)

### 3. Voter Contract (`0x16613524e02ad97eDfeF371bC883F2F5d6C480A5`)
**Key Functions Observed:**
- `vote` - Voting on pool weights with veAERO
- `claimBribes` - Claiming bribe rewards

**Event Patterns:**
- Governance-focused transactions
- Token ID-based voting system
- Bribe claiming with multiple tokens
- Weight-based voting system

### 4. AERO Token (`0x940181a94A35A4569E4529A3CDfB74e38FD98631`)
**Key Functions Observed:**
- `approve` - ERC-20 approvals
- `transfer` - ERC-20 transfers

**Event Patterns:**
- Standard ERC-20 token functions
- High approval amounts (likely for staking/voting)
- Regular transfer activity

### 5. Gauge Factory (`0x35f35cA5B132CaDf2916BaB57639128eAC5bbcb5`)
**Key Functions Observed:**
- `createGauge` - Creating gauges for pools

**Event Patterns:**
- Gauge creation for reward distribution
- Links to forwarder, pool, fees voting reward, and reward token
- Some failed transactions (likely duplicate gauge creation)

### 6. VotingEscrow (`0xeBf418Fe2512e7E6bd9b87a8F0f294aCDC67e6B4`)
**Key Functions Observed:**
- `increaseAmount` - Increasing veAERO lock amount
- `approve` - Approving veAERO operations

**Event Patterns:**
- veAERO lock management
- High-value approvals for voting operations
- Long-term locking mechanisms

### 7. Slipstream Pool Factory (`0x5e7BB104d84c7CB9B682AaC2F3d509f5F406809A`)
**Key Functions Observed:**
- `createPool` - Creating concentrated liquidity pools
- `mint` - Minting concentrated liquidity positions

**Event Patterns:**
- Concentrated liquidity pool creation
- Tick spacing and price parameters
- Position minting with specific ranges

## Event Signature Analysis

### Swap Events
- **Router Swaps**: Multiple swap functions with different input/output combinations
- **Event Signatures**: Need to identify specific event signatures for:
  - `Swap` events from pools
  - `AddLiquidity` events
  - `RemoveLiquidity` events

### Governance Events
- **Voting**: `vote` function calls with token ID, pool votes, and weights
- **Bribes**: `claimBribes` with multiple token arrays
- **veAERO**: Lock amount changes and approvals

### Liquidity Events
- **Pool Creation**: Factory `createPool` events
- **Gauge Creation**: Gauge factory `createGauge` events
- **Slipstream**: Concentrated liquidity pool creation and position minting

## Data Structure Requirements

### Swap Events
```protobuf
message AerodromeSwap {
  string block_number = 1;
  string transaction_hash = 2;
  string log_index = 3;
  string sender = 4;
  string recipient = 5;
  string token_in = 6;
  string token_out = 7;
  string amount_in = 8;
  string amount_out = 9;
  string price_impact = 10;
  string pool_address = 11;
  string timestamp = 12;
}
```

### Governance Events
```protobuf
message AerodromeGovernance {
  string block_number = 1;
  string transaction_hash = 2;
  string log_index = 3;
  string voter = 4;
  string token_id = 5;
  repeated string pool_votes = 6;
  repeated string weights = 7;
  string action = 8; // "vote" or "claim_bribes"
  string timestamp = 9;
}
```

### Liquidity Events
```protobuf
message AerodromeLiquidity {
  string block_number = 1;
  string transaction_hash = 2;
  string log_index = 3;
  string user = 4;
  string pool_address = 5;
  string token_a = 6;
  string token_b = 7;
  string amount_a = 8;
  string amount_b = 9;
  string liquidity = 10;
  string action = 11; // "add", "remove", or "create"
  string timestamp = 12;
}
```

## Key Insights

1. **High Activity**: Router contract shows the most activity with frequent swaps
2. **Complex Routing**: Many transactions involve multiple token pairs and complex routing
3. **Governance Active**: Regular voting and bribe claiming activity
4. **veAERO Locking**: Active veAERO lock management
5. **Slipstream Integration**: Concentrated liquidity functionality is being used
6. **Event Diversity**: Multiple event types across different contract functions

## Next Steps

1. **Event Signature Mapping**: Identify specific event signatures for each contract
2. **Data Parsing**: Implement proper parsing for complex transaction data
3. **Event Filtering**: Filter events by contract address and event signature
4. **Data Transformation**: Convert raw transaction data to structured protobuf messages
5. **Testing**: Test with real Base mainnet data using the identified patterns

## Contract Addresses Summary

- **Router**: `0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43`
- **Factory**: `0x420DD381b31aEf6683db6B902084cB0FFECe40Da`
- **Voter**: `0x16613524e02ad97eDfeF371bC883F2F5d6C480A5`
- **AERO Token**: `0x940181a94A35A4569E4529A3CDfB74e38FD98631`
- **Gauge Factory**: `0x35f35cA5B132CaDf2916BaB57639128eAC5bbcb5`
- **VotingEscrow**: `0xeBf418Fe2512e7E6bd9b87a8F0f294aCDC67e6B4`
- **Slipstream Pool Factory**: `0x5e7BB104d84c7CB9B682AaC2F3d509f5F406809A`
