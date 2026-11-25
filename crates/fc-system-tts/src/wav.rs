//! The smallest WAV reader that can tell a playable file from anything else: RIFF/WAVE header,
//! a `fmt ` chunk with a sane format, and a non-empty `data` chunk. The system voice's output is
//! checked with it before it is handed to the media store (a broken file must not be stored as
//! a card's sound).

/// What a valid WAV file says about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    /// `1` = PCM, `3` = IEEE float, `0xFFFE` = extensible…
    pub format: u16,
    /// Channels (≥ 1).
    pub channels: u16,
    /// Samples per second (≥ 1).
    pub sample_rate: u32,
    /// Bits per sample.
    pub bits_per_sample: u16,
    /// Bytes of audio in the `data` chunk (≥ 1; clipped to what the file really holds).
    pub data_len: u32,
}

impl WavInfo {
    /// Length of the audio in milliseconds (0 when the format has no fixed frame size).
    pub fn duration_ms(&self) -> u64 {
        let frame = u64::from(self.channels) * u64::from(self.bits_per_sample).div_ceil(8);
        if frame == 0 {
            return 0;
        }
        u64::from(self.data_len) / frame * 1000 / u64::from(self.sample_rate)
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Reads the header of a WAV file. `Err` says in plain English why it is not one.
pub fn parse(bytes: &[u8]) -> Result<WavInfo, &'static str> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file");
    }
    let mut at = 12usize;
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32_at(bytes, at + 4).ok_or("truncated chunk header")?;
        let body = at + 8;
        if id == b"fmt " {
            let format = u16_at(bytes, body).ok_or("truncated fmt chunk")?;
            let channels = u16_at(bytes, body + 2).ok_or("truncated fmt chunk")?;
            let rate = u32_at(bytes, body + 4).ok_or("truncated fmt chunk")?;
            let bits = u16_at(bytes, body + 14).ok_or("truncated fmt chunk")?;
            if channels == 0 || rate == 0 {
                return Err("fmt chunk has no channels or no sample rate");
            }
            fmt = Some((format, channels, rate, bits));
        } else if id == b"data" {
            let (format, channels, sample_rate, bits_per_sample) =
                fmt.ok_or("data chunk before the fmt chunk")?;
            let available = u32::try_from(bytes.len() - body).unwrap_or(u32::MAX);
            // A streaming writer may leave the length at 0 or 0xFFFFFFFF: count what is there.
            let data_len = if len == 0 || len > available {
                available
            } else {
                len
            };
            if data_len == 0 {
                return Err("no audio in the data chunk");
            }
            return Ok(WavInfo {
                format,
                channels,
                sample_rate,
                bits_per_sample,
                data_len,
            });
        }
        // Chunks are padded to an even length.
        let next = usize::try_from(len)
            .ok()
            .and_then(|l| body.checked_add(l))
            .and_then(|e| e.checked_add(e & 1));
        match next {
            Some(n) => at = n,
            None => break,
        }
    }
    Err("no data chunk")
}

/// A mono 16-bit PCM WAV file of `samples` (tests and fakes).
pub fn pcm16_mono(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_pcm_file() {
        let wav = pcm16_mono(16_000, &[0, 100, -100, 0].repeat(4000));
        let info = parse(&wav).unwrap();
        assert_eq!(
            (info.format, info.channels, info.sample_rate),
            (1, 1, 16_000)
        );
        assert_eq!(info.data_len, 32_000);
        assert_eq!(info.duration_ms(), 1000);
    }

    #[test]
    fn skips_unknown_chunks_and_counts_a_streamed_length() {
        let mut wav = pcm16_mono(8000, &[1, 2, 3]);
        // A LIST chunk (odd length → one pad byte) between fmt and data.
        let list = [b"LIST".as_slice(), &3u32.to_le_bytes(), b"abc\0"].concat();
        wav.splice(36..36, list);
        let data_len_at = wav.len() - 6 - 4;
        wav[data_len_at..data_len_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&wav).unwrap().data_len, 6);
    }

    #[test]
    fn refuses_what_is_not_playable() {
        assert!(parse(b"").is_err());
        assert!(parse(b"ID3\x04 not a wav at all").is_err());
        assert_eq!(
            parse(&pcm16_mono(8000, &[])),
            Err("no audio in the data chunk")
        );
        let mut no_rate = pcm16_mono(8000, &[1]);
        no_rate[24..28].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse(&no_rate).is_err());
        let wav = pcm16_mono(8000, &[1, 2]);
        assert_eq!(parse(&wav[..30]), Err("truncated fmt chunk"));
        assert_eq!(parse(&wav[..36]), Err("no data chunk"));
    }
}
