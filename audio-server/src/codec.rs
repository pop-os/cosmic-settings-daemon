// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

pub struct EventCodec;

const MAX: usize = 8 * 1024 * 1024;

impl tokio_util::codec::Encoder<&[u8]> for EventCodec {
    type Error = std::io::Error;

    fn encode(
        &mut self,
        item: &[u8],
        dst: &mut tokio_util::bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        if item.len() > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame of length {} is too large.", item.len()),
            ));
        }

        dst.reserve(4 + item.len());
        dst.extend_from_slice(&u32::to_le_bytes(item.len() as u32));
        dst.extend_from_slice(item);
        Ok(())
    }
}
