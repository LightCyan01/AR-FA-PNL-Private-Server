use std::{error::Error, sync::Arc};

use aes::Aes128;
use bytes::{Buf, Bytes};
use cbc::{Decryptor, Encryptor};
use cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use futures_util::TryStreamExt;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage};
use prost_types::FileDescriptorSet;
use sha2::{Digest, Sha256};
use thiserror::Error;

const SEED: [u8; 16] = [
    0x48, 0x7a, 0x99, 0x61, 0xc9, 0x47, 0xf7, 0xd9, 0x2e, 0xd6, 0xb7, 0x9f, 0xc0, 0x54, 0x5f, 0xea,
];
const IV: [u8; 16] = [
    0x65, 0xa9, 0x9b, 0x89, 0xa6, 0x34, 0xfc, 0xa3, 0x19, 0x3c, 0x52, 0x12, 0xe5, 0x21, 0x93, 0x78,
];

pub type BoxError = Box<dyn Error + Send + Sync>;
pub type AppBody = BoxBody<Bytes, BoxError>;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("invalid frame")]
    InvalidFrame,
    #[error("AES failure")]
    Cipher,
    #[error("protobuf decode failed: {0}")]
    Protobuf(#[from] prost::DecodeError),
    #[error("protobuf descriptor set failed: {0}")]
    Descriptor(String),
    #[error("unknown protobuf message: {0}")]
    UnknownMessage(String),
}

#[derive(Clone)]
pub struct ProtoRegistry {
    pool: Arc<DescriptorPool>,
}

impl ProtoRegistry {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TransportError> {
        let set = FileDescriptorSet::decode(bytes)?;
        let pool = DescriptorPool::from_file_descriptor_set(set)
            .map_err(|error| TransportError::Descriptor(error.to_string()))?;
        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    pub fn from_file(path: &std::path::Path) -> Result<Self, TransportError> {
        Self::from_bytes(
            &std::fs::read(path).map_err(|error| TransportError::Descriptor(error.to_string()))?,
        )
    }

    pub fn decode(&self, name: &str, bytes: &[u8]) -> Result<DynamicMessage, TransportError> {
        let descriptor = self
            .pool
            .get_message_by_name(name)
            .ok_or_else(|| TransportError::UnknownMessage(name.into()))?;
        Ok(DynamicMessage::decode(descriptor, bytes)?)
    }

    pub fn empty(&self, name: &str) -> Result<DynamicMessage, TransportError> {
        let descriptor = self
            .pool
            .get_message_by_name(name)
            .ok_or_else(|| TransportError::UnknownMessage(name.into()))?;
        Ok(DynamicMessage::new(descriptor))
    }
}

pub fn decrypt_frame(frame: &[u8]) -> Result<(u8, Vec<u8>), TransportError> {
    if frame.len() < 17 || !(frame.len() - 1).is_multiple_of(16) {
        return Err(TransportError::InvalidFrame);
    }
    let prefix = frame[0];
    let mut encrypted = frame[1..].to_vec();
    let decryptor = Decryptor::<Aes128>::new_from_slices(&derive_key(prefix), &IV)
        .map_err(|_| TransportError::Cipher)?;
    let plaintext = decryptor
        .decrypt_padded_mut::<Pkcs7>(&mut encrypted)
        .map_err(|_| TransportError::Cipher)?
        .to_vec();
    Ok((prefix, plaintext))
}

pub fn encrypt_frame(prefix: u8, plaintext: &[u8]) -> Result<Vec<u8>, TransportError> {
    let mut encrypted = vec![0u8; plaintext.len() + 16];
    encrypted[..plaintext.len()].copy_from_slice(plaintext);
    let encryptor = Encryptor::<Aes128>::new_from_slices(&derive_key(prefix), &IV)
        .map_err(|_| TransportError::Cipher)?;
    let output = encryptor
        .encrypt_padded_mut::<Pkcs7>(&mut encrypted, plaintext.len())
        .map_err(|_| TransportError::Cipher)?;
    let mut frame = Vec::with_capacity(1 + output.len());
    frame.push(prefix);
    frame.extend_from_slice(output);
    Ok(frame)
}

pub fn derive_key(prefix: u8) -> [u8; 16] {
    let mut source = SEED;
    if prefix > 0x7f {
        source.reverse();
    }
    let shift = prefix & 7;
    let offset = (prefix >> 3) as usize;
    let mut key = [0u8; 16];
    for (index, slot) in key.iter_mut().enumerate() {
        let left = source[(index + offset) & 15].wrapping_shl(shift as u32);
        let right = if shift == 0 {
            0
        } else {
            source[(index + offset + 1) & 15] >> (8 - shift)
        };
        *slot = left | right;
    }
    key
}

pub fn request_fingerprint(route: &str, plaintext: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(route.as_bytes());
    hasher.update([0]);
    hasher.update(plaintext);
    hasher.finalize().into()
}

pub fn response_prefix(policy: &str, request_prefix: Option<u8>) -> u8 {
    match policy {
        "request" => request_prefix.unwrap_or(0),
        _ => uuid::Uuid::new_v4().as_bytes()[0],
    }
}

pub fn full(bytes: impl Into<Bytes>) -> AppBody {
    Full::new(bytes.into())
        .map_err(|never| match never {})
        .boxed()
}

pub fn stream<S>(stream: S) -> AppBody
where
    S: futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static,
{
    let stream = stream
        .map_ok(http_body::Frame::data)
        .map_err(|error| Box::new(error) as BoxError);
    StreamBody::new(stream).boxed()
}

pub fn copy_buf<B: Buf + Clone>(buf: &B) -> Vec<u8> {
    let mut copy = buf.clone();
    let mut output = vec![0; copy.remaining()];
    copy.copy_to_slice(&mut output);
    output
}
