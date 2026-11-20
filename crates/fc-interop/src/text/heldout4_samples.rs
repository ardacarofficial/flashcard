//! Fourth held-out separator-detection set (v1.0): written blind by a separate agent that never saw the detector or earlier sets, before any detector change was measured against it.

/// `(label, text, expected rows)`.
pub(super) fn samples() -> Vec<(&'static str, &'static str, Vec<Vec<String>>)> {
    vec![
        (
            "vocab tab tr-en",
            "kitap\tbook\nkalem\tpen",
            r(&[&["kitap", "book"], &["kalem", "pen"]]),
        ),
        (
            "quizlet tab en-tr fruits",
            "apple\telma\nbanana\tmuz\ncherry\tkiraz\ngrape\tüzüm",
            r(&[
                &["apple", "elma"],
                &["banana", "muz"],
                &["cherry", "kiraz"],
                &["grape", "üzüm"],
            ]),
        ),
        (
            "sheets copy 3 cols with header",
            "Word\tType\tMeaning\nrun\tverb\tkoşmak\nhappy\tadj\tmutlu\nquickly\tadv\thızlıca",
            r(&[
                &["Word", "Type", "Meaning"],
                &["run", "verb", "koşmak"],
                &["happy", "adj", "mutlu"],
                &["quickly", "adv", "hızlıca"],
            ]),
        ),
        (
            "csv comma header",
            "word,meaning\nhouse,ev\ncar,araba\ntree,ağaç",
            r(&[
                &["word", "meaning"],
                &["house", "ev"],
                &["car", "araba"],
                &["tree", "ağaç"],
            ]),
        ),
        (
            "excel tr semicolon",
            "kelime;anlam\nev;house\nokul;school\nkapı;door",
            r(&[
                &["kelime", "anlam"],
                &["ev", "house"],
                &["okul", "school"],
                &["kapı", "door"],
            ]),
        ),
        (
            "dash vocab with comma in value",
            "abandon - terk etmek\nabsorb - emmek\naccurate - doğru, kesin",
            r(&[
                &["abandon", "terk etmek"],
                &["absorb", "emmek"],
                &["accurate", "doğru, kesin"],
            ]),
        ),
        (
            "colon german",
            "der Hund: dog\ndie Katze: cat\nder Vogel: bird",
            r(&[
                &["der Hund", "dog"],
                &["die Katze", "cat"],
                &["der Vogel", "bird"],
            ]),
        ),
        (
            "equals tr",
            "su = water\nateş = fire\ntoprak = earth\nhava = air",
            r(&[
                &["su", "water"],
                &["ateş", "fire"],
                &["toprak", "earth"],
                &["hava", "air"],
            ]),
        ),
        (
            "ascii arrow",
            "run -> koşmak\nwalk -> yürümek\nswim -> yüzmek",
            r(&[
                &["run", "koşmak"],
                &["walk", "yürümek"],
                &["swim", "yüzmek"],
            ]),
        ),
        (
            "pipe german",
            "der Tisch | masa\ndie Lampe | lamba\ndas Fenster | pencere",
            r(&[
                &["der Tisch", "masa"],
                &["die Lampe", "lamba"],
                &["das Fenster", "pencere"],
            ]),
        ),
        (
            "single column en",
            "apple\nbanana\ncherry\ndate",
            r(&[&["apple"], &["banana"], &["cherry"], &["date"]]),
        ),
        (
            "single column tr",
            "merhaba\nteşekkürler\nlütfen\nhoşça kal",
            r(&[&["merhaba"], &["teşekkürler"], &["lütfen"], &["hoşça kal"]]),
        ),
        (
            "single column sentences with commas",
            "Hello, how are you?\nNice to meet you.\nSee you later, alligator.\nThank you very much.",
            r(&[
                &["Hello, how are you?"],
                &["Nice to meet you."],
                &["See you later, alligator."],
                &["Thank you very much."],
            ]),
        ),
        (
            "single column numbered",
            "1. abandon\n2. abide\n3. abolish\n4. absorb",
            r(&[&["1. abandon"], &["2. abide"], &["3. abolish"], &["4. absorb"]]),
        ),
        (
            "numbered dash",
            "1. apple - elma\n2. pear - armut\n3. plum - erik",
            r(&[
                &["1. apple", "elma"],
                &["2. pear", "armut"],
                &["3. plum", "erik"],
            ]),
        ),
        (
            "crlf tab",
            "ev\thouse\r\nokul\tschool\r\nkapı\tdoor\r\n",
            r(&[&["ev", "house"], &["okul", "school"], &["kapı", "door"]]),
        ),
        (
            "crlf csv capitals",
            "country,capital\r\nTurkey,Ankara\r\nFrance,Paris\r\nGermany,Berlin\r\n",
            r(&[
                &["country", "capital"],
                &["Turkey", "Ankara"],
                &["France", "Paris"],
                &["Germany", "Berlin"],
            ]),
        ),
        (
            "csv quoted commas",
            "\"hello, world\",merhaba dünya\n\"good morning\",günaydın\nthanks,teşekkürler",
            r(&[
                &["hello, world", "merhaba dünya"],
                &["good morning", "günaydın"],
                &["thanks", "teşekkürler"],
            ]),
        ),
        (
            "csv escaped quotes",
            "\"to say \"\"hi\"\"\",\"\"\"selam\"\" demek\"\nto wave,el sallamak\nto nod,başını sallamak",
            r(&[
                &["to say \"hi\"", "\"selam\" demek"],
                &["to wave", "el sallamak"],
                &["to nod", "başını sallamak"],
            ]),
        ),
        (
            "csv quoted multiline value",
            "term,definition\n\"osmosis\",\"movement of water\nthrough a membrane\"\n\"diffusion\",\"spreading of particles\"",
            r(&[
                &["term", "definition"],
                &["osmosis", "movement of water\nthrough a membrane"],
                &["diffusion", "spreading of particles"],
            ]),
        ),
        (
            "comma space",
            "big, büyük\nsmall, küçük\ntall, uzun\nshort, kısa",
            r(&[
                &["big", "büyük"],
                &["small", "küçük"],
                &["tall", "uzun"],
                &["short", "kısa"],
            ]),
        ),
        (
            "tab defs with commas",
            "ambiguous\tbelirsiz, iki anlamlı\nconcise\tkısa, öz\nvivid\tcanlı, parlak",
            r(&[
                &["ambiguous", "belirsiz, iki anlamlı"],
                &["concise", "kısa, öz"],
                &["vivid", "canlı, parlak"],
            ]),
        ),
        (
            "semicolon defs with commas",
            "to give up;vazgeçmek, bırakmak\nto look after;bakmak, ilgilenmek\nto put off;ertelemek",
            r(&[
                &["to give up", "vazgeçmek, bırakmak"],
                &["to look after", "bakmak, ilgilenmek"],
                &["to put off", "ertelemek"],
            ]),
        ),
        (
            "colon formulas with equals",
            "Pythagoras: a² + b² = c²\nNewton's second law: F = m·a\nOhm's law: V = I·R\nArea of a circle: πr²",
            r(&[
                &["Pythagoras", "a² + b² = c²"],
                &["Newton's second law", "F = m·a"],
                &["Ohm's law", "V = I·R"],
                &["Area of a circle", "πr²"],
            ]),
        ),
        (
            "tab formulas",
            "Kinetic energy\tKE = ½mv²\nPotential energy\tPE = mgh\nDensity\tρ = m/V",
            r(&[
                &["Kinetic energy", "KE = ½mv²"],
                &["Potential energy", "PE = mgh"],
                &["Density", "ρ = m/V"],
            ]),
        ),
        (
            "tab history dates",
            "1071\tMalazgirt Savaşı\n1453\tİstanbul'un fethi\n1923\tCumhuriyetin ilanı",
            r(&[
                &["1071", "Malazgirt Savaşı"],
                &["1453", "İstanbul'un fethi"],
                &["1923", "Cumhuriyetin ilanı"],
            ]),
        ),
        (
            "dash dates with dots",
            "Cumhuriyetin ilanı - 29.10.1923\nTBMM'nin açılışı - 23.04.1920\nLozan Antlaşması - 24.07.1923",
            r(&[
                &["Cumhuriyetin ilanı", "29.10.1923"],
                &["TBMM'nin açılışı", "23.04.1920"],
                &["Lozan Antlaşması", "24.07.1923"],
            ]),
        ),
        (
            "dash year ranges",
            "World War I - 1914-1918\nWorld War II - 1939-1945\nKorean War - 1950-1953",
            r(&[
                &["World War I", "1914-1918"],
                &["World War II", "1939-1945"],
                &["Korean War", "1950-1953"],
            ]),
        ),
        (
            "tab times with colon",
            "Breakfast\t08:00\nLunch\t12:30\nDinner\t19:00",
            r(&[
                &["Breakfast", "08:00"],
                &["Lunch", "12:30"],
                &["Dinner", "19:00"],
            ]),
        ),
        (
            "colon battle years",
            "Malazgirt: 1071\nKosova: 1389\nÇaldıran: 1514\nMohaç: 1526",
            r(&[
                &["Malazgirt", "1071"],
                &["Kosova", "1389"],
                &["Çaldıran", "1514"],
                &["Mohaç", "1526"],
            ]),
        ),
        (
            "messy blank lines dash",
            "\n\nkedi - cat  \n\nköpek - dog\n\n\nkuş - bird\n",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["kuş", "bird"]]),
        ),
        (
            "padded tab values",
            "  elma \t apple  \n armut\tpear \nkiraz \tcherry",
            r(&[&["elma", "apple"], &["armut", "pear"], &["kiraz", "cherry"]]),
        ),
        (
            "blank-line multiline tab",
            "Mitochondria\tPowerhouse of the cell.\nProduces ATP.\n\nRibosome\tMakes proteins.\n\nNucleus\tContains DNA.\nControls the cell.",
            r(&[
                &["Mitochondria", "Powerhouse of the cell.\nProduces ATP."],
                &["Ribosome", "Makes proteins."],
                &["Nucleus", "Contains DNA.\nControls the cell."],
            ]),
        ),
        (
            "blank-line multiline colon",
            "Photosynthesis: the process by which plants\nmake food using sunlight\n\nRespiration: the process by which cells\nrelease energy from glucose\n\nTranspiration: loss of water from leaves",
            r(&[
                &["Photosynthesis", "the process by which plants\nmake food using sunlight"],
                &["Respiration", "the process by which cells\nrelease energy from glucose"],
                &["Transpiration", "loss of water from leaves"],
            ]),
        ),
        (
            "blank-line two-line term def",
            "abundant\nbol, çok miktarda\n\nscarce\nkıt, az bulunan\n\nfertile\nverimli",
            r(&[
                &["abundant", "bol, çok miktarda"],
                &["scarce", "kıt, az bulunan"],
                &["fertile", "verimli"],
            ]),
        ),
        (
            "blank-line three-line term def example",
            "run\nkoşmak\nI run every morning.\n\neat\nyemek\nWe eat at seven.\n\nsleep\nuyumak\nThe baby is sleeping.",
            r(&[
                &["run", "koşmak", "I run every morning."],
                &["eat", "yemek", "We eat at seven."],
                &["sleep", "uyumak", "The baby is sleeping."],
            ]),
        ),
        (
            "blank-line multiline dash",
            "Ottoman Empire - founded in 1299\nlasted until 1922\n\nSeljuk Empire - founded in 1037\n\nByzantine Empire - capital Constantinople\nfell in 1453",
            r(&[
                &["Ottoman Empire", "founded in 1299\nlasted until 1922"],
                &["Seljuk Empire", "founded in 1037"],
                &["Byzantine Empire", "capital Constantinople\nfell in 1453"],
            ]),
        ),
        (
            "custom record semicolon equals",
            "elma=apple;armut=pear;muz=banana;kiraz=cherry",
            r(&[
                &["elma", "apple"],
                &["armut", "pear"],
                &["muz", "banana"],
                &["kiraz", "cherry"],
            ]),
        ),
        (
            "custom record semicolon dash",
            "dog - köpek; cat - kedi; bird - kuş; fish - balık",
            r(&[
                &["dog", "köpek"],
                &["cat", "kedi"],
                &["bird", "kuş"],
                &["fish", "balık"],
            ]),
        ),
        (
            "custom record semicolon comma days",
            "Montag,Pazartesi;Dienstag,Salı;Mittwoch,Çarşamba;Donnerstag,Perşembe",
            r(&[
                &["Montag", "Pazartesi"],
                &["Dienstag", "Salı"],
                &["Mittwoch", "Çarşamba"],
                &["Donnerstag", "Perşembe"],
            ]),
        ),
        (
            "custom record comma dash",
            "dog - köpek, cat - kedi, bird - kuş",
            r(&[&["dog", "köpek"], &["cat", "kedi"], &["bird", "kuş"]]),
        ),
        (
            "unicode arrow",
            "go → gitmek\ncome → gelmek\nsee → görmek",
            r(&[&["go", "gitmek"], &["come", "gelmek"], &["see", "görmek"]]),
        ),
        (
            "fat arrow",
            "big => büyük\nsmall => küçük\nold => yaşlı",
            r(&[&["big", "büyük"], &["small", "küçük"], &["old", "yaşlı"]]),
        ),
        (
            "en dash word autocorrect",
            "kitap – book\nmasa – table\nsandalye – chair",
            r(&[&["kitap", "book"], &["masa", "table"], &["sandalye", "chair"]]),
        ),
        (
            "hyphenated words dash",
            "well-known - tanınmış\nself-confident - kendine güvenen\nup-to-date - güncel",
            r(&[
                &["well-known", "tanınmış"],
                &["self-confident", "kendine güvenen"],
                &["up-to-date", "güncel"],
            ]),
        ),
        (
            "sentence pairs dash",
            "I am hungry. - Acıktım.\nWhere is the station? - İstasyon nerede?\nHow much is it? - Ne kadar?",
            r(&[
                &["I am hungry.", "Acıktım."],
                &["Where is the station?", "İstasyon nerede?"],
                &["How much is it?", "Ne kadar?"],
            ]),
        ),
        (
            "spanish equals",
            "hola = merhaba\nadiós = hoşça kal\ngracias = teşekkürler",
            r(&[
                &["hola", "merhaba"],
                &["adiós", "hoşça kal"],
                &["gracias", "teşekkürler"],
            ]),
        ),
        (
            "french colon spaced",
            "le chat : kedi\nle chien : köpek\nla souris : fare",
            r(&[
                &["le chat", "kedi"],
                &["le chien", "köpek"],
                &["la souris", "fare"],
            ]),
        ),
        (
            "tab empty value",
            "kedi\tcat\nköpek\t\nkuş\tbird",
            r(&[&["kedi", "cat"], &["köpek", ""], &["kuş", "bird"]]),
        ),
        (
            "csv 3 cols with sentences",
            "word,translation,example\nrun,koşmak,I run every day\neat,yemek,We eat at noon",
            r(&[
                &["word", "translation", "example"],
                &["run", "koşmak", "I run every day"],
                &["eat", "yemek", "We eat at noon"],
            ]),
        ),
        (
            "tab 4 cols irregular verbs",
            "Verb\tPast\tPast Participle\tTürkçe\ngo\twent\tgone\tgitmek\nsee\tsaw\tseen\tgörmek\ntake\ttook\ttaken\talmak",
            r(&[
                &["Verb", "Past", "Past Participle", "Türkçe"],
                &["go", "went", "gone", "gitmek"],
                &["see", "saw", "seen", "görmek"],
                &["take", "took", "taken", "almak"],
            ]),
        ),
        (
            "csv irregular verbs no header",
            "go,went,gone\nsee,saw,seen\neat,ate,eaten\nwrite,wrote,written",
            r(&[
                &["go", "went", "gone"],
                &["see", "saw", "seen"],
                &["eat", "ate", "eaten"],
                &["write", "wrote", "written"],
            ]),
        ),
        (
            "semicolon 3 cols german plurals",
            "der Apfel;die Äpfel;elma\ndas Buch;die Bücher;kitap\ndie Frau;die Frauen;kadın",
            r(&[
                &["der Apfel", "die Äpfel", "elma"],
                &["das Buch", "die Bücher", "kitap"],
                &["die Frau", "die Frauen", "kadın"],
            ]),
        ),
        (
            "pipe 3 cols",
            "word | meaning | example\nbrave | cesur | He was very brave.\ncalm | sakin | Stay calm.",
            r(&[
                &["word", "meaning", "example"],
                &["brave", "cesur", "He was very brave."],
                &["calm", "sakin", "Stay calm."],
            ]),
        ),
        (
            "colon q&a capitals tr",
            "Türkiye'nin başkenti: Ankara\nFransa'nın başkenti: Paris\nJaponya'nın başkenti: Tokyo",
            r(&[
                &["Türkiye'nin başkenti", "Ankara"],
                &["Fransa'nın başkenti", "Paris"],
                &["Japonya'nın başkenti", "Tokyo"],
            ]),
        ),
        (
            "tab trivia questions",
            "What is the capital of Italy?\tRome\nWho wrote Hamlet?\tShakespeare\nHow many continents are there?\tSeven",
            r(&[
                &["What is the capital of Italy?", "Rome"],
                &["Who wrote Hamlet?", "Shakespeare"],
                &["How many continents are there?", "Seven"],
            ]),
        ),
        (
            "dash chemistry questions",
            "What is H2O? - Water\nWhat is NaCl? - Salt\nWhat is CO2? - Carbon dioxide",
            r(&[
                &["What is H2O?", "Water"],
                &["What is NaCl?", "Salt"],
                &["What is CO2?", "Carbon dioxide"],
            ]),
        ),
        (
            "colon element symbols",
            "Fe: Demir\nCu: Bakır\nAu: Altın\nAg: Gümüş\nNa: Sodyum",
            r(&[
                &["Fe", "Demir"],
                &["Cu", "Bakır"],
                &["Au", "Altın"],
                &["Ag", "Gümüş"],
                &["Na", "Sodyum"],
            ]),
        ),
        (
            "tab elements crlf",
            "H\tHidrojen\r\nHe\tHelyum\r\nLi\tLityum",
            r(&[&["H", "Hidrojen"], &["He", "Helyum"], &["Li", "Lityum"]]),
        ),
        (
            "equals no spaces",
            "cat=kedi\ndog=köpek\nmouse=fare",
            r(&[&["cat", "kedi"], &["dog", "köpek"], &["mouse", "fare"]]),
        ),
        (
            "colon no space colors",
            "red:kırmızı\nblue:mavi\ngreen:yeşil\nyellow:sarı",
            r(&[
                &["red", "kırmızı"],
                &["blue", "mavi"],
                &["green", "yeşil"],
                &["yellow", "sarı"],
            ]),
        ),
        (
            "dash with parenthesized forms",
            "to run (ran, run) - koşmak\nto see (saw, seen) - görmek\nto go (went, gone) - gitmek",
            r(&[
                &["to run (ran, run)", "koşmak"],
                &["to see (saw, seen)", "görmek"],
                &["to go (went, gone)", "gitmek"],
            ]),
        ),
        (
            "tab with parentheses and commas",
            "bark (v.)\thavlamak, bağırmak\nbark (n.)\tağaç kabuğu\nbat (n.)\tyarasa, sopa",
            r(&[
                &["bark (v.)", "havlamak, bağırmak"],
                &["bark (n.)", "ağaç kabuğu"],
                &["bat (n.)", "yarasa, sopa"],
            ]),
        ),
        (
            "single column phrasal verbs",
            "give up\nlook after\nturn down\nput off",
            r(&[&["give up"], &["look after"], &["turn down"], &["put off"]]),
        ),
        (
            "single column hyphenated words",
            "well-being\nmother-in-law\nup-to-date\nself-esteem",
            r(&[
                &["well-being"],
                &["mother-in-law"],
                &["up-to-date"],
                &["self-esteem"],
            ]),
        ),
        (
            "single column german nouns",
            "der Tisch\ndie Lampe\ndas Fenster\ndie Tür",
            r(&[&["der Tisch"], &["die Lampe"], &["das Fenster"], &["die Tür"]]),
        ),
        (
            "single column dates",
            "29.10.1923\n23.04.1920\n19.05.1919",
            r(&[&["29.10.1923"], &["23.04.1920"], &["19.05.1919"]]),
        ),
        (
            "single column apostrophes tr",
            "İstanbul'da\nAnkara'ya\nİzmir'den",
            r(&[&["İstanbul'da"], &["Ankara'ya"], &["İzmir'den"]]),
        ),
        (
            "tab medical long definitions",
            "Tachycardia\tAbnormally fast heart rate, usually over 100 beats per minute\nBradycardia\tAbnormally slow heart rate, under 60 beats per minute\nHypertension\tHigh blood pressure",
            r(&[
                &["Tachycardia", "Abnormally fast heart rate, usually over 100 beats per minute"],
                &["Bradycardia", "Abnormally slow heart rate, under 60 beats per minute"],
                &["Hypertension", "High blood pressure"],
            ]),
        ),
        (
            "csv no space tr fruits",
            "elma,apple\narmut,pear\nüzüm,grape\nincir,fig",
            r(&[
                &["elma", "apple"],
                &["armut", "pear"],
                &["üzüm", "grape"],
                &["incir", "fig"],
            ]),
        ),
        (
            "csv multiword values",
            "ice cream,dondurma\nfrench fries,patates kızartması\nhot dog,sosisli",
            r(&[
                &["ice cream", "dondurma"],
                &["french fries", "patates kızartması"],
                &["hot dog", "sosisli"],
            ]),
        ),
        (
            "semicolon spaced",
            "big ; büyük\nsmall ; küçük\nfast ; hızlı",
            r(&[&["big", "büyük"], &["small", "küçük"], &["fast", "hızlı"]]),
        ),
        (
            "csv all fields quoted",
            "\"kitap\",\"book\"\n\"kalem\",\"pen\"\n\"defter\",\"notebook\"",
            r(&[&["kitap", "book"], &["kalem", "pen"], &["defter", "notebook"]]),
        ),
        (
            "semicolon csv quoted semicolon inside",
            "kelime;örnek\nkoşmak;\"Her sabah koşarım; sen?\"\nyemek;Akşam yemek yedik",
            r(&[
                &["kelime", "örnek"],
                &["koşmak", "Her sabah koşarım; sen?"],
                &["yemek", "Akşam yemek yedik"],
            ]),
        ),
        (
            "tab sentence translations",
            "I like coffee.\tKahveyi severim.\nShe is reading a book.\tO bir kitap okuyor.\nWe went to the park, then home.\tParka, sonra eve gittik.",
            r(&[
                &["I like coffee.", "Kahveyi severim."],
                &["She is reading a book.", "O bir kitap okuyor."],
                &["We went to the park, then home.", "Parka, sonra eve gittik."],
            ]),
        ),
        (
            "crlf dash with blank line",
            "kedi - cat\r\nköpek - dog\r\n\r\nkuş - bird\r\n",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["kuş", "bird"]]),
        ),
        (
            "anki tab with html",
            "<b>Haus</b>\tev\n<i>Baum</i>\tağaç\nAuto\taraba",
            r(&[&["<b>Haus</b>", "ev"], &["<i>Baum</i>", "ağaç"], &["Auto", "araba"]]),
        ),
        (
            "anki 3 fields with tags",
            "Hund\tdog\tanimals\nKatze\tcat\tanimals\nApfel\tapple\tfood",
            r(&[
                &["Hund", "dog", "animals"],
                &["Katze", "cat", "animals"],
                &["Apfel", "apple", "food"],
            ]),
        ),
        (
            "equals multiplication",
            "7 x 8 = 56\n6 x 9 = 54\n12 x 12 = 144",
            r(&[&["7 x 8", "56"], &["6 x 9", "54"], &["12 x 12", "144"]]),
        ),
        (
            "equals subtraction",
            "15 - 7 = 8\n20 - 4 = 16\n9 - 3 = 6",
            r(&[&["15 - 7", "8"], &["20 - 4", "16"], &["9 - 3", "6"]]),
        ),
        (
            "equals unit conversions",
            "1 km = 1000 m\n1 kg = 1000 g\n1 hour = 60 minutes\n1 day = 24 hours",
            r(&[
                &["1 km", "1000 m"],
                &["1 kg", "1000 g"],
                &["1 hour", "60 minutes"],
                &["1 day", "24 hours"],
            ]),
        ),
        (
            "colon spanish verbs",
            "hablar: konuşmak\ncomer: yemek\nvivir: yaşamak\nescribir: yazmak",
            r(&[
                &["hablar", "konuşmak"],
                &["comer", "yemek"],
                &["vivir", "yaşamak"],
                &["escribir", "yazmak"],
            ]),
        ),
        (
            "tab french articles",
            "la maison\tev\nle livre\tkitap\nl'école\tokul",
            r(&[&["la maison", "ev"], &["le livre", "kitap"], &["l'école", "okul"]]),
        ),
        (
            "csv contractions",
            "don't,yapma\ncan't,yapamam\nwon't,yapmayacağım",
            r(&[
                &["don't", "yapma"],
                &["can't", "yapamam"],
                &["won't", "yapmayacağım"],
            ]),
        ),
        (
            "pipe no spaces",
            "cat|kedi\ndog|köpek\nbird|kuş",
            r(&[&["cat", "kedi"], &["dog", "köpek"], &["bird", "kuş"]]),
        ),
        (
            "tab ragged rows",
            "word\tmeaning\tnote\nrun\tkoşmak\nswim\tyüzmek\tirregular",
            r(&[
                &["word", "meaning", "note"],
                &["run", "koşmak"],
                &["swim", "yüzmek", "irregular"],
            ]),
        ),
        (
            "tab values with comma place",
            "Atatürk'ün doğumu\t1881, Selanik\nMimar Sinan'ın doğumu\t1489, Ağırnas\nYunus Emre'nin doğumu\t1238, Sivrihisar",
            r(&[
                &["Atatürk'ün doğumu", "1881, Selanik"],
                &["Mimar Sinan'ın doğumu", "1489, Ağırnas"],
                &["Yunus Emre'nin doğumu", "1238, Sivrihisar"],
            ]),
        ),
        (
            "colon abbreviations with dots",
            "e.g.: for example\ni.e.: that is\netc.: and so on",
            r(&[
                &["e.g.", "for example"],
                &["i.e.", "that is"],
                &["etc.", "and so on"],
            ]),
        ),
        (
            "dash idioms",
            "break the ice - buzları eritmek\nhit the books - ders çalışmak\npiece of cake - çok kolay",
            r(&[
                &["break the ice", "buzları eritmek"],
                &["hit the books", "ders çalışmak"],
                &["piece of cake", "çok kolay"],
            ]),
        ),
        (
            "equals german",
            "das Haus = ev\ndie Schule = okul\nder Zug = tren",
            r(&[&["das Haus", "ev"], &["die Schule", "okul"], &["der Zug", "tren"]]),
        ),
        (
            "colon biology with commas",
            "Photosynthesis: plants use light, water, and CO2 to make glucose\nRespiration: cells break down glucose, releasing energy\nTranspiration: water evaporates from leaves",
            r(&[
                &["Photosynthesis", "plants use light, water, and CO2 to make glucose"],
                &["Respiration", "cells break down glucose, releasing energy"],
                &["Transpiration", "water evaporates from leaves"],
            ]),
        ),
        (
            "semicolon decimal comma tr excel",
            "pi;3,14\ne;2,718\naltın oran;1,618",
            r(&[&["pi", "3,14"], &["e", "2,718"], &["altın oran", "1,618"]]),
        ),
        (
            "tab decimal numbers",
            "Pi\t3.14159\nEuler\t2.71828\nGolden ratio\t1.61803",
            r(&[
                &["Pi", "3.14159"],
                &["Euler", "2.71828"],
                &["Golden ratio", "1.61803"],
            ]),
        ),
        (
            "arrow no spaces comparatives",
            "big->bigger\nsmall->smaller\ngood->better",
            r(&[&["big", "bigger"], &["small", "smaller"], &["good", "better"]]),
        ),
        (
            "csv with blank lines between",
            "kedi,cat\n\nköpek,dog\n\nkuş,bird\n\n",
            r(&[&["kedi", "cat"], &["köpek", "dog"], &["kuş", "bird"]]),
        ),
        (
            "quizlet tab longer list",
            "to borrow\tödünç almak\nto lend\tödünç vermek\nto rent\tkiralamak\nto owe\tborçlu olmak\nto earn\tkazanmak\nto spend\tharcamak",
            r(&[
                &["to borrow", "ödünç almak"],
                &["to lend", "ödünç vermek"],
                &["to rent", "kiralamak"],
                &["to owe", "borçlu olmak"],
                &["to earn", "kazanmak"],
                &["to spend", "harcamak"],
            ]),
        ),
        (
            "dash es-tr numbers",
            "uno - bir\ndos - iki\ntres - üç\ncuatro - dört\ncinco - beş",
            r(&[
                &["uno", "bir"],
                &["dos", "iki"],
                &["tres", "üç"],
                &["cuatro", "dört"],
                &["cinco", "beş"],
            ]),
        ),
        (
            "colon fr phrases with commas",
            "Bonjour, ça va ?: Merhaba, nasılsın?\nMerci beaucoup: Çok teşekkürler\nÀ bientôt: Görüşürüz",
            r(&[
                &["Bonjour, ça va ?", "Merhaba, nasılsın?"],
                &["Merci beaucoup", "Çok teşekkürler"],
                &["À bientôt", "Görüşürüz"],
            ]),
        ),
        (
            "tab law terms tr",
            "Anayasa\tDevletin temel hukuk düzenini belirleyen yasa\nKanun\tTBMM tarafından çıkarılan yazılı hukuk kuralı\nYönetmelik\tKanunların uygulanmasını sağlayan düzenleme",
            r(&[
                &["Anayasa", "Devletin temel hukuk düzenini belirleyen yasa"],
                &["Kanun", "TBMM tarafından çıkarılan yazılı hukuk kuralı"],
                &["Yönetmelik", "Kanunların uygulanmasını sağlayan düzenleme"],
            ]),
        ),
        (
            "csv quoted header and mixed quoting",
            "\"English\",\"Türkçe\"\nfriend,arkadaş\n\"best friend\",\"en iyi arkadaş\"\nneighbour,komşu",
            r(&[
                &["English", "Türkçe"],
                &["friend", "arkadaş"],
                &["best friend", "en iyi arkadaş"],
                &["neighbour", "komşu"],
            ]),
        ),
        (
            "dash numbered with dots in value",
            "1. Dr. - doktor\n2. Mr. - bay\n3. Mrs. - bayan",
            r(&[&["1. Dr.", "doktor"], &["2. Mr.", "bay"], &["3. Mrs.", "bayan"]]),
        ),
        (
            "blank-line multiline equals",
            "Newton's first law = an object stays at rest\nor in uniform motion unless acted on by a force\n\nNewton's third law = every action has an equal\nand opposite reaction",
            r(&[
                &["Newton's first law", "an object stays at rest\nor in uniform motion unless acted on by a force"],
                &["Newton's third law", "every action has an equal\nand opposite reaction"],
            ]),
        ),
        (
            "single column long list",
            "accommodate\nachieve\nacquire\nadequate\nadvocate\naffect\naggregate\nallocate",
            r(&[
                &["accommodate"],
                &["achieve"],
                &["acquire"],
                &["adequate"],
                &["advocate"],
                &["affect"],
                &["aggregate"],
                &["allocate"],
            ]),
        ),
        (
            "tab urls as values",
            "Wikipedia\thttps://www.wikipedia.org\nTDK Sözlük\thttps://sozluk.gov.tr\nCambridge\thttps://dictionary.cambridge.org",
            r(&[
                &["Wikipedia", "https://www.wikipedia.org"],
                &["TDK Sözlük", "https://sozluk.gov.tr"],
                &["Cambridge", "https://dictionary.cambridge.org"],
            ]),
        ),
    ]
}

fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}
