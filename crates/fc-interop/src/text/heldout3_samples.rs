//! Third held-out separator-detection set (v1.0): written by a separate agent
//! that never looked at the detector source or its earlier test sets, modelled
//! on what Turkish/English learners actually paste from Quizlet, Anki, Excel,
//! notes apps and word lists. It was written before any detector change was
//! measured against it, so it serves as an independent check, not a tuning
//! target.

/// `(label, text, expected rows)`.
pub(super) fn samples() -> Vec<(&'static str, &'static str, Vec<Vec<String>>)> {
    vec![
        // ------------------------------------------------------------------
        // Vocabulary pairs
        // ------------------------------------------------------------------
        (
            "vocab comma-space",
            "elma, apple\narmut, pear\nkiraz, cherry",
            r(&[&["elma", "apple"], &["armut", "pear"], &["kiraz", "cherry"]]),
        ),
        (
            "vocab tab tr-en",
            "kitap\tbook\nkalem\tpen\ndefter\tnotebook\nsilgi\teraser",
            r(&[
                &["kitap", "book"],
                &["kalem", "pen"],
                &["defter", "notebook"],
                &["silgi", "eraser"],
            ]),
        ),
        (
            "vocab bare semicolon",
            "ev;house\naraba;car\nyol;road\nköprü;bridge",
            r(&[
                &["ev", "house"],
                &["araba", "car"],
                &["yol", "road"],
                &["köprü", "bridge"],
            ]),
        ),
        (
            "vocab colon-space animals",
            "köpek: dog\nkedi: cat\nkuş: bird\nbalık: fish",
            r(&[
                &["köpek", "dog"],
                &["kedi", "cat"],
                &["kuş", "bird"],
                &["balık", "fish"],
            ]),
        ),
        (
            "vocab spaced dash verbs",
            "gülmek - to laugh\nağlamak - to cry\nkoşmak - to run\nyürümek - to walk",
            r(&[
                &["gülmek", "to laugh"],
                &["ağlamak", "to cry"],
                &["koşmak", "to run"],
                &["yürümek", "to walk"],
            ]),
        ),
        (
            "vocab german equals spaced",
            "Hund = dog\nKatze = cat\nMaus = mouse\nPferd = horse",
            r(&[
                &["Hund", "dog"],
                &["Katze", "cat"],
                &["Maus", "mouse"],
                &["Pferd", "horse"],
            ]),
        ),
        (
            "vocab spanish ascii arrow",
            "hola -> hello\nadiós -> goodbye\ngracias -> thank you\npor favor -> please",
            r(&[
                &["hola", "hello"],
                &["adiós", "goodbye"],
                &["gracias", "thank you"],
                &["por favor", "please"],
            ]),
        ),
        (
            "vocab romaji unicode arrow",
            "neko → cat\ninu → dog\ntori → bird\nsakana → fish\numa → horse",
            r(&[
                &["neko", "cat"],
                &["inu", "dog"],
                &["tori", "bird"],
                &["sakana", "fish"],
                &["uma", "horse"],
            ]),
        ),
        (
            "vocab german double colon",
            "der Tisch :: the table\ndie Lampe :: the lamp\ndas Fenster :: the window",
            r(&[
                &["der Tisch", "the table"],
                &["die Lampe", "the lamp"],
                &["das Fenster", "the window"],
            ]),
        ),
        (
            "vocab bare pipe",
            "water|su\nfire|ateş\nearth|toprak\nair|hava",
            r(&[
                &["water", "su"],
                &["fire", "ateş"],
                &["earth", "toprak"],
                &["air", "hava"],
            ]),
        ),
        (
            "vocab spaced pipe phrases",
            "to borrow | ödünç almak\nto lend | ödünç vermek\nto owe | borçlu olmak",
            r(&[
                &["to borrow", "ödünç almak"],
                &["to lend", "ödünç vermek"],
                &["to owe", "borçlu olmak"],
            ]),
        ),
        (
            "vocab german plurals tab",
            "der Apfel, die Äpfel\telma\ndas Buch, die Bücher\tkitap\ndie Frau, die Frauen\tkadın\nder Stuhl, die Stühle\tsandalye",
            r(&[
                &["der Apfel, die Äpfel", "elma"],
                &["das Buch, die Bücher", "kitap"],
                &["die Frau, die Frauen", "kadın"],
                &["der Stuhl, die Stühle", "sandalye"],
            ]),
        ),
        (
            "vocab ipa three tab columns",
            "thought\t/θɔːt/\tdüşünce\nthrough\t/θruː/\tiçinden\nthorough\t/ˈθʌrə/\tdikkatli\nthough\t/ðəʊ/\tgerçi",
            r(&[
                &["thought", "/θɔːt/", "düşünce"],
                &["through", "/θruː/", "içinden"],
                &["thorough", "/ˈθʌrə/", "dikkatli"],
                &["though", "/ðəʊ/", "gerçi"],
            ]),
        ),
        (
            "vocab romaji greetings colon",
            "ohayou gozaimasu: good morning\nkonnichiwa: hello\nkonbanwa: good evening\noyasuminasai: good night",
            r(&[
                &["ohayou gozaimasu", "good morning"],
                &["konnichiwa", "hello"],
                &["konbanwa", "good evening"],
                &["oyasuminasai", "good night"],
            ]),
        ),
        (
            "vocab spanish dash with comma meanings",
            "el perro - the dog\nla casa - the house, the home\nel coche - the car, the automobile\nla mesa - the table",
            r(&[
                &["el perro", "the dog"],
                &["la casa", "the house, the home"],
                &["el coche", "the car, the automobile"],
                &["la mesa", "the table"],
            ]),
        ),
        (
            "vocab bare comma adjectives",
            "big,büyük\nsmall,küçük\ntall,uzun\nshort,kısa\nwide,geniş",
            r(&[
                &["big", "büyük"],
                &["small", "küçük"],
                &["tall", "uzun"],
                &["short", "kısa"],
                &["wide", "geniş"],
            ]),
        ),
        (
            "vocab tab multiple meanings with commas",
            "run\tkoşmak, işletmek, çalıştırmak\nset\tkoymak, ayarlamak\nget\talmak, elde etmek, anlamak\ntake\talmak, götürmek",
            r(&[
                &["run", "koşmak, işletmek, çalıştırmak"],
                &["set", "koymak, ayarlamak"],
                &["get", "almak, elde etmek, anlamak"],
                &["take", "almak, götürmek"],
            ]),
        ),
        (
            "vocab colon synonyms with commas",
            "abundant: plentiful, ample\nbenevolent: kind, generous\ncandid: frank, honest, open\ndiligent: hard-working, careful",
            r(&[
                &["abundant", "plentiful, ample"],
                &["benevolent", "kind, generous"],
                &["candid", "frank, honest, open"],
                &["diligent", "hard-working, careful"],
            ]),
        ),
        (
            "vocab semicolon-space with comma defs",
            "ubiquitous; found everywhere, omnipresent\nephemeral; short-lived, fleeting\nmeticulous; very careful, precise\nlethargic; sluggish, drowsy",
            r(&[
                &["ubiquitous", "found everywhere, omnipresent"],
                &["ephemeral", "short-lived, fleeting"],
                &["meticulous", "very careful, precise"],
                &["lethargic", "sluggish, drowsy"],
            ]),
        ),
        (
            "vocab spanish numbers bare equals",
            "uno=one\ndos=two\ntres=three\ncuatro=four\ncinco=five",
            r(&[
                &["uno", "one"],
                &["dos", "two"],
                &["tres", "three"],
                &["cuatro", "four"],
                &["cinco", "five"],
            ]),
        ),
        (
            "vocab phrasal verbs tab",
            "give up\tvazgeçmek\nlook after\tbakmak, ilgilenmek\nput off\tertelemek\nturn down\treddetmek",
            r(&[
                &["give up", "vazgeçmek"],
                &["look after", "bakmak, ilgilenmek"],
                &["put off", "ertelemek"],
                &["turn down", "reddetmek"],
            ]),
        ),
        (
            "vocab hyphenated words with spaced dash",
            "well-known - tanınmış\nself-confident - kendine güvenen\nup-to-date - güncel\nlong-term - uzun vadeli",
            r(&[
                &["well-known", "tanınmış"],
                &["self-confident", "kendine güvenen"],
                &["up-to-date", "güncel"],
                &["long-term", "uzun vadeli"],
            ]),
        ),
        (
            "vocab french spaced colon",
            "l'arbre : the tree\nla fleur : the flower\nle soleil : the sun\nla lune : the moon",
            r(&[
                &["l'arbre", "the tree"],
                &["la fleur", "the flower"],
                &["le soleil", "the sun"],
                &["la lune", "the moon"],
            ]),
        ),
        (
            "vocab crlf tab furniture",
            "masa\ttable\r\nsandalye\tchair\r\nkapı\tdoor\r\npencere\twindow\r\n",
            r(&[
                &["masa", "table"],
                &["sandalye", "chair"],
                &["kapı", "door"],
                &["pencere", "window"],
            ]),
        ),
        (
            "vocab quoted irregular verb forms csv",
            "\"to get, got, gotten\",almak\n\"to go, went, gone\",gitmek\n\"to see, saw, seen\",görmek\n\"to be, was/were, been\",olmak",
            r(&[
                &["to get, got, gotten", "almak"],
                &["to go, went, gone", "gitmek"],
                &["to see, saw, seen", "görmek"],
                &["to be, was/were, been", "olmak"],
            ]),
        ),
        (
            "vocab part of speech in parens colon",
            "çalışkan (adj): hard-working\ntembel (adj): lazy\nçalışmak (v): to work\niş (n): job, work",
            r(&[
                &["çalışkan (adj)", "hard-working"],
                &["tembel (adj)", "lazy"],
                &["çalışmak (v)", "to work"],
                &["iş (n)", "job, work"],
            ]),
        ),
        (
            "vocab multiword comma-space food",
            "ice cream, dondurma\nhot dog, sosisli\nfrench fries, patates kızartması\nscrambled eggs, çırpılmış yumurta",
            r(&[
                &["ice cream", "dondurma"],
                &["hot dog", "sosisli"],
                &["french fries", "patates kızartması"],
                &["scrambled eggs", "çırpılmış yumurta"],
            ]),
        ),
        (
            "vocab fat arrow colours",
            "red => kırmızı\nblue => mavi\ngreen => yeşil\npurple => mor",
            r(&[
                &["red", "kırmızı"],
                &["blue", "mavi"],
                &["green", "yeşil"],
                &["purple", "mor"],
            ]),
        ),
        (
            "vocab bare hyphen pairs",
            "cat-kedi\ndog-köpek\nbird-kuş\nhorse-at\ncow-inek",
            r(&[
                &["cat", "kedi"],
                &["dog", "köpek"],
                &["bird", "kuş"],
                &["horse", "at"],
                &["cow", "inek"],
            ]),
        ),
        (
            "vocab kanji reading meaning tab",
            "水\tmizu\twater\n火\thi\tfire\n山\tyama\tmountain\n川\tkawa\triver\n木\tki\ttree",
            r(&[
                &["水", "mizu", "water"],
                &["火", "hi", "fire"],
                &["山", "yama", "mountain"],
                &["川", "kawa", "river"],
                &["木", "ki", "tree"],
            ]),
        ),
        (
            "vocab korean slash three fields",
            "안녕하세요 / annyeonghaseyo / hello\n감사합니다 / gamsahamnida / thank you\n네 / ne / yes\n아니요 / aniyo / no",
            r(&[
                &["안녕하세요", "annyeonghaseyo", "hello"],
                &["감사합니다", "gamsahamnida", "thank you"],
                &["네", "ne", "yes"],
                &["아니요", "aniyo", "no"],
            ]),
        ),
        (
            "vocab english-first colon with articles in def",
            "the weather: hava durumu\na forecast: tahmin\nthe rain: yağmur\na storm: fırtına\nthe wind: rüzgar",
            r(&[
                &["the weather", "hava durumu"],
                &["a forecast", "tahmin"],
                &["the rain", "yağmur"],
                &["a storm", "fırtına"],
                &["the wind", "rüzgar"],
            ]),
        ),
        (
            "vocab german semicolon-space with comma plurals",
            "das Kind, die Kinder; çocuk\nder Mann, die Männer; adam\ndie Stadt, die Städte; şehir",
            r(&[
                &["das Kind, die Kinder", "çocuk"],
                &["der Mann, die Männer", "adam"],
                &["die Stadt, die Städte", "şehir"],
            ]),
        ),
        (
            "vocab long tab list",
            "accomplish\tbaşarmak\nacquire\tedinmek\nadapt\tuyum sağlamak\nadvocate\tsavunmak\nallocate\tayırmak\nanticipate\tbeklemek, öngörmek\nassess\tdeğerlendirmek\nassume\tvarsaymak\nattain\tulaşmak\nbenefit\tfayda",
            r(&[
                &["accomplish", "başarmak"],
                &["acquire", "edinmek"],
                &["adapt", "uyum sağlamak"],
                &["advocate", "savunmak"],
                &["allocate", "ayırmak"],
                &["anticipate", "beklemek, öngörmek"],
                &["assess", "değerlendirmek"],
                &["assume", "varsaymak"],
                &["attain", "ulaşmak"],
                &["benefit", "fayda"],
            ]),
        ),
        // ------------------------------------------------------------------
        // CSV / TSV with 3+ columns
        // ------------------------------------------------------------------
        (
            "csv header front back tags",
            "front,back,tags\nelma,apple,food\narmut,pear,food\nmasa,table,furniture",
            r(&[
                &["front", "back", "tags"],
                &["elma", "apple", "food"],
                &["armut", "pear", "food"],
                &["masa", "table", "furniture"],
            ]),
        ),
        (
            "excel tr semicolon decimal comma constants",
            "Sabit;Değer;Birim\npi;3,14;\ne;2,718;\nyerçekimi;9,81;m/s²",
            r(&[
                &["Sabit", "Değer", "Birim"],
                &["pi", "3,14", ""],
                &["e", "2,718", ""],
                &["yerçekimi", "9,81", "m/s²"],
            ]),
        ),
        (
            "tsv irregular verbs four columns",
            "base\tpast\tparticiple\tanlam\ngo\twent\tgone\tgitmek\neat\tate\teaten\tyemek\nwrite\twrote\twritten\tyazmak",
            r(&[
                &["base", "past", "participle", "anlam"],
                &["go", "went", "gone", "gitmek"],
                &["eat", "ate", "eaten", "yemek"],
                &["write", "wrote", "written", "yazmak"],
            ]),
        ),
        (
            "csv quoted field with newline",
            "term,definition\nphotosynthesis,\"the process by which plants make food\nusing sunlight\"\nosmosis,\"movement of water, through a membrane\"\ndiffusion,spreading of particles",
            r(&[
                &["term", "definition"],
                &[
                    "photosynthesis",
                    "the process by which plants make food\nusing sunlight",
                ],
                &["osmosis", "movement of water, through a membrane"],
                &["diffusion", "spreading of particles"],
            ]),
        ),
        (
            "csv doubled quotes in examples",
            "word,example\nsay,\"He said \"\"hello\"\" to me\"\ntell,\"She told me \"\"no\"\"\"\nask,\"I asked, politely\"",
            r(&[
                &["word", "example"],
                &["say", "He said \"hello\" to me"],
                &["tell", "She told me \"no\""],
                &["ask", "I asked, politely"],
            ]),
        ),
        (
            "excel tr semicolon history questions",
            "Soru;Cevap;Ünite\n\"Osmanlı'nın kurucusu kimdir?\";Osman Bey;1\n\"İstanbul ne zaman fethedildi?\";1453;2\n\"Lale Devri hangi padişah döneminde?\";III. Ahmet;3",
            r(&[
                &["Soru", "Cevap", "Ünite"],
                &["Osmanlı'nın kurucusu kimdir?", "Osman Bey", "1"],
                &["İstanbul ne zaman fethedildi?", "1453", "2"],
                &["Lale Devri hangi padişah döneminde?", "III. Ahmet", "3"],
            ]),
        ),
        (
            "tsv country capital population",
            "Ülke\tBaşkent\tNüfus\nTürkiye\tAnkara\t85.000.000\nAlmanya\tBerlin\t83.200.000\nFransa\tParis\t68.000.000",
            r(&[
                &["Ülke", "Başkent", "Nüfus"],
                &["Türkiye", "Ankara", "85.000.000"],
                &["Almanya", "Berlin", "83.200.000"],
                &["Fransa", "Paris", "68.000.000"],
            ]),
        ),
        (
            "csv anki export with html",
            "<b>apple</b>,elma,<i>noun</i>\n<b>run</b>,koşmak,<i>verb</i>\n<b>quick</b>,hızlı,<i>adjective</i>",
            r(&[
                &["<b>apple</b>", "elma", "<i>noun</i>"],
                &["<b>run</b>", "koşmak", "<i>verb</i>"],
                &["<b>quick</b>", "hızlı", "<i>adjective</i>"],
            ]),
        ),
        (
            "tsv anki export html with commas",
            "Hund\tdog<br>(der, -e)\nKatze\tcat<br>(die, -n)\nHaus\thouse<br>(das, ¨er)\nBaum\ttree<br>(der, ¨e)",
            r(&[
                &["Hund", "dog<br>(der, -e)"],
                &["Katze", "cat<br>(die, -n)"],
                &["Haus", "house<br>(das, ¨er)"],
                &["Baum", "tree<br>(der, ¨e)"],
            ]),
        ),
        (
            "csv five columns elements",
            "Sembol,Ad,Name,Atom No,Grup\nH,Hidrojen,Hydrogen,1,1\nHe,Helyum,Helium,2,18\nLi,Lityum,Lithium,3,1\nC,Karbon,Carbon,6,14",
            r(&[
                &["Sembol", "Ad", "Name", "Atom No", "Grup"],
                &["H", "Hidrojen", "Hydrogen", "1", "1"],
                &["He", "Helyum", "Helium", "2", "18"],
                &["Li", "Lityum", "Lithium", "3", "1"],
                &["C", "Karbon", "Carbon", "6", "14"],
            ]),
        ),
        (
            "csv empty middle field",
            "word,ipa,meaning\ncat,/kæt/,kedi\ndog,,köpek\nbird,/bɜːd/,kuş",
            r(&[
                &["word", "ipa", "meaning"],
                &["cat", "/kæt/", "kedi"],
                &["dog", "", "köpek"],
                &["bird", "/bɜːd/", "kuş"],
            ]),
        ),
        (
            "excel tr semicolon prices",
            "Ürün;Fiyat;Adet\nElma;12,50;3\nArmut;15,75;2\nMuz;32,00;6",
            r(&[
                &["Ürün", "Fiyat", "Adet"],
                &["Elma", "12,50", "3"],
                &["Armut", "15,75", "2"],
                &["Muz", "32,00", "6"],
            ]),
        ),
        (
            "csv crlf header with sentences",
            "English,Turkish,Example\r\nhappy,mutlu,I am happy.\r\nsad,üzgün,She looks sad.\r\nangry,kızgın,Don't be angry.\r\n",
            r(&[
                &["English", "Turkish", "Example"],
                &["happy", "mutlu", "I am happy."],
                &["sad", "üzgün", "She looks sad."],
                &["angry", "kızgın", "Don't be angry."],
            ]),
        ),
        (
            "spaced pipe three columns",
            "abate | verb | azalmak\nbrisk | adj | canlı, hızlı\ncogent | adj | ikna edici\ndearth | noun | kıtlık",
            r(&[
                &["abate", "verb", "azalmak"],
                &["brisk", "adj", "canlı, hızlı"],
                &["cogent", "adj", "ikna edici"],
                &["dearth", "noun", "kıtlık"],
            ]),
        ),
        (
            "csv quoted example sentences with commas",
            "word,meaning,example\nalthough,rağmen,\"Although it rained, we went out.\"\nhowever,ancak,\"It's cheap; however, it's slow.\"\ntherefore,bu yüzden,\"I think, therefore I am.\"",
            r(&[
                &["word", "meaning", "example"],
                &["although", "rağmen", "Although it rained, we went out."],
                &["however", "ancak", "It's cheap; however, it's slow."],
                &["therefore", "bu yüzden", "I think, therefore I am."],
            ]),
        ),
        // ------------------------------------------------------------------
        // Definitions / sentences with punctuation
        // ------------------------------------------------------------------
        (
            "definitions colon with commas and parens",
            "Mitochondria: the powerhouse of the cell (produces ATP, via respiration)\nRibosome: site of protein synthesis\nNucleus: contains DNA, controls the cell\nVacuole: storage (water, nutrients, waste)",
            r(&[
                &[
                    "Mitochondria",
                    "the powerhouse of the cell (produces ATP, via respiration)",
                ],
                &["Ribosome", "site of protein synthesis"],
                &["Nucleus", "contains DNA, controls the cell"],
                &["Vacuole", "storage (water, nutrients, waste)"],
            ]),
        ),
        (
            "literary terms tab with colons",
            "Simile\tA comparison using 'like' or 'as': brave as a lion\nMetaphor\tA direct comparison: time is money\nHyperbole\tExaggeration for effect, e.g. I've told you a million times\nIrony\tSaying the opposite of what is meant",
            r(&[
                &["Simile", "A comparison using 'like' or 'as': brave as a lion"],
                &["Metaphor", "A direct comparison: time is money"],
                &[
                    "Hyperbole",
                    "Exaggeration for effect, e.g. I've told you a million times",
                ],
                &["Irony", "Saying the opposite of what is meant"],
            ]),
        ),
        (
            "definitions spaced dash repeated in value",
            "Empathy - the ability to understand others' feelings\nApathy - lack of interest, enthusiasm, or concern\nSympathy - feeling sorry for someone - not the same as empathy",
            r(&[
                &["Empathy", "the ability to understand others' feelings"],
                &["Apathy", "lack of interest, enthusiasm, or concern"],
                &["Sympathy", "feeling sorry for someone - not the same as empathy"],
            ]),
        ),
        (
            "turkish civics definitions colon",
            "Demokrasi: Halkın kendi kendini yönetmesi, egemenliğin halka ait olması.\nCumhuriyet: Devlet başkanının seçimle belirlendiği yönetim biçimi.\nLaiklik: Din ve devlet işlerinin birbirinden ayrılması.",
            r(&[
                &[
                    "Demokrasi",
                    "Halkın kendi kendini yönetmesi, egemenliğin halka ait olması.",
                ],
                &[
                    "Cumhuriyet",
                    "Devlet başkanının seçimle belirlendiği yönetim biçimi.",
                ],
                &["Laiklik", "Din ve devlet işlerinin birbirinden ayrılması."],
            ]),
        ),
        (
            "sentence translations tab with commas",
            "I'm tired, but I'll come.\tYorgunum ama geleceğim.\nIf it rains, we'll stay home.\tYağmur yağarsa evde kalırız.\nWhen I was young, I lived in Izmir.\tGençken İzmir'de yaşadım.",
            r(&[
                &["I'm tired, but I'll come.", "Yorgunum ama geleceğim."],
                &["If it rains, we'll stay home.", "Yağmur yağarsa evde kalırız."],
                &["When I was young, I lived in Izmir.", "Gençken İzmir'de yaşadım."],
            ]),
        ),
        (
            "legal terms em dash",
            "Davacı — dava açan taraf\nDavalı — aleyhine dava açılan taraf\nTanık — olayı gören, bilgi veren kişi\nBilirkişi — uzman görüşü veren kişi",
            r(&[
                &["Davacı", "dava açan taraf"],
                &["Davalı", "aleyhine dava açılan taraf"],
                &["Tanık", "olayı gören, bilgi veren kişi"],
                &["Bilirkişi", "uzman görüşü veren kişi"],
            ]),
        ),
        (
            "medical abbreviations colon",
            "BP: blood pressure (mmHg)\nHR: heart rate, beats/min\nRR: respiratory rate\nSpO2: oxygen saturation, normal 95-100%",
            r(&[
                &["BP", "blood pressure (mmHg)"],
                &["HR", "heart rate, beats/min"],
                &["RR", "respiratory rate"],
                &["SpO2", "oxygen saturation, normal 95-100%"],
            ]),
        ),
        (
            "quotes with dash attribution",
            "\"I think, therefore I am.\" - Descartes\n\"The unexamined life is not worth living.\" - Socrates\n\"God is dead.\" - Nietzsche\n\"Know thyself, then.\" - Thales",
            r(&[
                &["I think, therefore I am.", "Descartes"],
                &["The unexamined life is not worth living.", "Socrates"],
                &["God is dead.", "Nietzsche"],
                &["Know thyself, then.", "Thales"],
            ]),
        ),
        (
            "grammar tenses colon with slashes",
            "Present Perfect: have/has + V3 (I have eaten)\nPast Simple: V2 (I ate)\nFuture Simple: will + V1 (I will eat)\nPast Perfect: had + V3 (I had eaten)",
            r(&[
                &["Present Perfect", "have/has + V3 (I have eaten)"],
                &["Past Simple", "V2 (I ate)"],
                &["Future Simple", "will + V1 (I will eat)"],
                &["Past Perfect", "had + V3 (I had eaten)"],
            ]),
        ),
        (
            "punctuation terms tab with mixed punctuation",
            "semicolon\tjoins clauses; e.g. I came; I saw\ncolon\tintroduces a list: a, b, c\ncomma\tseparates items, clauses\ndash\tmarks a break - like this",
            r(&[
                &["semicolon", "joins clauses; e.g. I came; I saw"],
                &["colon", "introduces a list: a, b, c"],
                &["comma", "separates items, clauses"],
                &["dash", "marks a break - like this"],
            ]),
        ),
        // ------------------------------------------------------------------
        // Q&A / exam facts
        // ------------------------------------------------------------------
        (
            "history dates colon",
            "İstanbul'un Fethi: 1453\nMalazgirt Savaşı: 1071\nCumhuriyetin İlanı: 1923\nKurtuluş Savaşı: 1919-1922",
            r(&[
                &["İstanbul'un Fethi", "1453"],
                &["Malazgirt Savaşı", "1071"],
                &["Cumhuriyetin İlanı", "1923"],
                &["Kurtuluş Savaşı", "1919-1922"],
            ]),
        ),
        (
            "wars year ranges tab",
            "World War I\t1914-1918\nWorld War II\t1939-1945\nKorean War\t1950-1953\nCold War\t1947-1991",
            r(&[
                &["World War I", "1914-1918"],
                &["World War II", "1939-1945"],
                &["Korean War", "1950-1953"],
                &["Cold War", "1947-1991"],
            ]),
        ),
        (
            "schedule times colon in value",
            "Toplantı: 10:30\nÖğle arası: 12:00-13:00\nSınav: 09:30\nKapanış: 17:45",
            r(&[
                &["Toplantı", "10:30"],
                &["Öğle arası", "12:00-13:00"],
                &["Sınav", "09:30"],
                &["Kapanış", "17:45"],
            ]),
        ),
        (
            "physics formulas colon with equals",
            "Kinetic energy: E = 1/2 mv^2\nMass-energy: E = mc^2\nPythagoras: a^2 + b^2 = c^2\nOhm's law: V = IR",
            r(&[
                &["Kinetic energy", "E = 1/2 mv^2"],
                &["Mass-energy", "E = mc^2"],
                &["Pythagoras", "a^2 + b^2 = c^2"],
                &["Ohm's law", "V = IR"],
            ]),
        ),
        (
            "trivia questions tab",
            "What is the capital of Australia?\tCanberra\nHow many bones in the adult human body?\t206\nWho wrote 1984?\tGeorge Orwell\nWhat is the boiling point of water?\t100 °C",
            r(&[
                &["What is the capital of Australia?", "Canberra"],
                &["How many bones in the adult human body?", "206"],
                &["Who wrote 1984?", "George Orwell"],
                &["What is the boiling point of water?", "100 °C"],
            ]),
        ),
        (
            "urls colon-space",
            "Rust docs: https://doc.rust-lang.org\nTauri: https://tauri.app\nMDN: https://developer.mozilla.org/en-US/\nCrates: https://crates.io",
            r(&[
                &["Rust docs", "https://doc.rust-lang.org"],
                &["Tauri", "https://tauri.app"],
                &["MDN", "https://developer.mozilla.org/en-US/"],
                &["Crates", "https://crates.io"],
            ]),
        ),
        (
            "ratios tab",
            "golden ratio\t1:1.618\nHD aspect ratio\t16:9\nA4 paper ratio\t1:√2\nold TV ratio\t4:3",
            r(&[
                &["golden ratio", "1:1.618"],
                &["HD aspect ratio", "16:9"],
                &["A4 paper ratio", "1:√2"],
                &["old TV ratio", "4:3"],
            ]),
        ),
        (
            "csv quoted exam questions",
            "question,answer\n\"Largest planet?\",Jupiter\n\"Smallest prime, greater than 10?\",11\n\"Chemical symbol for gold?\",Au",
            r(&[
                &["question", "answer"],
                &["Largest planet?", "Jupiter"],
                &["Smallest prime, greater than 10?", "11"],
                &["Chemical symbol for gold?", "Au"],
            ]),
        ),
        (
            "math formulas spaced pipe",
            "Area of circle | πr^2\nCircumference | 2πr\nVolume of sphere | 4/3 πr^3\nQuadratic formula | x = (-b ± √(b^2 - 4ac)) / 2a",
            r(&[
                &["Area of circle", "πr^2"],
                &["Circumference", "2πr"],
                &["Volume of sphere", "4/3 πr^3"],
                &["Quadratic formula", "x = (-b ± √(b^2 - 4ac)) / 2a"],
            ]),
        ),
        (
            "capitals spaced dash with hyphenated country",
            "Turkey - Ankara\nGermany - Berlin\nUnited States - Washington, D.C.\nBosnia and Herzegovina - Sarajevo\nGuinea-Bissau - Bissau",
            r(&[
                &["Turkey", "Ankara"],
                &["Germany", "Berlin"],
                &["United States", "Washington, D.C."],
                &["Bosnia and Herzegovina", "Sarajevo"],
                &["Guinea-Bissau", "Bissau"],
            ]),
        ),
        (
            "chemical reactions tab with arrows",
            "water formation\t2H2 + O2 → 2H2O\nphotosynthesis\t6CO2 + 6H2O → C6H12O6 + 6O2\nmethane combustion\tCH4 + 2O2 → CO2 + 2H2O",
            r(&[
                &["water formation", "2H2 + O2 → 2H2O"],
                &["photosynthesis", "6CO2 + 6H2O → C6H12O6 + 6O2"],
                &["methane combustion", "CH4 + 2O2 → CO2 + 2H2O"],
            ]),
        ),
        (
            "temperatures spaced dash with negative",
            "absolute zero - -273.15 °C\nwater freezes - 0 °C\nwater boils - 100 °C\nbody temperature - 37 °C",
            r(&[
                &["absolute zero", "-273.15 °C"],
                &["water freezes", "0 °C"],
                &["water boils", "100 °C"],
                &["body temperature", "37 °C"],
            ]),
        ),
        // ------------------------------------------------------------------
        // Single-column lists
        // ------------------------------------------------------------------
        (
            "single column words",
            "serendipity\nephemeral\nubiquitous\nresilience\neloquent",
            r(&[
                &["serendipity"],
                &["ephemeral"],
                &["ubiquitous"],
                &["resilience"],
                &["eloquent"],
            ]),
        ),
        (
            "single column sentences with commas",
            "However, I disagree.\nIn my opinion, it's too expensive.\nFirst, preheat the oven.\nWell, that depends.\nBy the way, where's the station?",
            r(&[
                &["However, I disagree."],
                &["In my opinion, it's too expensive."],
                &["First, preheat the oven."],
                &["Well, that depends."],
                &["By the way, where's the station?"],
            ]),
        ),
        (
            "single column hyphenated words",
            "mother-in-law\nwell-being\nself-esteem\nup-to-date\nforget-me-not",
            r(&[
                &["mother-in-law"],
                &["well-being"],
                &["self-esteem"],
                &["up-to-date"],
                &["forget-me-not"],
            ]),
        ),
        (
            "single column numbered list",
            "1. abandon\n2. abolish\n3. absorb\n4. abstract\n5. absurd",
            r(&[
                &["1. abandon"],
                &["2. abolish"],
                &["3. absorb"],
                &["4. abstract"],
                &["5. absurd"],
            ]),
        ),
        (
            "single column bullets",
            "• resilient\n• tenacious\n• meticulous\n• gregarious",
            r(&[
                &["• resilient"],
                &["• tenacious"],
                &["• meticulous"],
                &["• gregarious"],
            ]),
        ),
        (
            "single column turkish proverbs",
            "Damlaya damlaya göl olur.\nAyağını yorganına göre uzat.\nSakla samanı, gelir zamanı.\nİşleyen demir pışıl pışıl, işlemeyen demir paslanır.\nBir elin nesi var, iki elin sesi var.",
            r(&[
                &["Damlaya damlaya göl olur."],
                &["Ayağını yorganına göre uzat."],
                &["Sakla samanı, gelir zamanı."],
                &["İşleyen demir pışıl pışıl, işlemeyen demir paslanır."],
                &["Bir elin nesi var, iki elin sesi var."],
            ]),
        ),
        (
            "single column german nouns with articles",
            "der Tisch\ndie Lampe\ndas Fenster\nder Stuhl\ndie Tür",
            r(&[
                &["der Tisch"],
                &["die Lampe"],
                &["das Fenster"],
                &["der Stuhl"],
                &["die Tür"],
            ]),
        ),
        (
            "single column crlf trailing blank",
            "apple\r\nbanana\r\ncherry\r\n\r\n",
            r(&[&["apple"], &["banana"], &["cherry"]]),
        ),
        (
            "single column phrasal verbs",
            "look up\nlook after\nlook forward to\nlook down on\nlook into",
            r(&[
                &["look up"],
                &["look after"],
                &["look forward to"],
                &["look down on"],
                &["look into"],
            ]),
        ),
        (
            "single column anki cloze",
            "The {{c1::mitochondria}} is the powerhouse of the cell.\n{{c1::Ankara}} is the capital of {{c2::Turkey}}.\nWater boils at {{c1::100}} °C.\n{{c1::1453}} is when Istanbul was conquered.",
            r(&[
                &["The {{c1::mitochondria}} is the powerhouse of the cell."],
                &["{{c1::Ankara}} is the capital of {{c2::Turkey}}."],
                &["Water boils at {{c1::100}} °C."],
                &["{{c1::1453}} is when Istanbul was conquered."],
            ]),
        ),
        (
            "single column long word list",
            "abate\nbenign\ncandor\ndearth\neffigy\nfervent\ngarrulous\nhackneyed\nimpetuous\njudicious\nkindle\nlaconic",
            r(&[
                &["abate"],
                &["benign"],
                &["candor"],
                &["dearth"],
                &["effigy"],
                &["fervent"],
                &["garrulous"],
                &["hackneyed"],
                &["impetuous"],
                &["judicious"],
                &["kindle"],
                &["laconic"],
            ]),
        ),
        // ------------------------------------------------------------------
        // Blank-line separated / custom record separator
        // ------------------------------------------------------------------
        (
            "blank line colon multi-line defs",
            "cat: evcil hayvan\nmiyavlar\n\ndog: sadık hayvan\nhavlar\n\nbird: uçan hayvan\ncıvıldar",
            r(&[
                &["cat", "evcil hayvan\nmiyavlar"],
                &["dog", "sadık hayvan\nhavlar"],
                &["bird", "uçan hayvan\ncıvıldar"],
            ]),
        ),
        (
            "blank line tab multi-line defs",
            "photosynthesis\tbitkilerin besin üretmesi\nışık enerjisi kullanılır\n\nrespiration\thücresel solunum\nATP üretilir\n\nosmosis\tsuyun zardan geçişi",
            r(&[
                &[
                    "photosynthesis",
                    "bitkilerin besin üretmesi\nışık enerjisi kullanılır",
                ],
                &["respiration", "hücresel solunum\nATP üretilir"],
                &["osmosis", "suyun zardan geçişi"],
            ]),
        ),
        (
            "blank line poems single field",
            "Roses are red,\nviolets are blue.\n\nTwinkle, twinkle,\nlittle star.\n\nHumpty Dumpty sat on a wall,\nHumpty Dumpty had a great fall.",
            r(&[
                &["Roses are red,\nviolets are blue."],
                &["Twinkle, twinkle,\nlittle star."],
                &["Humpty Dumpty sat on a wall,\nHumpty Dumpty had a great fall."],
            ]),
        ),
        (
            "blank line pipe multi-line biology",
            "mitosis | cell division\nproduces 2 identical cells\n\nmeiosis | reduction division\nproduces 4 gametes\nhalves chromosome number\n\nbinary fission | bacterial division",
            r(&[
                &["mitosis", "cell division\nproduces 2 identical cells"],
                &[
                    "meiosis",
                    "reduction division\nproduces 4 gametes\nhalves chromosome number",
                ],
                &["binary fission", "bacterial division"],
            ]),
        ),
        (
            "blank line crlf colon",
            "sun: güneş\r\nyıldız olarak da bilinir\r\n\r\nmoon: ay\r\ndünyanın uydusu\r\n\r\nstar: yıldız\r\n",
            r(&[
                &["sun", "güneş\nyıldız olarak da bilinir"],
                &["moon", "ay\ndünyanın uydusu"],
                &["star", "yıldız"],
            ]),
        ),
        (
            "semicolon records comma fields",
            "elma,apple;armut,pear;kiraz,cherry;muz,banana",
            r(&[
                &["elma", "apple"],
                &["armut", "pear"],
                &["kiraz", "cherry"],
                &["muz", "banana"],
            ]),
        ),
        (
            "semicolon records colon fields spaced",
            "bir: one; iki: two; üç: three; dört: four; beş: five",
            r(&[
                &["bir", "one"],
                &["iki", "two"],
                &["üç", "three"],
                &["dört", "four"],
                &["beş", "five"],
            ]),
        ),
        (
            "semicolon records trailing semicolon",
            "red=kırmızı;blue=mavi;green=yeşil;",
            r(&[&["red", "kırmızı"], &["blue", "mavi"], &["green", "yeşil"]]),
        ),
        (
            "semicolon records days trailing newline",
            "Monday=Pazartesi; Tuesday=Salı; Wednesday=Çarşamba; Thursday=Perşembe\n",
            r(&[
                &["Monday", "Pazartesi"],
                &["Tuesday", "Salı"],
                &["Wednesday", "Çarşamba"],
                &["Thursday", "Perşembe"],
            ]),
        ),
        // ------------------------------------------------------------------
        // Messy real pastes
        // ------------------------------------------------------------------
        (
            "messy inconsistent colon spacing",
            "apple:elma\nbanana :  muz\n  cherry:kiraz  \ngrape  :üzüm",
            r(&[
                &["apple", "elma"],
                &["banana", "muz"],
                &["cherry", "kiraz"],
                &["grape", "üzüm"],
            ]),
        ),
        (
            "messy tab trailing empty lines",
            "one\tbir\ntwo\tiki\nthree\tüç\n\n",
            r(&[&["one", "bir"], &["two", "iki"], &["three", "üç"]]),
        ),
        (
            "messy missing definition dash",
            "sun - güneş\nmoon - ay\nstar\ncloud - bulut\nrain - yağmur",
            r(&[
                &["sun", "güneş"],
                &["moon", "ay"],
                &["star"],
                &["cloud", "bulut"],
                &["rain", "yağmur"],
            ]),
        ),
        (
            "messy aligned colon columns",
            "cat      : kedi\nelephant : fil\ndog      : köpek\nmouse    : fare",
            r(&[
                &["cat", "kedi"],
                &["elephant", "fil"],
                &["dog", "köpek"],
                &["mouse", "fare"],
            ]),
        ),
        (
            "messy tab padded with spaces",
            "kitap\t book\nkalem \tpen\n defter\tnotebook \nsilgi  \t  eraser",
            r(&[
                &["kitap", "book"],
                &["kalem", "pen"],
                &["defter", "notebook"],
                &["silgi", "eraser"],
            ]),
        ),
        (
            "messy tilde",
            "water ~ su\nfire ~ ateş\nice ~ buz\nsteam ~ buhar",
            r(&[
                &["water", "su"],
                &["fire", "ateş"],
                &["ice", "buz"],
                &["steam", "buhar"],
            ]),
        ),
        (
            "messy spaced slash organs",
            "kalp / heart\nbeyin / brain\nakciğer / lung\nböbrek / kidney",
            r(&[
                &["kalp", "heart"],
                &["beyin", "brain"],
                &["akciğer", "lung"],
                &["böbrek", "kidney"],
            ]),
        ),
        (
            "messy double angle",
            "hello >> merhaba\ngoodbye >> hoşçakal\nthanks >> teşekkürler\nsorry >> özür dilerim",
            r(&[
                &["hello", "merhaba"],
                &["goodbye", "hoşçakal"],
                &["thanks", "teşekkürler"],
                &["sorry", "özür dilerim"],
            ]),
        ),
        (
            "messy double equals",
            "true == doğru\nfalse == yanlış\nnull == boş\nvoid == hiçlik",
            r(&[
                &["true", "doğru"],
                &["false", "yanlış"],
                &["null", "boş"],
                &["void", "hiçlik"],
            ]),
        ),
        (
            "messy indented tab lines",
            "    run\tkoşmak\n    jump\tzıplamak\n  swim\tyüzmek\n      climb\ttırmanmak",
            r(&[
                &["run", "koşmak"],
                &["jump", "zıplamak"],
                &["swim", "yüzmek"],
                &["climb", "tırmanmak"],
            ]),
        ),
        (
            "messy comma trailing spaces",
            "red , kırmızı   \nblue,mavi \n green ,  yeşil\nyellow,sarı   ",
            r(&[
                &["red", "kırmızı"],
                &["blue", "mavi"],
                &["green", "yeşil"],
                &["yellow", "sarı"],
            ]),
        ),
        (
            "messy class notes colon inside value",
            "Ders: Matematik\nSaat: 13:15\nSınıf: 10-A\nNot: Kitap getir: sayfa 42",
            r(&[
                &["Ders", "Matematik"],
                &["Saat", "13:15"],
                &["Sınıf", "10-A"],
                &["Not", "Kitap getir: sayfa 42"],
            ]),
        ),
        (
            "messy pipe with blank and whitespace lines",
            "uzun | long\n\nkısa | short\n   \ngeniş | wide\ndar | narrow\n",
            r(&[
                &["uzun", "long"],
                &["kısa", "short"],
                &["geniş", "wide"],
                &["dar", "narrow"],
            ]),
        ),
        (
            "messy city country comma pairs",
            "Paris, France\nBerlin, Germany\nRome, Italy\nMadrid, Spain\nLisbon, Portugal",
            r(&[
                &["Paris", "France"],
                &["Berlin", "Germany"],
                &["Rome", "Italy"],
                &["Madrid", "Spain"],
                &["Lisbon", "Portugal"],
            ]),
        ),
        (
            "messy tab with thousands commas",
            "population of Turkey\t85,000,000\npopulation of Germany\t83,200,000\ndistance to the Moon\t384,400 km\nspeed of light\t299,792 km/s",
            r(&[
                &["population of Turkey", "85,000,000"],
                &["population of Germany", "83,200,000"],
                &["distance to the Moon", "384,400 km"],
                &["speed of light", "299,792 km/s"],
            ]),
        ),
        (
            "messy colon with decimal comma",
            "pi: 3,14159\ne: 2,71828\nphi: 1,61803\nkök 2: 1,41421",
            r(&[
                &["pi", "3,14159"],
                &["e", "2,71828"],
                &["phi", "1,61803"],
                &["kök 2", "1,41421"],
            ]),
        ),
        (
            "messy crlf spaced dash mixed",
            "to arrive - varmak\r\nto leave  -  ayrılmak\r\nto stay - kalmak \r\n  to return - dönmek\r\n\r\n",
            r(&[
                &["to arrive", "varmak"],
                &["to leave", "ayrılmak"],
                &["to stay", "kalmak"],
                &["to return", "dönmek"],
            ]),
        ),
        (
            "messy quizlet tab with trailing tab",
            "abolish\tortadan kaldırmak\t\nacclaim\tövgü\t\nadamant\tkararlı, inatçı\t\nadhere\tbağlı kalmak\t",
            r(&[
                &["abolish", "ortadan kaldırmak", ""],
                &["acclaim", "övgü", ""],
                &["adamant", "kararlı, inatçı", ""],
                &["adhere", "bağlı kalmak", ""],
            ]),
        ),
    ]
}

fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}
