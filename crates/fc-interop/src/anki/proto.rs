//! The tiny subset of protobuf wire format Anki's latest schema needs (config blobs, media
//! list, package metadata). Unknown fields are skipped.

use crate::error::{InteropError, InteropResult};

/// One decoded field value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed64(u64),
    Fixed32(u32),
}

fn bad(reason: &str) -> InteropError {
    InteropError::invalid("apkg", format!("protobuf: {reason}"))
}

fn varint(buf: &[u8], pos: &mut usize) -> InteropResult<u64> {
    let mut out: u64 = 0;
    for shift in (0..64).step_by(7) {
        let b = *buf.get(*pos).ok_or_else(|| bad("truncated varint"))?;
        *pos += 1;
        out |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(out);
        }
    }
    Err(bad("varint too long"))
}

/// All `(field number, value)` pairs of a message, in order.
pub(crate) fn decode(buf: &[u8]) -> InteropResult<Vec<(u32, Value<'_>)>> {
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < buf.len() {
        let key = varint(buf, &mut pos)?;
        let field = u32::try_from(key >> 3).map_err(|_| bad("field number"))?;
        let value = match key & 7 {
            0 => Value::Varint(varint(buf, &mut pos)?),
            1 => {
                let b = buf
                    .get(pos..pos + 8)
                    .ok_or_else(|| bad("truncated fixed64"))?;
                pos += 8;
                Value::Fixed64(u64::from_le_bytes(
                    b.try_into().map_err(|_| bad("fixed64"))?,
                ))
            }
            2 => {
                let len = usize::try_from(varint(buf, &mut pos)?).map_err(|_| bad("length"))?;
                let end = pos.checked_add(len).ok_or_else(|| bad("length"))?;
                let b = buf.get(pos..end).ok_or_else(|| bad("truncated bytes"))?;
                pos = end;
                Value::Bytes(b)
            }
            5 => {
                let b = buf
                    .get(pos..pos + 4)
                    .ok_or_else(|| bad("truncated fixed32"))?;
                pos += 4;
                Value::Fixed32(u32::from_le_bytes(
                    b.try_into().map_err(|_| bad("fixed32"))?,
                ))
            }
            _ => return Err(bad("unsupported wire type")),
        };
        out.push((field, value));
    }
    Ok(out)
}

/// Last varint value of field `n` (protobuf "last one wins").
pub(crate) fn varint_field(fields: &[(u32, Value<'_>)], n: u32) -> Option<u64> {
    fields.iter().rev().find_map(|(f, v)| match v {
        Value::Varint(x) if *f == n => Some(*x),
        _ => None,
    })
}

/// Last length-delimited value of field `n`.
pub(crate) fn bytes_field<'a>(fields: &[(u32, Value<'a>)], n: u32) -> Option<&'a [u8]> {
    fields.iter().rev().find_map(|(f, v)| match v {
        Value::Bytes(b) if *f == n => Some(*b),
        _ => None,
    })
}

/// Field `n` as UTF-8 text (empty if absent).
pub(crate) fn string_field(fields: &[(u32, Value<'_>)], n: u32) -> String {
    bytes_field(fields, n)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

/// Every length-delimited value of repeated field `n`.
pub(crate) fn repeated_bytes<'a>(fields: &[(u32, Value<'a>)], n: u32) -> Vec<&'a [u8]> {
    fields
        .iter()
        .filter_map(|(f, v)| match v {
            Value::Bytes(b) if *f == n => Some(*b),
            _ => None,
        })
        .collect()
}

/// Message encoder (test fixtures build the latest Anki format with it).
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Enc(pub Vec<u8>);

#[cfg(test)]
impl Enc {
    fn raw_varint(&mut self, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.0.push(b);
                return;
            }
            self.0.push(b | 0x80);
        }
    }

    pub(crate) fn varint(mut self, field: u32, v: u64) -> Self {
        self.raw_varint(u64::from(field) << 3);
        self.raw_varint(v);
        self
    }

    pub(crate) fn bytes(mut self, field: u32, b: &[u8]) -> Self {
        self.raw_varint((u64::from(field) << 3) | 2);
        self.raw_varint(b.len() as u64);
        self.0.extend_from_slice(b);
        self
    }

    pub(crate) fn string(self, field: u32, s: &str) -> Self {
        self.bytes(field, s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_what_it_encodes() {
        let inner = Enc::default().string(1, "hi").0;
        let msg = Enc::default()
            .varint(1, 300)
            .bytes(2, &inner)
            .string(3, "ü")
            .string(4, "a")
            .string(4, "b")
            .0;
        let f = decode(&msg).unwrap();
        assert_eq!(varint_field(&f, 1), Some(300));
        assert_eq!(
            string_field(&decode(bytes_field(&f, 2).unwrap()).unwrap(), 1),
            "hi"
        );
        assert_eq!(string_field(&f, 3), "ü");
        assert_eq!(repeated_bytes(&f, 4), vec![b"a".as_slice(), b"b"]);
        assert!(decode(&[0x08]).is_err());
        assert!(decode(&[0x0a, 0x05, 1]).is_err());
    }
}
