//! **Held-out** separator-detection set (review item #5). These samples were written after
//! the detector was finished and the detector was **not** changed to fit them; they measure
//! it, they do not tune it. Each sample states the rows a person would expect (not "what our
//! parser gives with the right separator"), so samples that need options the detector never
//! suggests (`max_fields`, quoting off, unknown separators) count as misses.
//!
//! Result when added: **40/46 (87%)**, versus ≥95% on the tuning corpus in `tests.rs` — the
//! honest number. Misses at that time:
//!
//! * `pi;3,14`, `sıcaklık;36,6 °C` — decimal commas: each line has exactly one `,` and one
//!   `;`, so both split into two columns and `,` wins on prior weight. (`Ürün;Fiyat;KDV` with
//!   three columns and the `x;y` numeric table are detected.)
//! * `kedi::cat` — `::` is not a candidate; `:` splits into three fields.
//! * `Toplantı: 10:30`, `Google: https://…` — need `max_fields: 2`; detection never limits
//!   fields.
//! * `Merhaba, nasılsın?` — a one-column list of sentences with commas reads as two columns.
//!
//! **Afterwards the detector was revised for exactly these failure classes** (protected-token
//! masking for numbers/times/URLs with a suggested `max_fields`, `::` and dash/arrow
//! candidates, "a separator missing from > 25% of the records is not one", record separators
//! must separate something). It now gets **46/46**, but this set has become a tuning set: the
//! untouched measurement is the second held-out set (`heldout2_tests.rs`).

use super::*;

/// Correct samples now (40 when the set was added). Raising it is fine; lowering it means
/// detection got worse.
const HELD_OUT_FLOOR: usize = 46;

fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}

