//! Built-in starter decks (docs/02 §32): small, written for this app (no third-party lists),
//! in Turkish and English, each with the classification template it uses and a value per
//! note. Installing one makes ordinary decks and notes the user can change or delete.

use crate::i18n::{Locale, LocalizedText};

/// Which onboarding goal suggests a starter deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarterGoal {
    /// Language learning.
    Language,
    /// Exam preparation.
    Exam,
    /// General knowledge.
    General,
    /// How to use the app (offered to everyone).
    App,
}

/// One note of a starter deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterNote {
    /// Field key → text.
    pub fields: &'static [(&'static str, LocalizedText)],
    /// Index of the classification option (in the template's option order), if any.
    pub option: Option<usize>,
}

impl StarterNote {
    /// Field values in `locale`, skipping empty ones.
    pub fn fields_in(&self, locale: Locale) -> Vec<(&'static str, &'static str)> {
        self.fields
            .iter()
            .map(|(k, v)| (*k, v.get(locale)))
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }
}

/// A starter deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterDeck {
    /// Stable id (`english_basics`, …).
    pub id: &'static str,
    /// Deck name.
    pub name: LocalizedText,
    /// One-line description.
    pub description: LocalizedText,
    /// Goal that suggests it.
    pub goal: StarterGoal,
    /// Built-in note type marker (`fc_core::content::builtin`).
    pub notetype: &'static str,
    /// Classification template defined on the deck (`fc_core::attribute::templates`).
    pub template: Option<&'static str>,
    /// Notes.
    pub notes: &'static [StarterNote],
}

/// Every starter deck, in display order.
pub fn all() -> &'static [StarterDeck] {
    DECKS
}

/// A starter deck by id.
pub fn find(id: &str) -> Option<&'static StarterDeck> {
    DECKS.iter().find(|d| d.id == id)
}

const fn t(en: &'static str, tr: &'static str) -> LocalizedText {
    LocalizedText { en, tr }
}

macro_rules! notes {
    ($( [$($key:literal: ($en:literal, $tr:literal)),* $(,)?] $(@ $opt:literal)? ;)*) => {
        &[$(StarterNote {
            fields: &[$(($key, t($en, $tr))),*],
            option: notes!(@opt $($opt)?),
        }),*]
    };
    (@opt) => { None };
    (@opt $opt:literal) => { Some($opt) };
}

