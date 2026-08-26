//! Base B20 token detection.
//!
//! B20 is Base's ERC-20 superset for real-world assets. Coinbase's tokenized
//! stocks are B20 `Asset` tokens, which is why Aerodrome pools for them need to
//! be recognised as B20 markets.
//!
//! Every B20 is created by the singleton `IB20Factory` precompile at a
//! deterministic address:
//!
//! ```text
//! [ B20 prefix: 10 bytes ][ variant: 1 byte ][ bytes9(keccak256(deployer, salt)) ]
//! ```
//!
//! Detection therefore needs no allowlist and no state read: a new tokenized
//! asset is recognised the moment a pool for it is created. The variant is
//! recoverable from the address too, so `Asset` and `Stablecoin` tokens are told
//! apart without an RPC call.
//!
//! Verified against the live precompile at `0xB20f...0000`, which reports
//! `isB20 == true` for the 10-byte prefix with *any* variant byte (0x00, 0x01 and
//! 0x7f all match) and `false` as soon as the prefix itself differs. Matching on
//! 11 bytes -- prefix plus a hardcoded `Asset` variant byte -- would silently drop
//! every `Stablecoin` and every future variant.
//!
//! Spec: <https://github.com/base/base-std/tree/main/docs/B20>

/// Singleton `IB20Factory` precompile that creates every B20 token.
pub const B20_FACTORY: [u8; 20] = [
    0xb2, 0x0f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// `0xb2` followed by nine zero bytes -- the prefix shared by every B20 address.
pub const B20_ADDRESS_PREFIX: [u8; 10] = [0xb2, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Index of the variant byte within a B20 address.
const VARIANT_BYTE: usize = B20_ADDRESS_PREFIX.len();

/// True when `address` is a B20 token of any variant.
///
/// Mirrors `IB20Factory.isB20`, which recovers the answer from the address alone.
/// It attests that the address belongs to the B20 namespace -- not that any
/// particular issuer created it, since `createB20` is permissionless.
pub fn is_b20(address: &[u8]) -> bool {
    address.len() == 20 && address[..B20_ADDRESS_PREFIX.len()] == B20_ADDRESS_PREFIX
}

/// The raw variant byte of a B20 address.
pub fn variant_byte(address: &[u8]) -> Option<u8> {
    is_b20(address).then(|| address[VARIANT_BYTE])
}

/// Human name for a variant byte.
///
/// Unrecognised bytes are reported as `"unknown"` rather than rejected, so a
/// variant added in a later hardfork still indexes as a B20 instead of vanishing.
pub fn variant_name(variant: u8) -> &'static str {
    match variant {
        0 => "asset",
        1 => "stablecoin",
        _ => "unknown",
    }
}

/// Variant name for an address, or `""` when it is not a B20.
pub fn variant_of(address: &[u8]) -> &'static str {
    match variant_byte(address) {
        Some(v) => variant_name(v),
        None => "",
    }
}

/// Returns the B20 side of a pair, preferring `token0` when both sides are B20.
pub fn b20_side<'a>(token0: &'a [u8], token1: &'a [u8]) -> Option<&'a [u8]> {
    if is_b20(token0) {
        Some(token0)
    } else if is_b20(token1) {
        Some(token1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(hex_str: &str) -> Vec<u8> {
        hex::decode(hex_str.trim_start_matches("0x")).unwrap()
    }

    /// The 13 tokenized stocks Coinbase had issued at the Aerodrome launch.
    const KNOWN_STOCKS: [&str; 13] = [
        "b200000000000000000000C2e324d24d7eEcd1fb", // AAPLc
        "b200000000000000000000d9192b6B456483C2E8", // AMZNc
        "b200000000000000000000c85a31389D71F3ecfb", // COINc
        "B20000000000000000000019f6E7C675b73C2e4D", // CRCLc
        "b2000000000000000000002D0BA3164cc74f58B7", // GOOGLc
        "B2000000000000000000004AFF16039bA04bdFBc", // INTCc
        "b2000000000000000000008bC8786B856E61707C", // METAc
        "B200000000000000000000Ab99cFa739E253872B", // MSFTc
        "b2000000000000000000004884b426556b92883d", // MSTRc
        "b20000000000000000000078ee7ce2fE4908108C", // NVDAc
        "b200000000000000000000397293Cb8cda9a10c5", // SNDKc
        "b2000000000000000000007b9fcbd005511aCBd5", // SPCXc
        "b2000000000000000000001e800a7f5189430cD0", // TSLAc
    ];

    #[test]
    fn detects_every_issued_tokenized_stock_as_an_asset_variant() {
        for t in KNOWN_STOCKS {
            let a = addr(t);
            assert!(is_b20(&a), "{t} should be a B20");
            assert_eq!(variant_of(&a), "asset", "{t} should be the Asset variant");
        }
    }

    /// The precompile accepts any variant byte behind the prefix. Hardcoding the
    /// Asset byte would drop Stablecoin and every future variant.
    #[test]
    fn accepts_every_variant_byte() {
        let stablecoin = addr("b2000000000000000000010000000000000000ff");
        assert!(is_b20(&stablecoin));
        assert_eq!(variant_of(&stablecoin), "stablecoin");

        let future = addr("b20000000000000000007f0000000000000000ff");
        assert!(is_b20(&future), "a future variant must still index as B20");
        assert_eq!(variant_of(&future), "unknown");
    }

    #[test]
    fn rejects_addresses_outside_the_prefix() {
        // Confirmed false by the live precompile: the prefix itself differs.
        assert!(!is_b20(&addr("b2000000000000000001000000000000000000ff")));

        for t in [
            "833589fCD6eDb6E08f4c7C32D4f71b54bdA02913", // USDC
            "4200000000000000000000000000000000000006", // WETH
            "940181a94A35A4569E4529A3CDfB74e38FD98631", // AERO
            "0000000000000000000000000000000000000000",
        ] {
            assert!(!is_b20(&addr(t)), "{t} must not be detected as B20");
        }
    }

    #[test]
    fn rejects_bad_lengths() {
        assert!(!is_b20(&addr("b2000000000000000000")));
        assert!(!is_b20(&[]));
        assert_eq!(variant_byte(&[]), None);
    }

    #[test]
    fn picks_the_b20_side_of_a_pair() {
        let usdc = addr("833589fCD6eDb6E08f4c7C32D4f71b54bdA02913");
        let aapl = addr("b200000000000000000000C2e324d24d7eEcd1fb");
        assert_eq!(b20_side(&usdc, &aapl), Some(aapl.as_slice()));
        assert_eq!(b20_side(&aapl, &usdc), Some(aapl.as_slice()));
        assert_eq!(b20_side(&usdc, &usdc), None);
        // A B20/B20 pair resolves to token0 rather than reporting no B20 side.
        assert_eq!(b20_side(&aapl, &aapl), Some(aapl.as_slice()));
    }
}