/// `(label, text, expected rows)`.
fn samples() -> Vec<(&'static str, &'static str, Vec<Vec<String>>)> {
    vec![
        // --- Quizlet exports (term/definition separator, row separator)
        (
            "quizlet tab",
            "kedi\tcat\nköpek\tdog\nkuş\tbird",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["kuş", "bird"]]),
        ),
        (
            "quizlet tab, commas in definitions",
            "apple\telma, bir meyve\npear\tarmut, sarı meyve\nplum\terik",
            r(&[
                &["apple", "elma, bir meyve"],
                &["pear", "armut, sarı meyve"],
                &["plum", "erik"],
            ]),
        ),
        (
            "quizlet comma/newline",
            "house,ev\nwater,su\nday,gün",
            r(&[&["house", "ev"], &["water", "su"], &["day", "gün"]]),
        ),
        (
            "quizlet tab/semicolon",
            "house\tev;water\tsu;day\tgün",
            r(&[&["house", "ev"], &["water", "su"], &["day", "gün"]]),
        ),
        (
            "quizlet comma/semicolon",
            "house,ev;water,su;day,gün",
            r(&[&["house", "ev"], &["water", "su"], &["day", "gün"]]),
        ),
        (
            "quizlet custom ' - '",
            "to run - koşmak\nto walk - yürümek\nto swim - yüzmek",
            r(&[
                &["to run", "koşmak"],
                &["to walk", "yürümek"],
                &["to swim", "yüzmek"],
            ]),
        ),
        (
            "quizlet custom '::'",
            "kedi::cat\nköpek::dog\nkuş::bird",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["kuş", "bird"]]),
        ),
        (
            "quizlet tab, CRLF, trailing newline",
            "bonjour\tmerhaba\r\nmerci\tteşekkürler\r\nau revoir\thoşça kal\r\n",
            r(&[
                &["bonjour", "merhaba"],
                &["merci", "teşekkürler"],
                &["au revoir", "hoşça kal"],
            ]),
        ),
        (
            "quizlet tab, ' - ' inside definitions",
            "well-known\tiyi bilinen - meşhur\nup-to-date\tgüncel\nlong-term\tuzun vadeli - kalıcı",
            r(&[
                &["well-known", "iyi bilinen - meşhur"],
                &["up-to-date", "güncel"],
                &["long-term", "uzun vadeli - kalıcı"],
            ]),
        ),
        (
            "quizlet tab, colons inside definitions",
            "Osmanlı\tKuruluş: 1299\nSelçuklu\tKuruluş: 1037\nKarahanlı\tKuruluş: 840",
            r(&[
                &["Osmanlı", "Kuruluş: 1299"],
                &["Selçuklu", "Kuruluş: 1037"],
                &["Karahanlı", "Kuruluş: 840"],
            ]),
        ),
        // --- decimal commas (Turkish/European CSV uses `;`)
        (
            "decimal commas, semicolon",
            "pi;3,14\ne;2,72\nphi;1,62",
            r(&[&["pi", "3,14"], &["e", "2,72"], &["phi", "1,62"]]),
        ),
        (
            "decimal commas, 3 columns with header",
            "Ürün;Fiyat;KDV\nElma;12,50;%1\nArmut;15,75;%1",
            r(&[
                &["Ürün", "Fiyat", "KDV"],
                &["Elma", "12,50", "%1"],
                &["Armut", "15,75", "%1"],
            ]),
        ),
        (
            "decimal commas with units",
            "sıcaklık;36,6 °C\nnem;%45,5\nbasınç;1013,25 hPa",
            r(&[
                &["sıcaklık", "36,6 °C"],
                &["nem", "%45,5"],
                &["basınç", "1013,25 hPa"],
            ]),
        ),
        (
            "decimal commas, tab",
            "1,5\tbir buçuk\n2,25\tiki çeyrek\n0,5\tyarım",
            r(&[&["1,5", "bir buçuk"], &["2,25", "iki çeyrek"], &["0,5", "yarım"]]),
        ),
        (
            "decimal commas, colon",
            "Avogadro sayısı: 6,022 × 10^23\nPlanck sabiti: 6,626 × 10^-34\nIşık hızı: 299.792 km/s",
            r(&[
                &["Avogadro sayısı", "6,022 × 10^23"],
                &["Planck sabiti", "6,626 × 10^-34"],
                &["Işık hızı", "299.792 km/s"],
            ]),
        ),
        (
            "decimal commas, numeric pairs",
            "x;y\n1,0;2,5\n3,5;4,0\n5,5;6,25",
            r(&[&["x", "y"], &["1,0", "2,5"], &["3,5", "4,0"], &["5,5", "6,25"]]),
        ),
        // --- tabs with commas inside values
        (
            "tab, sentences with commas",
            "Paris\tFransa'nın başkenti, en büyük şehri\nBerlin\tAlmanya'nın başkenti, Spree kıyısında\nRoma\tİtalya'nın başkenti, antik şehir",
            r(&[
                &["Paris", "Fransa'nın başkenti, en büyük şehri"],
                &["Berlin", "Almanya'nın başkenti, Spree kıyısında"],
                &["Roma", "İtalya'nın başkenti, antik şehir"],
            ]),
        ),
        (
            "tab, 3 columns, commas",
            "front\tback\ttags\ncat\tkedi, pisi\tanimals\ndog\tköpek, it\tanimals",
            r(&[
                &["front", "back", "tags"],
                &["cat", "kedi, pisi", "animals"],
                &["dog", "köpek, it", "animals"],
            ]),
        ),
        (
            "tab, synonyms",
            "abandon\tterk etmek, bırakmak\nability\tyetenek, beceri\nable\tyapabilen, muktedir",
            r(&[
                &["abandon", "terk etmek, bırakmak"],
                &["ability", "yetenek, beceri"],
                &["able", "yapabilen, muktedir"],
            ]),
        ),
        (
            "tab, commas on both sides",
            "red, green\tkırmızı, yeşil\nblue, black\tmavi, siyah\nwhite, grey\tbeyaz, gri",
            r(&[
                &["red, green", "kırmızı, yeşil"],
                &["blue, black", "mavi, siyah"],
                &["white, grey", "beyaz, gri"],
            ]),
        ),
        // --- quoted CSV
        (
            "quoted names",
            "\"Smith, John\",yazar\n\"Doe, Jane\",editör\n\"Kaya, Ayşe\",çevirmen",
            r(&[
                &["Smith, John", "yazar"],
                &["Doe, Jane", "editör"],
                &["Kaya, Ayşe", "çevirmen"],
            ]),
        ),
        (
            "fully quoted with header",
            "term,definition\n\"mitochondria\",\"powerhouse of the cell, makes ATP\"\n\"ribosome\",\"makes proteins\"",
            r(&[
                &["term", "definition"],
                &["mitochondria", "powerhouse of the cell, makes ATP"],
                &["ribosome", "makes proteins"],
            ]),
        ),
        (
            "quoted newline",
            "\"line one\nline two\",tanım\nsecond,ikinci\nthird,üçüncü",
            r(&[
                &["line one\nline two", "tanım"],
                &["second", "ikinci"],
                &["third", "üçüncü"],
            ]),
        ),
        (
            "escaped quotes",
            "\"He said \"\"hi\"\"\",dedi ki merhaba\n\"ok\",tamam\n\"no\",hayır",
            r(&[
                &["He said \"hi\"", "dedi ki merhaba"],
                &["ok", "tamam"],
                &["no", "hayır"],
            ]),
        ),
        (
            "semicolon CSV, quoted semicolons",
            "\"Ankara; başkent\";Türkiye\n\"Paris; başkent\";Fransa\n\"Roma\";İtalya",
            r(&[
                &["Ankara; başkent", "Türkiye"],
                &["Paris; başkent", "Fransa"],
                &["Roma", "İtalya"],
            ]),
        ),
        // --- Turkish text, various pair separators
        (
            "turkish comma-space",
            "ağaç, tree\nçiçek, flower\nşehir, city\nırmak, river",
            r(&[
                &["ağaç", "tree"],
                &["çiçek", "flower"],
                &["şehir", "city"],
                &["ırmak", "river"],
            ]),
        ),
        (
            "turkish colon",
            "İyi günler: Good day\nGünaydın: Good morning\nİyi akşamlar: Good evening",
            r(&[
                &["İyi günler", "Good day"],
                &["Günaydın", "Good morning"],
                &["İyi akşamlar", "Good evening"],
            ]),
        ),
        (
            "turkish ' = '",
            "öğretmen = teacher\nöğrenci = student\nokul = school",
            r(&[
                &["öğretmen", "teacher"],
                &["öğrenci", "student"],
                &["okul", "school"],
            ]),
        ),
        (
            "arrow with dashes in values",
            "Çanakkale Savaşı -> 1915\nKurtuluş Savaşı -> 1919-1922\nCumhuriyet -> 1923",
            r(&[
                &["Çanakkale Savaşı", "1915"],
                &["Kurtuluş Savaşı", "1919-1922"],
                &["Cumhuriyet", "1923"],
            ]),
        ),
        (
            "pipe",
            "fiil|verb\nisim|noun\nsıfat|adjective",
            r(&[&["fiil", "verb"], &["isim", "noun"], &["sıfat", "adjective"]]),
        ),
        (
            "fat arrow",
            "big => büyük\nsmall => küçük\nfast => hızlı",
            r(&[&["big", "büyük"], &["small", "küçük"], &["fast", "hızlı"]]),
        ),
        (
            "double angle",
            "kar >> snow\nyağmur >> rain\ndolu >> hail",
            r(&[&["kar", "snow"], &["yağmur", "rain"], &["dolu", "hail"]]),
        ),
        (
            "spaced dash, hyphens in words",
            "e-mail - e-posta\nT-shirt - tişört\nself-service - self servis",
            r(&[
                &["e-mail", "e-posta"],
                &["T-shirt", "tişört"],
                &["self-service", "self servis"],
            ]),
        ),
        // --- single-column lists
        (
            "word list",
            "elma\narmut\nkiraz\nçilek",
            r(&[&["elma"], &["armut"], &["kiraz"], &["çilek"]]),
        ),
        (
            "sentence list",
            "Bugün hava çok güzel.\nYarın yağmur yağacak.\nDün kar yağdı.",
            r(&[
                &["Bugün hava çok güzel."],
                &["Yarın yağmur yağacak."],
                &["Dün kar yağdı."],
            ]),
        ),
        (
            "sentence list with commas",
            "Merhaba, nasılsın?\nİyiyim, teşekkürler.\nSen nasılsın?",
            r(&[
                &["Merhaba, nasılsın?"],
                &["İyiyim, teşekkürler."],
                &["Sen nasılsın?"],
            ]),
        ),
        // --- colon `term: definition`
        (
            "colon definitions",
            "Photosynthesis: the process by which plants make food\nRespiration: breaking down glucose for energy",
            r(&[
                &["Photosynthesis", "the process by which plants make food"],
                &["Respiration", "breaking down glucose for energy"],
            ]),
        ),
        (
            "colon acronyms",
            "HTTP: Hypertext Transfer Protocol\nURL: Uniform Resource Locator\nCPU: Central Processing Unit",
            r(&[
                &["HTTP", "Hypertext Transfer Protocol"],
                &["URL", "Uniform Resource Locator"],
                &["CPU", "Central Processing Unit"],
            ]),
        ),
        (
            "colon, times in values",
            "Toplantı: 10:30\nÖğle arası: 12:00\nÇıkış: 17:45",
            r(&[
                &["Toplantı", "10:30"],
                &["Öğle arası", "12:00"],
                &["Çıkış", "17:45"],
            ]),
        ),
        (
            "colon, URLs in values",
            "Google: https://google.com\nWiki: https://wikipedia.org\nHarita: https://openstreetmap.org",
            r(&[
                &["Google", "https://google.com"],
                &["Wiki", "https://wikipedia.org"],
                &["Harita", "https://openstreetmap.org"],
            ]),
        ),
        (
            "colon, blank-line records with multi-line definitions",
            "Mitoz: hücre bölünmesi\nikiye ayrılır\n\nMayoz: üreme hücresi\nbölünmesi",
            r(&[
                &["Mitoz", "hücre bölünmesi\nikiye ayrılır"],
                &["Mayoz", "üreme hücresi\nbölünmesi"],
            ]),
        ),
        // --- mixed
        (
            "comma, header, sentences",
            "word,meaning,example\nrun,koşmak,I run every day\neat,yemek,We eat at noon",
            r(&[
                &["word", "meaning", "example"],
                &["run", "koşmak", "I run every day"],
                &["eat", "yemek", "We eat at noon"],
            ]),
        ),
        (
            "excel semicolon, empty cells",
            "kelime;anlam;not\nkedi;cat;\nköpek;dog;evcil\nkuş;bird;",
            r(&[
                &["kelime", "anlam", "not"],
                &["kedi", "cat", ""],
                &["köpek", "dog", "evcil"],
                &["kuş", "bird", ""],
            ]),
        ),
        (
            "tab, empty definition",
            "uno\tbir\ndos\t\ntres\tüç\ncuatro\tdört",
            r(&[&["uno", "bir"], &["dos", ""], &["tres", "üç"], &["cuatro", "dört"]]),
        ),
        (
            "padding and blank lines",
            "  sun ,  güneş \n\n moon , ay\n\nstar , yıldız  \n",
            r(&[&["sun", "güneş"], &["moon", "ay"], &["star", "yıldız"]]),
        ),
        (
            "colon fields, semicolon records on one line",
            "kedi: cat; köpek: dog; kuş: bird",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["kuş", "bird"]]),
        ),
    ]
}

#[test]
fn held_out_detection_accuracy() {
    let samples = samples();
    let mut misses = Vec::new();
    for (label, text, expected) in &samples {
        let d = detect(text);
        let got = parse_delimited(text, &d.options).unwrap_or_default();
        if &got != expected {
            misses.push(format!(
                "{label}: got {:?}/{:?} → {got:?}",
                d.options.field_separator, d.options.record_separator
            ));
        }
    }
    let correct = samples.len() - misses.len();
    let report = format!(
        "held-out detection: {correct}/{} correct; misses:\n{}",
        samples.len(),
        misses.join("\n")
    );
    eprintln!("{report}");
    assert!(samples.len() >= 40);
    assert!(correct >= HELD_OUT_FLOOR, "{report}");
}
