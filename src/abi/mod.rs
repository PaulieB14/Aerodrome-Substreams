// Generated ABI module for Aerodrome Pool events
pub mod pool {
    pub mod events {
        use substreams_ethereum::Event;

        #[derive(Debug, Clone, PartialEq)]
        pub struct Swap {
            pub sender: Vec<u8>,
            pub to: Vec<u8>,
            pub amount0_in: substreams::scalar::BigInt,
            pub amount1_in: substreams::scalar::BigInt,
            pub amount0_out: substreams::scalar::BigInt,
            pub amount1_out: substreams::scalar::BigInt,
        }

        impl Event for Swap {
            const NAME: &'static str = "Swap";

            fn match_log(log: &substreams_ethereum::pb::eth::v2::Log) -> bool {
                if log.topics.len() != 3 {
                    return false;
                }
                // Swap(address indexed sender, address indexed to, uint256 amount0In, uint256 amount1In, uint256 amount0Out, uint256 amount1Out)
                // keccak256("Swap(address,address,uint256,uint256,uint256,uint256)")
                log.topics[0].as_slice() == &[
                    0xd7, 0x8a, 0xd9, 0x5f, 0xa4, 0x6c, 0x99, 0x4b,
                    0x6e, 0x1f, 0x56, 0x21, 0x3a, 0x60, 0x3c, 0xc6,
                    0x6e, 0x23, 0x23, 0xba, 0x6f, 0x7b, 0x2d, 0x7f,
                    0xbb, 0x7f, 0xed, 0x15, 0x75, 0x49, 0xdf, 0x08,
                ]
            }

