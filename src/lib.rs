use substreams_ethereum::pb::eth::v2 as eth;

#[substreams::handlers::map]
pub fn map_blocks(blk: eth::Block) -> Result<eth::Block, substreams::errors::Error> {
    Ok(blk)
}