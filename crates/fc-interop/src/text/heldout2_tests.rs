//! **Second held-out** separator-detection set. Written after the detector was revised for
//! the misses of the first held-out set (`heldout_tests.rs`, which that revision therefore
//! tuned against) and measured once, before any change to fit it. Like the first set, each
//! sample states the rows a person would expect, so samples that need something no
//! separator choice gives (Anki `#` header lines, aligned columns, double tabs, unknown
//! separators) count as misses.
//!
//! Conventions: numbering stays in the first value (`1. apple`), because the importer never
//! rewrites values; leading/trailing blanks are trimmed (the parser always trims).
//!
//! **First measurement (no tuning): 63/70 (90.0%)** — below the ≥95% target; this is the
//! honest held-out number. The detector was **not** tuned against these samples afterwards.
//! One format gap they exposed was then closed as a feature (Anki's `#separator:` /
//! `#html:` header lines, tested in `tests.rs`), which makes the two Anki-header samples
//! pass: **65/70 (92.9%)**, no longer a pure held-out figure for those two. Remaining misses:
//!
//! * `kedi ~ cat` — `~` is not a candidate separator;
//! * `cat\t\tkedi` (tabs repeated for alignment) and space-aligned columns — the parser
//!   never collapses repeated separators or runs of spaces;
//! * `Evet, geliyorum.` on every line — a comma in every line reads as two columns (a
//!   sentence list and a `word, meaning` list look the same to column statistics);
//! * `well-being` / `up-to-date` list — `-` splits inconsistently but more often than not.

use super::*;

/// Correct samples now (see the module docs: 63 at the first measurement). Raising it is
/// fine; lowering it means detection got worse.
const HELD_OUT_2_FLOOR: usize = 65;

fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}

