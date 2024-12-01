use std::error::Error;

pub struct CartridgeInfo {
    pub title: String,
}

impl CartridgeInfo {
    pub fn parse(data: &[u8]) -> Result<CartridgeInfo, Box<dyn Error>> {
        assert!(data.len() >= 4);

        Ok(CartridgeInfo {
            title: std::str::from_utf8(&data[0xA0..0xA0 + 12])?.to_string(),
        })
    }
}
