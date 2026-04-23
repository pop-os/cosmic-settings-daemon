// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use cosmic_settings_audio_core::Event;
use tokio_util::bytes::Buf;

const MAX: usize = 8 * 1024 * 1024;

pub struct EventDecoder;

impl tokio_util::codec::Decoder for EventDecoder {
    type Item = Event;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut tokio_util::bytes::BytesMut,
    ) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            // Not enough data to read length marker.
            return Ok(None);
        }

        // Read length marker.
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;

        // Check that the length is not too large to avoid a denial of
        // service attack where the server runs out of memory.
        if length > MAX {
            return Err(Error::IO(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", length),
            )));
        }

        if src.len() < 4 + length {
            // The full string has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            src.reserve(4 + length - src.len());

            // We inform the Framed that we need more bytes to form the next
            // frame.
            return Ok(None);
        }

        // Use advance to modify src such that it no longer contains
        // this frame.
        let data = src[4..4 + length].to_vec();
        src.advance(4 + length);

        ron::de::from_bytes(&data).map(Some).map_err(Error::Ron)
    }
}

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    Ron(ron::de::SpannedError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ron(why) => write!(f, "RON deserialize error: {why}"),
            Self::IO(why) => write!(f, "I/O stream error: {why}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::IO(error)
    }
}