// CEFR option indexes: A1 = 0, A2 = 1. Meanings: English learner's definition / Turkish.
const ENGLISH: &[StarterNote] = notes![
    ["word": ("hello", "hello"), "meaning": ("a word you say when you meet someone", "merhaba"), "example": ("Hello, how are you today?", "Hello, how are you today?")] @0;
    ["word": ("water", "water"), "meaning": ("the clear liquid that you drink", "su"), "example": ("Can I have a glass of water, please?", "Can I have a glass of water, please?")] @0;
    ["word": ("book", "book"), "meaning": ("pages with writing on them, joined together", "kitap"), "example": ("This book is about a small village.", "This book is about a small village.")] @0;
    ["word": ("house", "house"), "meaning": ("a building where people live", "ev"), "example": ("Their house has a blue door.", "Their house has a blue door.")] @0;
    ["word": ("friend", "friend"), "meaning": ("a person you like and know well", "arkadaş"), "example": ("My friend lives near the park.", "My friend lives near the park.")] @0;
    ["word": ("eat", "eat"), "meaning": ("to put food in your mouth and swallow it", "yemek, yemek yemek"), "example": ("We eat dinner at seven.", "We eat dinner at seven.")] @0;
    ["word": ("drink", "drink"), "meaning": ("to take a liquid into your body through your mouth", "içmek"), "example": ("She drinks tea every morning.", "She drinks tea every morning.")] @0;
    ["word": ("big", "big"), "meaning": ("large in size", "büyük"), "example": ("That is a big dog.", "That is a big dog.")] @0;
    ["word": ("small", "small"), "meaning": ("little in size", "küçük"), "example": ("I have a small bag.", "I have a small bag.")] @0;
    ["word": ("happy", "happy"), "meaning": ("feeling good and pleased", "mutlu"), "example": ("The children are happy today.", "The children are happy today.")] @0;
    ["word": ("go", "go"), "meaning": ("to move to another place", "gitmek"), "example": ("I go to school by bus.", "I go to school by bus.")] @0;
    ["word": ("come", "come"), "meaning": ("to move towards the speaker", "gelmek"), "example": ("Come here, please.", "Come here, please.")] @0;
    ["word": ("day", "day"), "meaning": ("a period of 24 hours", "gün"), "example": ("Have a nice day!", "Have a nice day!")] @0;
    ["word": ("night", "night"), "meaning": ("the time when it is dark outside", "gece"), "example": ("You can see the stars at night.", "You can see the stars at night.")] @0;
    ["word": ("family", "family"), "meaning": ("parents, children and other relatives", "aile"), "example": ("My family is very big.", "My family is very big.")] @0;
    ["word": ("work", "work"), "meaning": ("to do a job for money", "çalışmak"), "example": ("My father works in a hospital.", "My father works in a hospital.")] @0;
    ["word": ("read", "read"), "meaning": ("to look at words and understand them", "okumak"), "example": ("I read a story every night.", "I read a story every night.")] @0;
    ["word": ("write", "write"), "meaning": ("to put letters or words on paper or a screen", "yazmak"), "example": ("Please write your name here.", "Please write your name here.")] @0;
    ["word": ("borrow", "borrow"), "meaning": ("to take something and give it back later", "ödünç almak"), "example": ("Can I borrow your pen?", "Can I borrow your pen?")] @1;
    ["word": ("journey", "journey"), "meaning": ("travelling from one place to another", "yolculuk"), "example": ("The journey took three hours.", "The journey took three hours.")] @1;
    ["word": ("decide", "decide"), "meaning": ("to choose something after thinking", "karar vermek"), "example": ("We decided to stay at home.", "We decided to stay at home.")] @1;
    ["word": ("healthy", "healthy"), "meaning": ("good for your body; not ill", "sağlıklı"), "example": ("Fruit is healthy food.", "Fruit is healthy food.")] @1;
    ["word": ("explain", "explain"), "meaning": ("to make something clear and easy to understand", "açıklamak"), "example": ("Can you explain this word?", "Can you explain this word?")] @1;
    ["word": ("neighbour", "neighbour"), "meaning": ("a person who lives near you", "komşu"), "example": ("Our neighbour has two cats.", "Our neighbour has two cats.")] @1;
    ["word": ("invite", "invite"), "meaning": ("to ask someone to come to an event", "davet etmek"), "example": ("They invited us to the wedding.", "They invited us to the wedding.")] @1;
    ["word": ("quiet", "quiet"), "meaning": ("making very little noise", "sessiz"), "example": ("The library is very quiet.", "The library is very quiet.")] @1;
    ["word": ("remember", "remember"), "meaning": ("to keep something in your mind", "hatırlamak"), "example": ("I always remember your birthday.", "I always remember your birthday.")] @1;
    ["word": ("weather", "weather"), "meaning": ("rain, sun, wind and temperature outside", "hava durumu"), "example": ("The weather is cold today.", "The weather is cold today.")] @1;
    ["word": ("arrive", "arrive"), "meaning": ("to reach a place", "varmak, ulaşmak"), "example": ("The train arrives at noon.", "The train arrives at noon.")] @1;
    ["word": ("cheap", "cheap"), "meaning": ("costing little money", "ucuz"), "example": ("This shirt was very cheap.", "This shirt was very cheap.")] @1;
];