            fn decode(log: &substreams_ethereum::pb::eth::v2::Log) -> Result<Self, String> {
                if !Self::match_log(log) {
                    return Err("Log does not match Swap event".to_string());
                }

                let sender = log.topics.get(1)
                    .ok_or("Missing topic 1")?
                    .get(12..32)
                    .ok_or("Invalid topic 1 length")?
                    .to_vec();
                let to = log.topics.get(2)
                    .ok_or("Missing topic 2")?
                    .get(12..32)
                    .ok_or("Invalid topic 2 length")?
                    .to_vec();

                let data = &log.data;
                if data.len() < 128 {
                    return Err("Data too short for Swap event".to_string());
                }

                let amount0_in = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[0..32]);
                let amount1_in = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[32..64]);
                let amount0_out = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[64..96]);
                let amount1_out = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[96..128]);

                Ok(Swap {
                    sender,
                    to,
                    amount0_in,
                    amount1_in,
                    amount0_out,
                    amount1_out,
                })
            }
        }

        #[derive(Debug, Clone, PartialEq)]
        pub struct Mint {
            pub sender: Vec<u8>,
            pub to: Vec<u8>,
            pub amount0: substreams::scalar::BigInt,
            pub amount1: substreams::scalar::BigInt,
        }

        impl Event for Mint {
            const NAME: &'static str = "Mint";

            fn match_log(log: &substreams_ethereum::pb::eth::v2::Log) -> bool {
                if log.topics.len() != 3 {
                    return false;
                }
                // Mint(address indexed sender, uint256 amount0, uint256 amount1, address indexed to)
                // keccak256("Mint(address,uint256,uint256,address)")
                log.topics[0].as_slice() == &[
                    0x4c, 0x20, 0x9b, 0x5f, 0xc8, 0xad, 0x50, 0x75,
                    0x8f, 0x13, 0xe2, 0xe1, 0x08, 0x8b, 0xa5, 0x6a,
                    0x56, 0x0d, 0xfe, 0x89, 0xc3, 0x0b, 0xa0, 0x8b,
                    0x7c, 0x57, 0x4a, 0x69, 0x63, 0x54, 0xae, 0x36,
                ]
            }

            fn decode(log: &substreams_ethereum::pb::eth::v2::Log) -> Result<Self, String> {
                if !Self::match_log(log) {
                    return Err("Log does not match Mint event".to_string());
                }

                let sender = log.topics.get(1)
                    .ok_or("Missing topic 1")?
                    .get(12..32)
                    .ok_or("Invalid topic 1 length")?
                    .to_vec();
                let to = log.topics.get(2)
                    .ok_or("Missing topic 2")?
                    .get(12..32)
                    .ok_or("Invalid topic 2 length")?
                    .to_vec();

                let data = &log.data;
                if data.len() < 64 {
                    return Err("Data too short for Mint event".to_string());
                }

                let amount0 = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[0..32]);
                let amount1 = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[32..64]);

                Ok(Mint {
                    sender,
                    to,
                    amount0,
                    amount1,
                })
            }
        }

        #[derive(Debug, Clone, PartialEq)]
        pub struct Burn {
            pub sender: Vec<u8>,
            pub to: Vec<u8>,
            pub amount0: substreams::scalar::BigInt,
            pub amount1: substreams::scalar::BigInt,
        }

        impl Event for Burn {
            const NAME: &'static str = "Burn";

            fn match_log(log: &substreams_ethereum::pb::eth::v2::Log) -> bool {
                if log.topics.len() != 3 {
                    return false;
                }
                // Burn(address indexed sender, uint256 amount0, uint256 amount1, address indexed to)
                // keccak256("Burn(address,uint256,uint256,address)")
                log.topics[0].as_slice() == &[
                    0xdc, 0xcd, 0x41, 0x2f, 0x0b, 0x12, 0x52, 0x81,
                    0x90, 0xce, 0x99, 0x95, 0xb5, 0x30, 0x9c, 0x21,
                    0x02, 0x29, 0x02, 0xf9, 0x3a, 0x30, 0x11, 0x56,
                    0x9d, 0x3f, 0x53, 0xf3, 0x74, 0x32, 0x00, 0xef,
                ]
            }

            fn decode(log: &substreams_ethereum::pb::eth::v2::Log) -> Result<Self, String> {
                if !Self::match_log(log) {
                    return Err("Log does not match Burn event".to_string());
                }

                let sender = log.topics.get(1)
                    .ok_or("Missing topic 1")?
                    .get(12..32)
                    .ok_or("Invalid topic 1 length")?
                    .to_vec();
                let to = log.topics.get(2)
                    .ok_or("Missing topic 2")?
                    .get(12..32)
                    .ok_or("Invalid topic 2 length")?
                    .to_vec();

                let data = &log.data;
                if data.len() < 64 {
                    return Err("Data too short for Burn event".to_string());
                }

                let amount0 = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[0..32]);
                let amount1 = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[32..64]);

                Ok(Burn {
                    sender,
                    to,
                    amount0,
                    amount1,
                })
            }
        }

        #[derive(Debug, Clone, PartialEq)]
        pub struct Sync {
            pub reserve0: substreams::scalar::BigInt,
            pub reserve1: substreams::scalar::BigInt,
        }

        impl Event for Sync {
            const NAME: &'static str = "Sync";

            fn match_log(log: &substreams_ethereum::pb::eth::v2::Log) -> bool {
                if log.topics.len() != 1 {
                    return false;
                }
                // Sync(uint112 reserve0, uint112 reserve1)
                // keccak256("Sync(uint112,uint112)")
                log.topics[0].as_slice() == &[
                    0x1c, 0x41, 0x1e, 0x9a, 0x96, 0xe0, 0x71, 0x24,
                    0x1c, 0x2f, 0x21, 0xf7, 0x72, 0x6b, 0x17, 0xae,
                    0x89, 0xe3, 0xca, 0xb4, 0xc7, 0x8b, 0xe5, 0x05,
                    0x09, 0xa0, 0xfa, 0x21, 0x12, 0x56, 0x61, 0x17,
                ]
            }

            fn decode(log: &substreams_ethereum::pb::eth::v2::Log) -> Result<Self, String> {
                if !Self::match_log(log) {
                    return Err("Log does not match Sync event".to_string());
                }

                let data = &log.data;
                if data.len() < 64 {
                    return Err("Data too short for Sync event".to_string());
                }

                let reserve0 = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[0..32]);
                let reserve1 = substreams::scalar::BigInt::from_unsigned_bytes_be(&data[32..64]);

                Ok(Sync {
                    reserve0,
                    reserve1,
                })
            }
        }
    }
}
