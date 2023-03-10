//! Define the hash algorithm and result used in this board.
//! This is all boilerplate.

use serde::{Serialize, Serializer, Deserialize, Deserializer, de};
use serde::de::Visitor;
use std::fmt;
use std::fmt::{Display, Formatter, Debug};
use std::str::FromStr;

/// The error type for decoding a string into HashValue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FromHashValueError {
    /// The string provided was not a valid hex string.
    InvalidHexString,
    /// The string was not exactly 64 characters long.
    InvalidLength,
}

impl std::error::Error for FromHashValueError {}

impl Display for FromHashValueError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            FromHashValueError::InvalidHexString => write!(f, "Invalid hex string"),
            FromHashValueError::InvalidLength => write!(f, "Nash length should be 64 hex characters"),
        }
    }
}


/// # Hash result
/// This is really just a fixed length array of bytes, but this can be annoying to serialize to JSON as an array of numbers.
/// So the main purpose of this wrapper is to allow serialization as a hex string, like that used by
/// the program "sha256sum" or its ilk.
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub struct HashValue(pub [u8;32]);

impl FromStr for HashValue {
    type Err = FromHashValueError;

    fn from_str(v: &str) -> Result<Self, Self::Err> {
        if v.len()==64 {
            let mut res = [0;32];
            match hex::decode_to_slice(v,&mut res) {
                Ok(_) => Ok(HashValue(res)),
                Err(_) => Err(FromHashValueError::InvalidHexString)
            }
        } else {
            Err(FromHashValueError::InvalidLength)
        }
    }
}
impl Display for HashValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}",&hex::encode(&self.0))
    }
}

impl Debug for HashValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}",&hex::encode(&self.0))
    }
}


/// Serialize an array of bytes as a string of the hexadecimal representation, as used in the "sha256sum" program.
impl Serialize for HashValue {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error> where S: Serializer {
        serializer.serialize_str(&hex::encode(&self.0))
    }
}

/// Serialize an array of bytes as a string of the hexadecimal representation, as used in the "sha256sum" program.
impl <'de> Deserialize<'de> for HashValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error> where D: Deserializer<'de> {
        deserializer.deserialize_str(HashValueVisitor)
    }
}


/// Utility to do the work of deserialization.
struct HashValueVisitor;
impl<'de> Visitor<'de> for HashValueVisitor {
    type Value = HashValue;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("a 64 character hexadecimal string")
    }

    /// called when a hex string is encountered.
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error, {
        HashValue::from_str(v).map_err(|s|E::custom(s))
    }
}

/// Parse a string of semicolon separated hash values
pub fn parse_string_to_hash_vec(s:&str) -> Result<Vec<HashValue>,FromHashValueError> {
    let mut res = vec![];
    if s.len()>0 {
        for s_hash in s.split(';') {
            res.push(HashValue::from_str(s_hash)?)
        }
    }
    Ok(res)
}