// YKS option indexes: TYT = 0.
const YKS_MATH: &[StarterNote] = notes![
    ["front": ("Sum of the interior angles of a triangle", "Üçgenin iç açıları toplamı"), "back": ("180°", "180°")] @0;
    ["front": ("Sum of the interior angles of a quadrilateral", "Dörtgenin iç açıları toplamı"), "back": ("360°", "360°")] @0;
    ["front": ("Sum of the interior angles of an n-sided polygon", "n kenarlı çokgenin iç açıları toplamı"), "back": ("(n − 2) · 180°", "(n − 2) · 180°")] @0;
    ["front": ("What is a prime number?", "Asal sayı nedir?"), "back": ("A natural number greater than 1 whose only divisors are 1 and itself. The smallest is 2.", "1'den büyük, 1 ve kendisinden başka böleni olmayan doğal sayı. En küçüğü 2'dir.")] @0;
    ["front": ("a² − b² = ?", "a² − b² = ?"), "back": ("(a − b)(a + b)", "(a − b)(a + b)")] @0;
    ["front": ("(a + b)² = ?", "(a + b)² = ?"), "back": ("a² + 2ab + b²", "a² + 2ab + b²")] @0;
    ["front": ("Pythagorean theorem (right triangle, hypotenuse c)", "Pisagor bağıntısı (dik üçgen, hipotenüs c)"), "back": ("a² + b² = c²", "a² + b² = c²")] @0;
    ["front": ("0! = ?", "0! = ?"), "back": ("1", "1")] @0;
    ["front": ("When is a number divisible by 3?", "Bir sayı 3'e ne zaman tam bölünür?"), "back": ("When the sum of its digits is a multiple of 3.", "Rakamları toplamı 3'ün katı olduğunda.")] @0;
    ["front": ("When is a number divisible by 9?", "Bir sayı 9'a ne zaman tam bölünür?"), "back": ("When the sum of its digits is a multiple of 9.", "Rakamları toplamı 9'un katı olduğunda.")] @0;
    ["front": ("Circumference of a circle with radius r", "Yarıçapı r olan çemberin çevresi"), "back": ("2πr", "2πr")] @0;
    ["front": ("Area of a circle with radius r", "Yarıçapı r olan dairenin alanı"), "back": ("πr²", "πr²")] @0;
    ["front": ("1 + 2 + … + n = ?", "1 + 2 + … + n = ?"), "back": ("n(n + 1) / 2", "n(n + 1) / 2")] @0;
    ["front": ("15% of 200", "200'ün %15'i"), "back": ("30", "30")] @0;
];

// Difficulty option indexes: easy = 0, medium = 1, hard = 2.
const CAPITALS: &[StarterNote] = notes![
    ["front": ("France", "Fransa"), "back": ("Paris", "Paris")] @0;
    ["front": ("Japan", "Japonya"), "back": ("Tokyo", "Tokyo")] @0;
    ["front": ("Italy", "İtalya"), "back": ("Rome", "Roma")] @0;
    ["front": ("Germany", "Almanya"), "back": ("Berlin", "Berlin")] @0;
    ["front": ("Spain", "İspanya"), "back": ("Madrid", "Madrid")] @0;
    ["front": ("Egypt", "Mısır"), "back": ("Cairo", "Kahire")] @0;
    ["front": ("Türkiye", "Türkiye"), "back": ("Ankara", "Ankara")] @0;
    ["front": ("United Kingdom", "Birleşik Krallık"), "back": ("London", "Londra")] @0;
    ["front": ("Canada", "Kanada"), "back": ("Ottawa", "Ottava")] @1;
    ["front": ("Australia", "Avustralya"), "back": ("Canberra", "Kanberra")] @1;
    ["front": ("Brazil", "Brezilya"), "back": ("Brasília", "Brasilia")] @1;
    ["front": ("Morocco", "Fas"), "back": ("Rabat", "Rabat")] @1;
    ["front": ("South Korea", "Güney Kore"), "back": ("Seoul", "Seul")] @1;
    ["front": ("Argentina", "Arjantin"), "back": ("Buenos Aires", "Buenos Aires")] @1;
    ["front": ("Norway", "Norveç"), "back": ("Oslo", "Oslo")] @1;
    ["front": ("Kenya", "Kenya"), "back": ("Nairobi", "Nairobi")] @1;
    ["front": ("Kazakhstan", "Kazakistan"), "back": ("Astana", "Astana")] @2;
    ["front": ("Nigeria", "Nijerya"), "back": ("Abuja", "Abuja")] @2;
    ["front": ("New Zealand", "Yeni Zelanda"), "back": ("Wellington", "Wellington")] @2;
    ["front": ("Switzerland", "İsviçre"), "back": ("Bern", "Bern")] @2;
    ["front": ("Mongolia", "Moğolistan"), "back": ("Ulaanbaatar", "Ulan Batur")] @2;
    ["front": ("Myanmar", "Myanmar"), "back": ("Naypyidaw", "Nepido")] @2;
];

