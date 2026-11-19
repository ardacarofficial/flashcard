use super::*;

fn rows(text: &str, f: &str, r: &str) -> Vec<Vec<String>> {
    parse_delimited(text, &TextOptions::new(f, r)).unwrap()
}

fn v(items: &[&[&str]]) -> Vec<Vec<String>> {
    items
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}

#[test]
fn acceptance_examples_parse() {
    // docs/04 v0.5: `Arda, porsuk`, `Arda: porsuk` and custom separators.
    assert_eq!(
        rows("Arda, porsuk", ",", NEWLINE),
        v(&[&["Arda", "porsuk"]])
    );
    assert_eq!(
        rows("Arda: porsuk", ":", NEWLINE),
        v(&[&["Arda", "porsuk"]])
    );
    assert_eq!(
        rows("Arda >> porsuk\nAli >> kedi", ">>", NEWLINE),
        v(&[&["Arda", "porsuk"], &["Ali", "kedi"]])
    );
    assert_eq!(
        rows("well-known - iyi bilinen", " - ", NEWLINE),
        v(&[&["well-known", "iyi bilinen"]])
    );
    // Quoted separators (docs/02 §15.1).
    assert_eq!(
        rows("\"Arda, Ali\": porsuk", ":", NEWLINE),
        v(&[&["Arda, Ali", "porsuk"]])
    );
    assert_eq!(
        rows("\"Arda, Ali\", porsuk", ",", NEWLINE),
        v(&[&["Arda, Ali", "porsuk"]])
    );
}

#[test]
fn quotes_escapes_and_multiline_values() {
    assert_eq!(
        rows("\"say \"\"hi\"\"\",selam\n", ",", NEWLINE),
        v(&[&["say \"hi\"", "selam"]])
    );
    assert_eq!(
        rows("\"line1\nline2\",x\ny,z", ",", NEWLINE),
        v(&[&["line1\nline2", "x"], &["y", "z"]])
    );
    // An unterminated quote is read literally.
    assert_eq!(rows("\"open,x", ",", NEWLINE), v(&[&["\"open", "x"]]));
    // Quoting off: quotes are data.
    let mut o = TextOptions::new(",", NEWLINE);
    o.quotes = false;
    assert_eq!(
        parse_delimited("\"a\",b", &o).unwrap(),
        v(&[&["\"a\"", "b"]])
    );
}

#[test]
fn record_separators() {
    assert_eq!(
        rows("a,b;c,d;e,f", ",", ";"),
        v(&[&["a", "b"], &["c", "d"], &["e", "f"]])
    );
    assert_eq!(
        rows("a: one\ntwo\n\n\nb: three\n  \nc: four", ":", BLANK_LINE),
        v(&[&["a", "one\ntwo"], &["b", "three"], &["c", "four"]])
    );
    assert_eq!(
        rows("\u{feff}a\tb\r\nc\td\r\n\r\n", "\t", NEWLINE),
        v(&[&["a", "b"], &["c", "d"]])
    );
}

#[test]
fn max_fields_keeps_the_rest() {
    let mut o = TextOptions::new(",", NEWLINE);
    o.max_fields = Some(2);
    assert_eq!(
        parse_delimited("dog, köpek, it", &o).unwrap(),
        v(&[&["dog", "köpek, it"]])
    );
    let q = TextOptions::quizlet();
    assert_eq!(
        parse_delimited("cat\tkedi\tpisi\n", &q).unwrap(),
        v(&[&["cat", "kedi\tpisi"]])
    );
}

#[test]
fn rejects_bad_options() {
    assert!(TextOptions::new("", NEWLINE).validate().is_err());
    assert!(TextOptions::new(",", ",").validate().is_err());
    assert!(TextOptions::new("a\nb", NEWLINE).validate().is_err());
    assert!(TextOptions::new("\"", NEWLINE).validate().is_err());
    assert!(TextOptions::new(";", ";;").validate().is_err());
}

