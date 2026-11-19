//! [`decode`]: what Turkish Windows/Excel writes (Windows-1254, UTF-16 with and without BOM,
//! UTF-8 with BOM) decodes to the same text, which then parses like UTF-8 input.

use super::*;

const TR: &str = "ğüşıöçĞÜŞİÖÇ";
/// A CSV with every Turkish-specific letter.
const CSV: &str = "ön;arka\nğüşıöç;ĞÜŞİÖÇ\nIşık;İğne\r\nçiçek;şeker\r\n";

/// Windows-1254 bytes of `s` (hand-mapped, independent of `encoding_rs`).
fn cp1254(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            'ğ' => 0xF0,
            'Ğ' => 0xD0,
            'ü' => 0xFC,
            'Ü' => 0xDC,
            'ş' => 0xFE,
            'Ş' => 0xDE,
            'ı' => 0xFD,
            'İ' => 0xDD,
            'ö' => 0xF6,
            'Ö' => 0xD6,
            'ç' => 0xE7,
            'Ç' => 0xC7,
            c if c.is_ascii() => c as u8,
            c => panic!("no cp1254 mapping for {c}"),
        })
        .collect()
}

fn utf16(s: &str, little: bool, bom: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let units = bom
        .then_some(0xFEFF_u16)
        .into_iter()
        .chain(s.encode_utf16());
    for u in units {
        out.extend_from_slice(&if little {
            u.to_le_bytes()
        } else {
            u.to_be_bytes()
        });
    }
    out
}

fn assert_parses(text: &str) {
    let d = detect(text);
    assert_eq!(d.options.field_separator, ";");
    let rows = parse_delimited(text, &d.options).unwrap();
    assert_eq!(
        rows,
        vec![
            vec!["ön".to_string(), "arka".into()],
            vec!["ğüşıöç".into(), "ĞÜŞİÖÇ".into()],
            vec!["Işık".into(), "İğne".into()],
            vec!["çiçek".into(), "şeker".into()],
        ]
    );
}

#[test]
fn windows_1254_decodes_turkish_letters() {
    let bytes = cp1254(CSV);
    assert!(std::str::from_utf8(&bytes).is_err());
    let d = decode(&bytes);
    assert_eq!(d.encoding, "windows-1254");
    assert_eq!(d.text, CSV);
    assert_eq!(decode(&cp1254(TR)).text, TR);
    assert_parses(&d.text);
}

#[test]
fn utf16_with_bom_both_byte_orders() {
    for (little, name) in [(true, "UTF-16LE"), (false, "UTF-16BE")] {
        let d = decode(&utf16(CSV, little, true));
        assert_eq!(d.encoding, name);
        assert_eq!(d.text, CSV, "{name}");
        assert_parses(&d.text);
    }
}

#[test]
fn utf16_without_bom_is_recognised() {
    for (little, name) in [(true, "UTF-16LE"), (false, "UTF-16BE")] {
        let d = decode(&utf16(CSV, little, false));
        assert_eq!(d.encoding, name);
        assert_eq!(d.text, CSV, "{name}");
    }
    // ASCII-only UTF-16 is valid UTF-8 too (full of NULs): still read as UTF-16.
    let d = decode(&utf16("a;b\nc;d", true, false));
    assert_eq!((d.encoding, d.text.as_str()), ("UTF-16LE", "a;b\nc;d"));
}

#[test]
fn utf8_with_and_without_bom() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(CSV.as_bytes());
    let d = decode(&bytes);
    assert_eq!((d.encoding, d.text.as_str()), ("UTF-8", CSV));
    assert_parses(&d.text);
    let d = decode(CSV.as_bytes());
    assert_eq!((d.encoding, d.text.as_str()), ("UTF-8", CSV));
    assert_eq!(decode(b"").text, "");
}
