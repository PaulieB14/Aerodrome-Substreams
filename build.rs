use anyhow::Result;
use substreams_ethereum::Abigen;

/// Generates typed event decoders from the ABI JSON in `abi/`.
///
/// Every decoder's topic0 is derived from the ABI at build time, so a signature
/// can no longer drift from the deployed contract the way a hand-written matcher can.
fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=abi");

    for (name, abi, out) in [
        ("Pool", "abi/pool.json", "src/abi/pool.rs"),
        ("PoolFactory", "abi/pool_factory.json", "src/abi/pool_factory.rs"),
        ("CLPool", "abi/cl_pool.json", "src/abi/cl_pool.rs"),
        ("CLFactory", "abi/cl_factory.json", "src/abi/cl_factory.rs"),
        ("Erc20", "abi/erc20.json", "src/abi/erc20.rs"),
        ("B20", "abi/b20.json", "src/abi/b20.rs"),
        ("B20Factory", "abi/b20_factory.json", "src/abi/b20_factory.rs"),
        ("FactoryRegistry", "abi/factory_registry.json", "src/abi/factory_registry.rs"),
    ] {
        Abigen::new(name, abi)?.generate()?.write_to_file(out)?;
    }

    Ok(())
}