const GUIDE: &[StarterNote] = notes![
    ["front": ("How do I rate a card after seeing the answer?", "Cevabı gördükten sonra kartı nasıl puanlarım?"), "back": ("Keys 1–4: 1 Again, 2 Hard, 3 Good, 4 Easy. Be honest: the schedule adapts to you.", "1–4 tuşları: 1 Tekrar, 2 Zor, 3 İyi, 4 Kolay. Dürüst ol: takvim sana uyar.")];
    ["front": ("How do I undo my last action?", "Son işlemi nasıl geri alırım?"), "back": ("Ctrl+Z (redo: Ctrl+Shift+Z). Answers, edits and deletions can all be undone.", "Ctrl+Z (yinele: Ctrl+Shift+Z). Cevaplar, düzenlemeler ve silmeler geri alınabilir.")];
    ["front": ("How do I open the command palette?", "Komut paletini nasıl açarım?"), "back": ("Ctrl+K. Type what you want to do.", "Ctrl+K. Ne yapmak istediğini yaz.")];
    ["front": ("How do I add a new note?", "Yeni not nasıl eklerim?"), "back": ("Ctrl+N, or \"Add\" in the menu.", "Ctrl+N ya da menüde \"Ekle\".")];
    ["front": ("What is a classification?", "Sınıflandırma nedir?"), "back": ("A structured label you define, such as Level (A1–C2) or Exam (TYT/AYT). Sub-decks inherit it.", "Senin tanımladığın yapılı bir etiket: Seviye (A1–C2) ya da Sınav (TYT/AYT) gibi. Alt desteler miras alır.")];
    ["front": ("How do I find cards of one level?", "Bir seviyedeki kartları nasıl bulurum?"), "back": ("Search level:B1, or level>=B1 for B1 and above.", "seviye:B1 ya da B1 ve üstü için seviye>=B1 ara.")];
    ["front": ("How do I flag a card?", "Karta nasıl bayrak koyarım?"), "back": ("Ctrl+1 … Ctrl+7 while studying.", "Çalışırken Ctrl+1 … Ctrl+7.")];
    ["front": ("I deleted a deck by mistake. What now?", "Bir desteyi yanlışlıkla sildim. Ne yapmalıyım?"), "back": ("Undo right away, or restore it from the trash later.", "Hemen geri al ya da daha sonra çöp kutusundan geri yükle.")];
    ["front": ("Can two people use the app on one computer?", "Aynı bilgisayarda iki kişi kullanabilir mi?"), "back": ("Yes: each profile has its own collection, settings and plugins. Profiles can be locked with a PIN.", "Evet: her profilin kendi koleksiyonu, ayarları ve eklentileri vardır. Profiller PIN ile kilitlenebilir.")];
    ["front": ("Can the app remind me to study?", "Uygulama bana çalışmayı hatırlatabilir mi?"), "back": ("Yes: turn on reminders and pick a time and days. No reminder on days you already studied.", "Evet: hatırlatıcıları aç, saat ve gün seç. Çalıştığın gün hatırlatma gelmez.")];
    ["front": ("Can I delete this guide deck?", "Bu rehber destesini silebilir miyim?"), "back": ("Of course. Starter decks are ordinary decks.", "Elbette. Başlangıç desteleri sıradan destelerdir.")];
];

