use rlp_codec::RlpError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("RLP encode failed: {0}")]
    Encode(RlpError),
    #[error("RLP decode failed: {0}")]
    Decode(#[from] RlpError),
    #[error("Unknown Message Type: {0}")]
    UnknownMessageType(u8),
    #[error("The size of the frame is too large: {0}")]
    FrameTooLarge(usize),
    #[error("Remote side closed the connection")]
    PeerDisconnected,
}
