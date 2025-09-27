# Deployment Guide

## Production Deployment

### 1. Build for Production
```bash
cargo build --release --target wasm32-unknown-unknown
```

### 2. Deploy to Substreams Registry
```bash
# Login to registry
substreams registry login

# Publish your substreams
substreams registry publish
```

### 3. Deploy to StreamingFast
```bash
# Set your API token
export SUBSTREAMS_API_TOKEN="your_token_here"

# Deploy with specific configuration
substreams run -e base-mainnet.streamingfast.io:443 substreams.yaml map_blocks
```

## Environment Variables

- `SUBSTREAMS_API_TOKEN`: Your StreamingFast API token
- `SF_API_TOKEN`: Alternative token variable
- `BASE_RPC_URL`: Custom Base RPC endpoint

## Monitoring

### Health Checks
```bash
# Check substreams status
substreams run -e base-mainnet.streamingfast.io:443 substreams.yaml map_blocks --start-block -1 --stop-block +1
```

### Performance Monitoring
- Monitor processing speed (blocks/second)
- Track memory usage
- Check error rates
- Monitor data quality

## Scaling

### Horizontal Scaling
- Run multiple substreams instances
- Use load balancers
- Distribute across regions

### Vertical Scaling
- Increase memory allocation
- Optimize gas usage
- Tune processing parameters

## Troubleshooting

### Common Issues
1. **Authentication errors**: Check API token
2. **Network issues**: Verify RPC endpoint
3. **Memory issues**: Increase allocation
4. **Performance issues**: Check start block range

### Logs
```bash
# Enable verbose logging
substreams run -e base-mainnet.streamingfast.io:443 substreams.yaml map_blocks --verbose
```

## Security

- Keep API tokens secure
- Use environment variables
- Rotate tokens regularly
- Monitor access logs