const DECKS: &[StarterDeck] = &[
    StarterDeck {
        id: "english_basics",
        name: t(
            "Basic English words (A1–A2)",
            "Temel İngilizce kelimeler (A1–A2)",
        ),
        description: t(
            "30 everyday words with meanings and example sentences.",
            "Anlamı ve örnek cümlesiyle 30 günlük kelime.",
        ),
        goal: StarterGoal::Language,
        notetype: "vocabulary",
        template: Some("cefr"),
        notes: ENGLISH,
    },
    StarterDeck {
        id: "yks_tyt_math",
        name: t("YKS – TYT math basics", "YKS – TYT matematik temelleri"),
        description: t(
            "Formulas and rules that come up again and again.",
            "Sık karşına çıkacak formüller ve kurallar.",
        ),
        goal: StarterGoal::Exam,
        notetype: "classic",
        template: Some("yks"),
        notes: YKS_MATH,
    },
    StarterDeck {
        id: "world_capitals",
        name: t("World capitals", "Dünya başkentleri"),
        description: t(
            "Countries and their capitals, from easy to hard.",
            "Kolaydan zora ülkeler ve başkentleri.",
        ),
        goal: StarterGoal::General,
        notetype: "classic",
        template: Some("difficulty"),
        notes: CAPITALS,
    },
    StarterDeck {
        id: "app_guide",
        name: t("App guide", "Uygulama rehberi"),
        description: t(
            "Learn the app the way you will learn everything else: with cards.",
            "Uygulamayı her şeyi öğreneceğin gibi öğren: kartlarla.",
        ),
        goal: StarterGoal::App,
        notetype: "classic",
        template: None,
        notes: GUIDE,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribute::find_template;
    use crate::content::builtin;
    use std::collections::HashSet;

    #[test]
    fn every_deck_is_consistent_with_its_note_type_and_template() {
        let mut ids = HashSet::new();
        for d in all() {
            assert!(ids.insert(d.id), "duplicate {}", d.id);
            assert!(!d.notes.is_empty());
            let nt = builtin::find(d.notetype).expect("built-in note type");
            let keys: Vec<String> = nt
                .build(crate::Timestamp(0), Locale::En)
                .fields
                .iter()
                .map(|f| f.key.as_str().to_owned())
                .collect();
            let options = d
                .template
                .map(|t| find_template(t).expect("template").options.len());
            for (i, n) in d.notes.iter().enumerate() {
                for (k, v) in n.fields {
                    assert!(
                        keys.iter().any(|f| f == k),
                        "{}#{i}: unknown field {k}",
                        d.id
                    );
                    assert!(!v.en.trim().is_empty() && !v.tr.trim().is_empty());
                }
                match (options, n.option) {
                    (Some(len), Some(o)) => assert!(o < len, "{}#{i}", d.id),
                    (None, None) => {}
                    other => panic!("{}#{i}: template/value mismatch {other:?}", d.id),
                }
            }
            for locale in [Locale::En, Locale::Tr] {
                assert!(!d.name.get(locale).is_empty());
                // Unique first fields per locale: notes are not duplicates of each other.
                let firsts: HashSet<_> = d.notes.iter().map(|n| n.fields_in(locale)[0].1).collect();
                assert_eq!(
                    firsts.len(),
                    d.notes.len(),
                    "{} duplicates in {locale:?}",
                    d.id
                );
            }
        }
    }

    #[test]
    fn starter_decks_are_small() {
        for d in all() {
            assert!(d.notes.len() <= 50, "{} too big for a starter deck", d.id);
        }
        assert!(find("english_basics").is_some() && find("nope").is_none());
    }
}
