//! Media store primitives (docs/03 §13): content addressing and type detection. Pure: the
//! file I/O lives in `fc-storage`.

use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of `bytes`: the media id. Equal content → equal id → one file.
pub fn media_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    out
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Relative storage path of a media file: `media/<first two hex chars>/<hash>`.
pub fn media_rel_path(hash: &str) -> String {
    format!("media/{}/{hash}", hash.get(..2).unwrap_or("00"))
}

/// Broad media category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum MediaKind {
    /// Image.
    Image,
    /// Audio.
    Audio,
    /// Video.
    Video,
}

/// Where a stored audio or image file came from (v1.4 W4, `media_origins`, docs/03 §13).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum MediaOrigin {
    /// Captured from the film by the Subtitle extension.
    Original,
    /// Cut from a local video file (`reading.clips.extract`, `media.audio.fill`).
    Clip,
    /// Synthesised by a TTS provider.
    Tts,
    /// Added by the user (or of unknown origin).
    #[default]
    User,
}

impl MediaOrigin {
    /// Every origin.
    pub const ALL: [Self; 4] = [Self::Original, Self::Clip, Self::Tts, Self::User];

    /// Storage string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Clip => "clip",
            Self::Tts => "tts",
            Self::User => "user",
        }
    }

    /// From the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.as_str() == s)
    }

    /// Whether the sound/picture is the film's own (captured or cut from the video).
    pub const fn from_video(self) -> bool {
        matches!(self, Self::Original | Self::Clip)
    }
}

/// Text hash of a TTS cache entry: SHA-256 of the text with whitespace collapsed (case kept:
/// a voice may read "US" and "us" differently). The cache key is (provider, language, voice,
/// speed, this hash), so the same word is synthesised once.
pub fn tts_text_hash(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    media_hash(collapsed.as_bytes())
}

/// A TTS speed as an integer key (per mille: 1.0 → 1000), so equal speeds compare equal.
pub fn tts_rate_key(rate: f32) -> i64 {
    // Rates are validated to 0.25–4 before they get here.
    (f64::from(rate) * 1000.0).round() as i64
}

/// Detects the MIME type from the content (magic bytes), falling back to the file extension.
/// Returns `None` for unsupported content (only images, audio and video are stored).
pub fn detect_mime(bytes: &[u8], file_name: Option<&str>) -> Option<&'static str> {
    let starts = |sig: &[u8]| bytes.starts_with(sig);
    let sniffed = if starts(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if starts(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some("audio/wav")
    } else if starts(b"ID3") || (bytes.len() > 1 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0) {
        Some("audio/mpeg")
    } else if starts(b"OggS") {
        Some("audio/ogg")
    } else if starts(b"fLaC") {
        Some("audio/flac")
    } else if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        // ISO base media: m4a is audio, the rest video.
        Some(if &bytes[8..11] == b"M4A" {
            "audio/mp4"
        } else {
            "video/mp4"
        })
    } else if starts(b"\x1a\x45\xdf\xa3") {
        Some("video/webm")
    } else {
        None
    };
    sniffed.or_else(|| {
        let ext = file_name?.rsplit_once('.')?.1.to_ascii_lowercase();
        Some(match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "bmp" => "image/bmp",
            "avif" => "image/avif",
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "ogg" | "oga" | "opus" => "audio/ogg",
            "m4a" => "audio/mp4",
            "flac" => "audio/flac",
            "mp4" | "m4v" => "video/mp4",
            "webm" => "video/webm",
            _ => return None,
        })
    })
}

/// Category of a MIME type.
pub fn media_kind(mime: &str) -> Option<MediaKind> {
    match mime.split('/').next()? {
        "image" => Some(MediaKind::Image),
        "audio" => Some(MediaKind::Audio),
        "video" => Some(MediaKind::Video),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_round_trip_and_tts_keys_are_stable() {
        for o in MediaOrigin::ALL {
            assert_eq!(MediaOrigin::parse(o.as_str()), Some(o));
        }
        assert!(MediaOrigin::Clip.from_video() && !MediaOrigin::Tts.from_video());
        assert_eq!(tts_text_hash("pick  up\n"), tts_text_hash("pick up"));
        assert_ne!(tts_text_hash("Pick up"), tts_text_hash("pick up"));
        assert_eq!(tts_rate_key(1.0), 1000);
        assert_eq!(tts_rate_key(0.85), 850);
    }

    #[test]
    fn hash_is_sha256_hex() {
        assert_eq!(
            media_hash(b"test"),
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
        assert_eq!(
            media_rel_path("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"),
            "media/9f/9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[test]
    fn mime_detection() {
        assert_eq!(
            detect_mime(b"\x89PNG\r\n\x1a\nrest", None),
            Some("image/png")
        );
        assert_eq!(
            detect_mime(b"\xff\xd8\xff\xe0", Some("x.png")),
            Some("image/jpeg")
        );
        assert_eq!(detect_mime(b"ID3\x03", None), Some("audio/mpeg"));
        assert_eq!(
            detect_mime(b"RIFF\0\0\0\0WEBPVP8", None),
            Some("image/webp")
        );
        assert_eq!(detect_mime(b"plain", Some("song.OGG")), Some("audio/ogg"));
        assert_eq!(detect_mime(b"<script>", Some("x.html")), None);
        assert_eq!(detect_mime(b"", None), None);
        assert_eq!(media_kind("audio/ogg"), Some(MediaKind::Audio));
        assert_eq!(media_kind("text/html"), None);
    }
}