/// `(label, text, expected rows)`.
fn samples() -> Vec<(&'static str, &'static str, Vec<Vec<String>>)> {
    vec![
        // --- vocabulary lists, TR/EN
        (
            "vocab comma-space",
            "elma, apple\narmut, pear\nkiraz, cherry\nçilek, strawberry\nkarpuz, watermelon",
            r(&[
                &["elma", "apple"],
                &["armut", "pear"],
                &["kiraz", "cherry"],
                &["çilek", "strawberry"],
                &["karpuz", "watermelon"],
            ]),
        ),
        (
            "vocab colon, comma in one definition",
            "abandon: terk etmek\nabsorb: emmek\naccurate: doğru, kesin\nachieve: başarmak",
            r(&[
                &["abandon", "terk etmek"],
                &["absorb", "emmek"],
                &["accurate", "doğru, kesin"],
                &["achieve", "başarmak"],
            ]),
        ),
        (
            "vocab spaced colon",
            "apple : elma\nbook : kitap\ncar : araba",
            r(&[&["apple", "elma"], &["book", "kitap"], &["car", "araba"]]),
        ),
        (
            "vocab phrasal verbs ' - '",
            "to borrow - ödünç almak\nto lend - ödünç vermek\nto owe - borçlu olmak\nto pay back - geri ödemek",
            r(&[
                &["to borrow", "ödünç almak"],
                &["to lend", "ödünç vermek"],
                &["to owe", "borçlu olmak"],
                &["to pay back", "geri ödemek"],
            ]),
        ),
        (
            "vocab ' - ' with comma synonyms",
            "ambitious - hırslı, istekli\ncautious - dikkatli, temkinli\ncurious - meraklı",
            r(&[
                &["ambitious", "hırslı, istekli"],
                &["cautious", "dikkatli, temkinli"],
                &["curious", "meraklı"],
            ]),
        ),
        (
            "vocab tab, part of speech",
            "run (v.)\tkoşmak\nrun (n.)\tkoşu\nrunner (n.)\tkoşucu",
            r(&[
                &["run (v.)", "koşmak"],
                &["run (n.)", "koşu"],
                &["runner (n.)", "koşucu"],
            ]),
        ),
        (
            "idioms colon, commas in definitions",
            "break the ice: buzları eritmek, ilk adımı atmak\nhit the books: ders çalışmak\nunder the weather: keyifsiz, hasta",
            r(&[
                &["break the ice", "buzları eritmek, ilk adımı atmak"],
                &["hit the books", "ders çalışmak"],
                &["under the weather", "keyifsiz, hasta"],
            ]),
        ),
        (
            "proverbs ' = ' with a comma",
            "Damlaya damlaya göl olur = Many a little makes a mickle\nSakla samanı, gelir zamanı = Waste not, want not\nAz olsun, öz olsun = Quality over quantity",
            r(&[
                &["Damlaya damlaya göl olur", "Many a little makes a mickle"],
                &["Sakla samanı, gelir zamanı", "Waste not, want not"],
                &["Az olsun, öz olsun", "Quality over quantity"],
            ]),
        ),
        (
            "chemical symbols colon",
            "Fe: Demir\nCu: Bakır\nAu: Altın\nAg: Gümüş",
            r(&[
                &["Fe", "Demir"],
                &["Cu", "Bakır"],
                &["Au", "Altın"],
                &["Ag", "Gümüş"],
            ]),
        ),
        (
            "irregular verbs, 4 columns comma",
            "go,went,gone,gitmek\nsee,saw,seen,görmek\neat,ate,eaten,yemek",
            r(&[
                &["go", "went", "gone", "gitmek"],
                &["see", "saw", "seen", "görmek"],
                &["eat", "ate", "eaten", "yemek"],
            ]),
        ),
        (
            "german nouns with plural, tab",
            "der Tisch, -e\tmasa\ndie Lampe, -n\tlamba\ndas Buch, ¨-er\tkitap",
            r(&[
                &["der Tisch, -e", "masa"],
                &["die Lampe, -n", "lamba"],
                &["das Buch, ¨-er", "kitap"],
            ]),
        ),
        // --- Quizlet exports
        (
            "quizlet tab, long definitions",
            "photosynthesis\tthe process by which green plants use sunlight to make food\nchlorophyll\tgreen pigment\nstomata\ttiny openings, mostly on leaves",
            r(&[
                &[
                    "photosynthesis",
                    "the process by which green plants use sunlight to make food",
                ],
                &["chlorophyll", "green pigment"],
                &["stomata", "tiny openings, mostly on leaves"],
            ]),
        ),
        (
            "quizlet comma, german",
            "die Katze,kedi\nder Hund,köpek\ndas Haus,ev\ndie Schule,okul",
            r(&[
                &["die Katze", "kedi"],
                &["der Hund", "köpek"],
                &["das Haus", "ev"],
                &["die Schule", "okul"],
            ]),
        ),
        (
            "quizlet tab / semicolon rows",
            "ser\tolmak (kalıcı);estar\tolmak (geçici);tener\tsahip olmak;hacer\tyapmak",
            r(&[
                &["ser", "olmak (kalıcı)"],
                &["estar", "olmak (geçici)"],
                &["tener", "sahip olmak"],
                &["hacer", "yapmak"],
            ]),
        ),
        (
            "quizlet comma / semicolon rows",
            "hola,merhaba;adiós,hoşça kal;gracias,teşekkürler;por favor,lütfen",
            r(&[
                &["hola", "merhaba"],
                &["adiós", "hoşça kal"],
                &["gracias", "teşekkürler"],
                &["por favor", "lütfen"],
            ]),
        ),
        (
            "quizlet tab, semicolons in definitions",
            "ephemeral\tshort-lived; fleeting\nubiquitous\tpresent everywhere; omnipresent\nlaconic\tusing few words, concise",
            r(&[
                &["ephemeral", "short-lived; fleeting"],
                &["ubiquitous", "present everywhere; omnipresent"],
                &["laconic", "using few words, concise"],
            ]),
        ),
        (
            "quizlet tab, colons in terms",
            "Ratio 1:2\tbire iki\nRatio 3:4\tüçe dört\nRatio 2:5\tikiye beş",
            r(&[
                &["Ratio 1:2", "bire iki"],
                &["Ratio 3:4", "üçe dört"],
                &["Ratio 2:5", "ikiye beş"],
            ]),
        ),
        (
            "quizlet custom '~'",
            "kedi ~ cat\nköpek ~ dog\nfare ~ mouse",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["fare", "mouse"]]),
        ),
        (
            "quizlet custom ' - ', CRLF",
            "il gatto - kedi\r\nil cane - köpek\r\nla casa - ev\r\n",
            r(&[&["il gatto", "kedi"], &["il cane", "köpek"], &["la casa", "ev"]]),
        ),
        // --- Anki text exports
        (
            "anki export with headers and tags column",
            "#separator:tab\n#html:true\n#tags column:3\nkedi\tcat\tanimals\nköpek\tdog\tanimals\nkuş\tbird\tanimals",
            r(&[
                &["kedi", "cat", "animals"],
                &["köpek", "dog", "animals"],
                &["kuş", "bird", "animals"],
            ]),
        ),
        (
            "anki export with headers",
            "#separator:tab\n#html:false\nhello\tmerhaba\ngoodbye\tgüle güle\nthanks\tteşekkürler",
            r(&[
                &["hello", "merhaba"],
                &["goodbye", "güle güle"],
                &["thanks", "teşekkürler"],
            ]),
        ),
        (
            "anki old export, HTML",
            "<b>der Apfel</b>\telma<br>meyve\n<b>die Birne</b>\tarmut\n<b>die Traube</b>\tüzüm",
            r(&[
                &["<b>der Apfel</b>", "elma<br>meyve"],
                &["<b>die Birne</b>", "armut"],
                &["<b>die Traube</b>", "üzüm"],
            ]),
        ),
        (
            "anki export, quoted multi-line field",
            "Mitoz\t\"hücre bölünmesi\nikiye ayrılır\"\nMayoz\t\"üreme hücresi\nbölünmesi\"\nAmitoz\tdoğrudan bölünme",
            r(&[
                &["Mitoz", "hücre bölünmesi\nikiye ayrılır"],
                &["Mayoz", "üreme hücresi\nbölünmesi"],
                &["Amitoz", "doğrudan bölünme"],
            ]),
        ),
        (
            "anki cloze with tags",
            "{{c1::Ankara}} Türkiye'nin başkentidir.\tcoğrafya\n{{c1::1923}} yılında Cumhuriyet ilan edildi.\ttarih\n{{c1::Everest}} dünyanın en yüksek dağıdır.\tcoğrafya",
            r(&[
                &["{{c1::Ankara}} Türkiye'nin başkentidir.", "coğrafya"],
                &["{{c1::1923}} yılında Cumhuriyet ilan edildi.", "tarih"],
                &["{{c1::Everest}} dünyanın en yüksek dağıdır.", "coğrafya"],
            ]),
        ),
        (
            "anki semicolon export",
            "kedi;cat\nköpek;dog\nkuş;bird\nat;horse",
            r(&[
                &["kedi", "cat"],
                &["köpek", "dog"],
                &["kuş", "bird"],
                &["at", "horse"],
            ]),
        ),
        // --- Excel CSV (Turkish locale: `;` with decimal commas)
        (
            "excel ; decimal commas, 3 columns",
            "Ürün;Birim Fiyat;Adet\nKalem;7,50;10\nDefter;24,90;5\nSilgi;3,25;20",
            r(&[
                &["Ürün", "Birim Fiyat", "Adet"],
                &["Kalem", "7,50", "10"],
                &["Defter", "24,90", "5"],
                &["Silgi", "3,25", "20"],
            ]),
        ),
        (
            "excel ; decimal commas, 2 columns",
            "Ankara;5,8\nİstanbul;15,9\nİzmir;4,4\nBursa;3,2",
            r(&[
                &["Ankara", "5,8"],
                &["İstanbul", "15,9"],
                &["İzmir", "4,4"],
                &["Bursa", "3,2"],
            ]),
        ),
        (
            "excel ; header, mixed integers and decimals",
            "Ders;Ortalama\nMatematik;72,5\nFizik;68,25\nKimya;80",
            r(&[
                &["Ders", "Ortalama"],
                &["Matematik", "72,5"],
                &["Fizik", "68,25"],
                &["Kimya", "80"],
            ]),
        ),
        (
            "excel ; one decimal only",
            "Elma;12\nArmut;7,5\nKiraz;30",
            r(&[&["Elma", "12"], &["Armut", "7,5"], &["Kiraz", "30"]]),
        ),
        (
            "excel ; empty trailing cells",
            "kelime;anlam;örnek\ngitmek;to go;\ngelmek;to come;Eve geldim.\nyemek;to eat;",
            r(&[
                &["kelime", "anlam", "örnek"],
                &["gitmek", "to go", ""],
                &["gelmek", "to come", "Eve geldim."],
                &["yemek", "to eat", ""],
            ]),
        ),
        (
            "excel ; quoted cells with semicolons",
            "\"terim\";\"tanım\"\n\"ozmoz\";\"suyun yarı geçirgen zardan geçişi; pasif taşıma\"\n\"difüzyon\";\"yüksekten düşük yoğunluğa geçiş\"",
            r(&[
                &["terim", "tanım"],
                &["ozmoz", "suyun yarı geçirgen zardan geçişi; pasif taşıma"],
                &["difüzyon", "yüksekten düşük yoğunluğa geçiş"],
            ]),
        ),
        (
            "excel ; dates and times",
            "Tarih;Saat;Ders\n15.03.2024;09:30;Matematik\n16.03.2024;11:00;Fizik\n18.03.2024;14:15;Kimya",
            r(&[
                &["Tarih", "Saat", "Ders"],
                &["15.03.2024", "09:30", "Matematik"],
                &["16.03.2024", "11:00", "Fizik"],
                &["18.03.2024", "14:15", "Kimya"],
            ]),
        ),
        (
            "excel ; thousands and decimals",
            "Şehir;Nüfus;Alan\nAnkara;5.803.482;25.632,5\nİzmir;4.462.056;11.891,2",
            r(&[
                &["Şehir", "Nüfus", "Alan"],
                &["Ankara", "5.803.482", "25.632,5"],
                &["İzmir", "4.462.056", "11.891,2"],
            ]),
        ),
        (
            "excel , english locale decimals",
            "name,price\nApple,1.25\nBanana,0.5\nCherry,3.75",
            r(&[
                &["name", "price"],
                &["Apple", "1.25"],
                &["Banana", "0.5"],
                &["Cherry", "3.75"],
            ]),
        ),
        (
            "excel , quoted thousands",
            "city,population\nAnkara,\"5,803,482\"\nİzmir,\"4,462,056\"\nBursa,\"3,194,720\"",
            r(&[
                &["city", "population"],
                &["Ankara", "5,803,482"],
                &["İzmir", "4,462,056"],
                &["Bursa", "3,194,720"],
            ]),
        ),
        (
            "excel , negative numbers and ranges",
            "temp,-5\nhumidity,40-60\nwind,12",
            r(&[&["temp", "-5"], &["humidity", "40-60"], &["wind", "12"]]),
        ),
        // --- TSV
        (
            "tsv 3 columns with header and sentences",
            "English\tTurkish\tExample\nbook\tkitap\tI read a book, then slept.\npen\tkalem\tThis is my pen.",
            r(&[
                &["English", "Turkish", "Example"],
                &["book", "kitap", "I read a book, then slept."],
                &["pen", "kalem", "This is my pen."],
            ]),
        ),
        (
            "tsv decimal commas",
            "pi\t3,14159\ne\t2,71828\nkök 2\t1,41421",
            r(&[&["pi", "3,14159"], &["e", "2,71828"], &["kök 2", "1,41421"]]),
        ),
        (
            "tsv trailing empty column",
            "kedi\tcat\t\nköpek\tdog\tpet\nkuş\tbird\t",
            r(&[
                &["kedi", "cat", ""],
                &["köpek", "dog", "pet"],
                &["kuş", "bird", ""],
            ]),
        ),
        (
            "tsv CRLF",
            "uno\tbir\r\ndos\tiki\r\ntres\tüç\r\n",
            r(&[&["uno", "bir"], &["dos", "iki"], &["tres", "üç"]]),
        ),
        (
            "tsv multi-line definitions, blank-line records",
            "photosynthesis\tconverts light to energy\noccurs in chloroplasts\n\nrespiration\tbreaks down glucose\nreleases energy",
            r(&[
                &[
                    "photosynthesis",
                    "converts light to energy\noccurs in chloroplasts",
                ],
                &["respiration", "breaks down glucose\nreleases energy"],
            ]),
        ),
        // --- dashes
        (
            "' - ' with year ranges",
            "Birinci Dünya Savaşı - 1914-1918\nİkinci Dünya Savaşı - 1939-1945\nKore Savaşı - 1950-1953",
            r(&[
                &["Birinci Dünya Savaşı", "1914-1918"],
                &["İkinci Dünya Savaşı", "1939-1945"],
                &["Kore Savaşı", "1950-1953"],
            ]),
        ),
        (
            "en dash (Word autocorrect)",
            "kedi – cat\nköpek – dog\ntavşan – rabbit",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["tavşan", "rabbit"]]),
        ),
        (
            "em dash",
            "freedom — özgürlük\njustice — adalet\npeace — barış",
            r(&[
                &["freedom", "özgürlük"],
                &["justice", "adalet"],
                &["peace", "barış"],
            ]),
        ),
        (
            "dash without spaces",
            "su-water\nateş-fire\ntoprak-earth\nhava-air",
            r(&[
                &["su", "water"],
                &["ateş", "fire"],
                &["toprak", "earth"],
                &["hava", "air"],
            ]),
        ),
        (
            "' - ' with times",
            "Kahvaltı - 08:00\nÖğle yemeği - 12:30\nAkşam yemeği - 19:00",
            r(&[
                &["Kahvaltı", "08:00"],
                &["Öğle yemeği", "12:30"],
                &["Akşam yemeği", "19:00"],
            ]),
        ),
        (
            "' - ' with URLs",
            "MDN - https://developer.mozilla.org/en-US/docs\nRust Book - https://doc.rust-lang.org/book/\nTDK - https://sozluk.gov.tr",
            r(&[
                &["MDN", "https://developer.mozilla.org/en-US/docs"],
                &["Rust Book", "https://doc.rust-lang.org/book/"],
                &["TDK", "https://sozluk.gov.tr"],
            ]),
        ),
        // --- equals and arrows
        (
            "' = ' formulas",
            "H2O = su\nNaCl = tuz\nCO2 = karbondioksit\nO2 = oksijen",
            r(&[
                &["H2O", "su"],
                &["NaCl", "tuz"],
                &["CO2", "karbondioksit"],
                &["O2", "oksijen"],
            ]),
        ),
        (
            "'=' without spaces",
            "hello=merhaba\nworld=dünya\nfriend=arkadaş",
            r(&[
                &["hello", "merhaba"],
                &["world", "dünya"],
                &["friend", "arkadaş"],
            ]),
        ),
        (
            "' → '",
            "kedi → cat\nköpek → dog\nkuş → bird\nbalık → fish",
            r(&[
                &["kedi", "cat"],
                &["köpek", "dog"],
                &["kuş", "bird"],
                &["balık", "fish"],
            ]),
        ),
        (
            "'→' without spaces",
            "big→büyük\nsmall→küçük\nold→eski",
            r(&[&["big", "büyük"], &["small", "küçük"], &["old", "eski"]]),
        ),
        (
            "' → ' verb forms, 3 columns",
            "go → went → gone\nsee → saw → seen\ntake → took → taken",
            r(&[
                &["go", "went", "gone"],
                &["see", "saw", "seen"],
                &["take", "took", "taken"],
            ]),
        ),
        (
            "' -> ' phrasal verbs",
            "run -> koşmak\nrun out -> tükenmek\nrun into -> rastlamak",
            r(&[
                &["run", "koşmak"],
                &["run out", "tükenmek"],
                &["run into", "rastlamak"],
            ]),
        ),
        // --- numbered lists
        (
            "numbered ' - '",
            "1. apple - elma\n2. banana - muz\n3. orange - portakal\n4. grape - üzüm",
            r(&[
                &["1. apple", "elma"],
                &["2. banana", "muz"],
                &["3. orange", "portakal"],
                &["4. grape", "üzüm"],
            ]),
        ),
        (
            "numbered colon",
            "1) kedi: cat\n2) köpek: dog\n3) kuş: bird",
            r(&[&["1) kedi", "cat"], &["2) köpek", "dog"], &["3) kuş", "bird"]]),
        ),
        (
            "numbered '1-' with ' = '",
            "1- elma = apple\n2- armut = pear\n3- kiraz = cherry",
            r(&[
                &["1- elma", "apple"],
                &["2- armut", "pear"],
                &["3- kiraz", "cherry"],
            ]),
        ),
        (
            "numbered single column",
            "1. Ankara\n2. İstanbul\n3. İzmir\n4. Bursa",
            r(&[&["1. Ankara"], &["2. İstanbul"], &["3. İzmir"], &["4. Bursa"]]),
        ),
        (
            "number column, tab",
            "1\tkedi\tcat\n2\tköpek\tdog\n3\tkuş\tbird",
            r(&[
                &["1", "kedi", "cat"],
                &["2", "köpek", "dog"],
                &["3", "kuş", "bird"],
            ]),
        ),
        // --- mixed whitespace
        (
            "tab with spaces around",
            "kedi \t cat\nköpek\t dog\nkuş \tbird",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["kuş", "bird"]]),
        ),
        (
            "double tabs for alignment",
            "cat\t\tkedi\ndog\t\tköpek\nelephant\tfil",
            r(&[&["cat", "kedi"], &["dog", "köpek"], &["elephant", "fil"]]),
        ),
        (
            "indented colon lines",
            "   merhaba:   hello   \n  güle güle :  bye\n\tteşekkürler: thanks",
            r(&[
                &["merhaba", "hello"],
                &["güle güle", "bye"],
                &["teşekkürler", "thanks"],
            ]),
        ),
        (
            "spaced comma, blank lines",
            "water , su\n\n\nfire , ateş\n\nearth , toprak\n",
            r(&[&["water", "su"], &["fire", "ateş"], &["earth", "toprak"]]),
        ),
        (
            "space-aligned columns",
            "kedi      cat\nköpek     dog\nkuş       bird",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["kuş", "bird"]]),
        ),
        // --- colon values with protected tokens
        (
            "colon, times and a time range",
            "Başlangıç: 09:00\nBitiş: 17:30\nMola: 12:00-13:00",
            r(&[
                &["Başlangıç", "09:00"],
                &["Bitiş", "17:30"],
                &["Mola", "12:00-13:00"],
            ]),
        ),
        (
            "colon, map scales",
            "Ölçek A: 1:25000\nÖlçek B: 1:50000\nÖlçek C: 1:100000",
            r(&[
                &["Ölçek A", "1:25000"],
                &["Ölçek B", "1:50000"],
                &["Ölçek C", "1:100000"],
            ]),
        ),
        (
            "comma, URLs",
            "Wikipedia,https://www.wikipedia.org\nTDK Sözlük,https://sozluk.gov.tr\nOxford,https://www.oxfordlearnersdictionaries.com",
            r(&[
                &["Wikipedia", "https://www.wikipedia.org"],
                &["TDK Sözlük", "https://sozluk.gov.tr"],
                &["Oxford", "https://www.oxfordlearnersdictionaries.com"],
            ]),
        ),
        (
            "blank-line records ' - '",
            "kedi - cat\nevcil hayvan\n\nköpek - dog\nsadık hayvan",
            r(&[
                &["kedi", "cat\nevcil hayvan"],
                &["köpek", "dog\nsadık hayvan"],
            ]),
        ),
        // --- single-column lists
        (
            "english sentences, some commas",
            "I like tea.\nShe said, \"No.\"\nWe went home.\nWell, maybe later.",
            r(&[
                &["I like tea."],
                &["She said, \"No.\""],
                &["We went home."],
                &["Well, maybe later."],
            ]),
        ),
        (
            "turkish sentences, a comma on every line",
            "Evet, geliyorum.\nHayır, gelmiyorum.\nTamam, görüşürüz.",
            r(&[
                &["Evet, geliyorum."],
                &["Hayır, gelmiyorum."],
                &["Tamam, görüşürüz."],
            ]),
        ),
        (
            "hyphenated words",
            "well-being\nself-esteem\nup-to-date\nmother-in-law",
            r(&[
                &["well-being"],
                &["self-esteem"],
                &["up-to-date"],
                &["mother-in-law"],
            ]),
        ),
    ]
}

#[test]
fn held_out_2_detection_accuracy() {
    let samples = samples();
    let mut misses = Vec::new();
    for (label, text, expected) in &samples {
        let d = detect(text);
        let got = parse_delimited(text, &d.options).unwrap_or_default();
        if &got != expected {
            misses.push(format!(
                "{label}: got {:?}/{:?}/{:?} → {got:?}",
                d.options.field_separator, d.options.record_separator, d.options.max_fields
            ));
        }
    }
    let correct = samples.len() - misses.len();
    let report = format!(
        "held-out 2 detection: {correct}/{} correct; misses:\n{}",
        samples.len(),
        misses.join("\n")
    );
    eprintln!("{report}");
    assert!(samples.len() >= 60);
    assert!(correct >= HELD_OUT_2_FLOOR, "{report}");
}
