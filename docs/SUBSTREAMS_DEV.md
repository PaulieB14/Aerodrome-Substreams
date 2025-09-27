# Publishing to Substreams.dev

This guide explains how to publish your Aerodrome Substreams to the official Substreams registry.

## Prerequisites

1. **Substreams CLI**: Ensure you have the latest version
   ```bash
   curl -L https://get.streamingfast.io/substreams | bash
   substreams version
   ```

2. **Registry Account**: Sign up at https://substreams.dev

## Publishing Steps

### 1. Login to Registry
```bash
substreams registry login
```
Follow the prompts to authenticate with your substreams.dev account.

### 2. Verify Package
```bash
# Test your package locally first
substreams run substreams.yaml map_blocks --start-block 1000000 --stop-block +5

# Check package info
substreams info substreams.yaml
```

### 3. Publish to Registry
```bash
# Publish from current directory (contains substreams.yaml)
substreams registry publish

# Or specify the path explicitly
substreams registry publish ./substreams.yaml
```

### 4. Verify Publication
Visit https://substreams.dev to see your published package.

## Package Details

- **Name**: `aerodrome-substreams`
- **Version**: `v0.1.2`
- **Description**: Real-time Aerodrome Finance data streaming for Base blockchain
- **GitHub**: https://github.com/PaulieB14/Aerodrome-Substreams
- **Chain**: Base (Chain ID: 8453)
- **Network**: Base Mainnet

## Usage After Publishing

Once published, users can run your substreams directly from the registry:

```bash
# Run from registry
substreams run aerodrome-substreams@v0.1.2 map_blocks --start-block 1000000 --stop-block +10

# Or use the latest version
substreams run aerodrome-substreams@latest map_blocks --start-block 1000000 --stop-block +10
```

## Registry Benefits

- **Discoverability**: Users can find your substreams on substreams.dev
- **Versioning**: Automatic version management
- **Distribution**: No need to share files manually
- **Documentation**: Built-in package documentation
- **Community**: Share with the Substreams ecosystem

## Troubleshooting

### Authentication Issues
```bash
# Re-login if needed
substreams registry login
```

### Package Validation
```bash
# Validate before publishing
substreams run substreams.yaml map_blocks --start-block 1000000 --stop-block +1
```

### Version Conflicts
If you get version conflicts, increment the version in `substreams.yaml` and `Cargo.toml`.

## Next Steps

1. **Monitor Usage**: Check registry analytics
2. **Gather Feedback**: Community feedback on substreams.dev
3. **Iterate**: Update based on user feedback
4. **Promote**: Share in Substreams community channels

## Support

- **Registry**: https://substreams.dev
- **Documentation**: https://docs.streamingfast.io/substreams
- **Community**: Substreams Discord/Telegram