#[test]
fn write_then_parse_roundtrips() {
    let data = v(&[
        &["plain", "with, comma"],
        &["\"quoted\"", "multi\nline"],
        &[" padded ", "x;y"],
        &["ünicode İı", ""],
    ]);
    for (f, r) in [
        (",", NEWLINE),
        ("\t", NEWLINE),
        (";", NEWLINE),
        (":", BLANK_LINE),
    ] {
        let o = TextOptions::new(f, r);
        let text = write_delimited(&data, &o);
        assert_eq!(
            parse_delimited(&text, &o).unwrap(),
            data,
            "{f:?} {r:?}\n{text}"
        );
    }
}

/// Labelled detection set: (text, field separator, record separator). The detection is right
/// when its suggestion parses the text into the same records as the label.
fn corpus() -> Vec<(String, &'static str, &'static str)> {
    let mut c: Vec<(String, &str, &str)> = vec![
        ("Arda, porsuk".into(), ",", NEWLINE),
        ("Arda: porsuk".into(), ":", NEWLINE),
        ("Arda, porsuk\nAli, kedi\nVeli, köpek".into(), ",", NEWLINE),
        ("Arda: porsuk\nAli: kedi\nVeli: köpek".into(), ":", NEWLINE),
        (
            "Arda: porsuk, hayvan\nAli: kedi, evcil\nVeli: köpek".into(),
            ":",
            NEWLINE,
        ),
        ("dog\tköpek\ncat\tkedi\nbird\tkuş\n".into(), "\t", NEWLINE),
        ("dog\tköpek, it\ncat\tkedi, pisi\n".into(), "\t", NEWLINE),
        ("dog;köpek\ncat;kedi\nbird;kuş".into(), ";", NEWLINE),
        ("dog = köpek\ncat = kedi".into(), " = ", NEWLINE),
        ("dog=köpek\ncat=kedi\nfish=balık".into(), "=", NEWLINE),
        ("dog - köpek\ncat - kedi".into(), " - ", NEWLINE),
        (
            "well-known - iyi bilinen\nself-made - kendi yapımı\nup-to-date - güncel".into(),
            " - ",
            NEWLINE,
        ),
        ("dog-köpek\ncat-kedi\nbird-kuş".into(), "-", NEWLINE),
        ("dog >> köpek\ncat >> kedi".into(), ">>", NEWLINE),
        ("dog=>köpek\ncat=>kedi\nhorse=>at".into(), "=>", NEWLINE),
        ("dog -> köpek\ncat -> kedi".into(), "->", NEWLINE),
        ("dog | köpek\ncat | kedi\ncow | inek".into(), "|", NEWLINE),
        ("\"Arda, Ali\": porsuk\nVeli: kedi".into(), ":", NEWLINE),
        ("\"Arda, Ali\", porsuk\n\"Veli\", kedi".into(), ",", NEWLINE),
        (
            "front,back,level\nkedi,cat,A1\nköpek,dog,A1\nhayal,dream,B1".into(),
            ",",
            NEWLINE,
        ),
        (
            "front;back;level\nkedi;cat;A1\nköpek;dog;A1".into(),
            ";",
            NEWLINE,
        ),
        (
            "front\tback\tlevel\tnotes\nkedi\tcat\tA1\tpet\nköpek\tdog\tA1\t".into(),
            "\t",
            NEWLINE,
        ),
        ("a,b;c,d;e,f".into(), ",", ";"),
        ("dog, köpek; cat, kedi; bird, kuş".into(), ",", ";"),
        ("dog\tköpek;cat\tkedi;bird\tkuş".into(), "\t", ";"),
        (
            "dog: a loyal animal\nthat barks\n\ncat: an animal\nthat meows\n\nbird: it flies"
                .into(),
            ":",
            BLANK_LINE,
        ),
        (
            "dog\ta loyal animal\nthat barks\n\ncat\tan animal\nthat meows".into(),
            "\t",
            BLANK_LINE,
        ),
        (
            "10:30,meeting\n11:00,lunch\n14:15,call".into(),
            ",",
            NEWLINE,
        ),
        (
            "http://a.example,site a\nhttp://b.example,site b".into(),
            ",",
            NEWLINE,
        ),
        ("dog, köpek\r\ncat, kedi\r\n".into(), ",", NEWLINE),
        ("\u{feff}dog;köpek\ncat;kedi".into(), ";", NEWLINE),
        (
            "İstanbul: Türkiye'nin en kalabalık şehri\nAnkara: başkent\nİzmir: Ege".into(),
            ":",
            NEWLINE,
        ),
        ("x^2: kare\nsqrt(x): karekök\nx^3: küp".into(), ":", NEWLINE),
        (
            "the cat sat - kedi oturdu\nthe dog ran - köpek koştu".into(),
            " - ",
            NEWLINE,
        ),
        (
            "hello,merhaba\n\ngoodbye,hoşçakal\n\nthanks,teşekkürler".into(),
            ",",
            NEWLINE,
        ),
        (
            "one\tbir\ntwo\tiki\nthree\tüç\nfour\tdört\nfive\tbeş".into(),
            "\t",
            NEWLINE,
        ),
        ("\"a; b\";c\n\"d; e\";f".into(), ";", NEWLINE),
        (
            "term|definition|example\nrun|koşmak|I run\nwalk|yürümek|I walk".into(),
            "|",
            NEWLINE,
        ),
        (
            "apple = elma\npear = armut\nplum = erik\ngrape = üzüm".into(),
            " = ",
            NEWLINE,
        ),
        (
            "kitap >> book\nkalem >> pen\ndefter >> notebook".into(),
            ">>",
            NEWLINE,
        ),
    ];
    // Generated variants: a vocabulary list with each separator and a few lengths.
    let words = [
        ("ev", "house"),
        ("su", "water"),
        ("gün", "day"),
        ("yol", "road"),
        ("göz", "eye"),
        ("el", "hand"),
    ];
    for (sep, rec) in [
        (",", NEWLINE),
        (":", NEWLINE),
        ("\t", NEWLINE),
        (";", NEWLINE),
        (" - ", NEWLINE),
        (">>", NEWLINE),
        ("=", NEWLINE),
        (",", ";"),
        ("\t", ";"),
        (":", BLANK_LINE),
    ] {
        for n in [2usize, 4, 6] {
            let o = TextOptions::new(sep, rec);
            let data: Vec<Vec<String>> = words[..n]
                .iter()
                .map(|(a, b)| vec![a.to_string(), b.to_string()])
                .collect();
            let mut text = write_delimited(&data, &o);
            if rec == BLANK_LINE {
                text = text.replace("\n\n", "\n\n\n");
            }
            c.push((text, sep, rec));
        }
    }
    c
}

#[test]
fn detection_is_at_least_95_percent_accurate() {
    let corpus = corpus();
    let mut wrong = Vec::new();
    for (text, f, r) in &corpus {
        let expected = rows(text, f, r);
        let d = detect(text);
        let got = parse_delimited(text, &d.options).unwrap();
        if got != expected {
            wrong.push(format!(
                "{text:?}: expected {f:?}/{r:?}, got {:?}/{:?}",
                d.options.field_separator, d.options.record_separator
            ));
        }
    }
    let accuracy = 1.0 - wrong.len() as f64 / corpus.len() as f64;
    assert!(
        accuracy >= 0.95,
        "accuracy {accuracy:.3} on {} samples; misses:\n{}",
        corpus.len(),
        wrong.join("\n")
    );
    assert!(corpus.len() >= 60);
}

#[test]
fn detection_reports_columns_and_confidence() {
    let d = detect("front,back,level\nkedi,cat,A1\nköpek,dog,A1");
    assert_eq!(d.options.field_separator, ",");
    assert_eq!(d.columns, 3);
    assert_eq!(d.records, 3);
    assert!(d.confidence > 0.5);
    let none = detect("just one line of text");
    assert_eq!(none.confidence, 0.0);
    assert_eq!(none.columns, 1);
}
