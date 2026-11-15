//! Name generators for character creation, one per heritage.
//!
//! Retail never generated names, but its creation screens told the player how each heritage and
//! sex names itself: the records each sex names before Throne of Destiny, and the
//! `ID_CharGen_<Heritage><Sex>Names` rows of string table `0x23000002` at the end of retail. Each
//! generator follows its heritage's text, with the forms it lists; the name parts are the texts'
//! examples and the heritage's NPC names in the world's content.
//!
//! A name drawn here is one a typed name would also be: [`is_valid`] is the creation model's name
//! check, and the name must come through its formatting unchanged.

use pyreal_primitives::EraId;

/// The sex keys of the creation tables.
pub const SEX_MALE: u32 = 1;
/// See [`SEX_MALE`].
pub const SEX_FEMALE: u32 = 2;

/// The names' own random stream: SplitMix64. Kept apart from [`crate::CharGenRng`] so a name
/// never moves the draws the client makes for everything else.
#[derive(Debug, Clone)]
pub struct NameRng(u64);

impl NameRng {
    /// A stream from `seed`; the same seed draws the same names.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`; 0 when `n` is 0.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let at = ((u128::from(self.next()) * n as u128) >> 64) as usize;
        at
    }

    /// True `percent` times in a hundred.
    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// Whether the name button is offered for `heritage` on a world of `era`: the original three
/// before Throne of Destiny, every heritage at the end of retail.
#[must_use]
pub fn offered(era: EraId, heritage: u32) -> bool {
    match era {
        EraId::Launch | EraId::Infiltration => (1..=3).contains(&heritage),
        EraId::Eor => (1..=13).contains(&heritage),
    }
}

/// Whether the creation model takes `name` as it is: the length it allows, and no change from its
/// formatting (letters, single spaces, `'` and `-` where it keeps them, the case it gives).
#[must_use]
pub fn is_valid(name: &str) -> bool {
    name.is_ascii()
        && pyreal_rules::chargen::name_length_ok(name)
        && pyreal_rules::names::format_name(name.as_bytes()) == name
}

/// Whether every word of `name` keeps the English rules of the later files' name-filter table
/// (`0x0E000020`): at most two of a letter and three vowels in a row, a vowel in the first four
/// letters and in every five (`th`, `ch` and `ph` one letter). `y` is a vowel, as the creation
/// text's own examples (Wothgryn) need.
#[must_use]
pub fn fits_name_filter(name: &str) -> bool {
    let vowel = |c: u8| matches!(c, b'a' | b'e' | b'i' | b'o' | b'u' | b'y');
    name.split(' ').all(|word| {
        let word: Vec<u8> = word
            .bytes()
            .filter(u8::is_ascii_alphabetic)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let mut letters: Vec<u8> = Vec::with_capacity(word.len());
        let mut i = 0;
        while i < word.len() {
            if matches!(word.get(i..i + 2), Some(b"th" | b"ch" | b"ph")) {
                letters.push(b'h');
                i += 2;
            } else {
                letters.push(word[i]);
                i += 1;
            }
        }
        !word.windows(3).any(|w| w[0] == w[1] && w[1] == w[2])
            && !letters.windows(4).any(|w| w.iter().all(|c| vowel(*c)))
            && (letters.len() < 4 || letters[..4].iter().any(|c| vowel(*c)))
            && !letters.windows(5).any(|w| !w.iter().any(|c| vowel(*c)))
    })
}

/// A name for `heritage` and sex `sex` (any other sex draws one), or `None` for a heritage
/// without a generator.
pub fn generate(heritage: u32, sex: u32, rng: &mut NameRng) -> Option<String> {
    generate_where(heritage, sex, rng, |_| true)
}

/// [`generate`], drawing again while `accept` refuses the name (a world's taboo table).
pub fn generate_where(
    heritage: u32,
    sex: u32,
    rng: &mut NameRng,
    accept: impl Fn(&str) -> bool,
) -> Option<String> {
    if !(1..=13).contains(&heritage) {
        return None;
    }
    let male = match sex {
        SEX_MALE => true,
        SEX_FEMALE => false,
        _ => rng.chance(50),
    };
    for _ in 0..64 {
        let name = draw(heritage, male, rng);
        if is_valid(&name) && fits_name_filter(&name) && accept(&name) {
            return Some(name);
        }
    }
    None
}

fn draw(heritage: u32, male: bool, rng: &mut NameRng) -> String {
    match heritage {
        1 => aluvian(male, rng),
        2 => gharundim(male, rng),
        3 => sho(male, rng),
        4 => viamontian(male, rng),
        5 | 10 => shadow(male, rng),
        6 => gear_knight(male, rng),
        7 => tumerok(male, rng),
        8 => lugian(male, rng),
        9 => empyrean(male, rng),
        11 => undead(male, rng),
        _ => olthoi(heritage == crate::HERITAGE_OLTHOI_ACID, rng),
    }
}

/// A head and a tail, never the same syllable twice (Wynwyn).
fn two(rng: &mut NameRng, first: &[&str], last: &[&str]) -> String {
    loop {
        let (a, b) = (rng.pick(first), rng.pick(last));
        if !a.eq_ignore_ascii_case(b) {
            return format!("{a}{b}");
        }
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
        .unwrap_or_default()
}

// Aluvian. Many have a first name only; others a first name, "the" and a word; a few a surname
// with "-sun" (son) or "-mad" (daughter).

const ALUVIAN_MALE_FIRST: &[&str] = &[
    "Hund", "Cuth", "Woth", "Os", "Bur", "Forst", "Scin", "Ar", "Rim", "Ead", "Ald", "Ber", "God",
    "Leof", "Sig", "Theod", "Wulf", "Dun", "Ethel", "Hal", "Grim", "Thur", "Wig", "Ceol", "Bryn",
    "Ulf", "Ealh",
];
const ALUVIAN_MALE_LAST: &[&str] = &[
    "reng", "stan", "gryn", "geld", "wylf", "lan", "grim", "brim", "gar", "ric", "mund", "wine",
    "helm", "ward", "bald", "red", "wald", "frith", "mer", "bert", "gram", "thorn",
];
const ALUVIAN_FEMALE_FIRST: &[&str] = &[
    "Lis", "Ule", "Soth", "Dom", "Ar", "Eth", "Dirn", "Has", "Cen", "Aeth", "Ead", "Wil", "Os",
    "Mild", "Gode", "Bran", "Ald", "Sel", "Hild", "Aid", "Wyn", "Elf", "Cyn", "Leof",
];
const ALUVIAN_FEMALE_LAST: &[&str] = &[
    "sith", "winn", "lal", "lan", "ryn", "ew", "mod", "broc", "har", "wyn", "frith", "gyth",
    "thryth", "burh", "hild", "flaed", "red", "ene", "da", "wen", "swith",
];
/// The words after "the": the retail examples, then those of the Aluvian NPCs.
const ALUVIAN_EPITHETS: &[&str] = &[
    "Clever", "Wary", "Ox", "True", "Cat", "Fair", "Deft", "Wolf", "Dire", "Pale", "Bold", "Fleet",
    "Subtle", "Curious", "Eldest", "Thrifty", "Discreet", "Grim", "Swift", "Wise", "Stout", "Tall",
    "Red", "Black", "Bear", "Hawk", "Fox", "Steady", "Quiet", "Younger", "Strong", "Lucky",
];
/// Surname stems: the retail examples (Emersun, Blodmad, ...) and NPCs (Algar Oreksun, Flinrala
/// Ryndmad, Claire Artmad, Turvald Snorborgson).
const ALUVIAN_SURNAME_STEMS: &[&str] = &[
    "Emer", "Ryne", "Eth", "Blod", "Grith", "Est", "Oreks", "Rynd", "Art", "Snorborg", "Hald",
    "Wulf", "Thur", "Ead", "Gar", "Brand", "Os", "Hund", "Leof", "Dun", "Ceol", "Grim",
];

fn aluvian_first(male: bool, rng: &mut NameRng) -> String {
    if male {
        two(rng, ALUVIAN_MALE_FIRST, ALUVIAN_MALE_LAST)
    } else {
        two(rng, ALUVIAN_FEMALE_FIRST, ALUVIAN_FEMALE_LAST)
    }
}

fn aluvian(male: bool, rng: &mut NameRng) -> String {
    let first = aluvian_first(male, rng);
    match rng.below(100) {
        0..=49 => first,
        50..=84 => format!("{first} the {}", rng.pick(ALUVIAN_EPITHETS)),
        _ => {
            let suffix = if male { "sun" } else { "mad" };
            format!("{first} {}{suffix}", rng.pick(ALUVIAN_SURNAME_STEMS))
        }
    }
}

// Gharu'ndim. 'ism: a first name alone. Hisba: a first name, "al-" and a surname. Nasab: a first
// name, "ibn" (son of) or "bint" (daughter of) and a name.

const GHARU_MALE_FIRST: &[&str] = &[
    "Sun", "Tiq", "Ab", "Ma'w", "Uz", "Fadl", "Abd", "Tash", "Dhar", "Ar", "Bhra", "Hosh", "Jaw",
    "Jib", "Kal", "Kay", "Mud", "Rif", "Qur", "Zah", "Has", "Nas", "Sal", "Khal", "Qa'l", "Ish",
];
const GHARU_MALE_LAST: &[&str] = &[
    "nuj", "abar", "ham", "har", "ik", "mar", "iris", "tal", "ilal", "ud", "varn", "aq", "ril",
    "al", "mor", "ris", "in", "akh", "id", "im", "ad", "sim", "dun",
];
const GHARU_FEMALE_FIRST: &[&str] = &[
    "Jaid", "Di'z", "Haj", "Qays", "Nuh", "Maj", "Ash", "Shul", "Khay", "Adr", "Af", "Al", "Day",
    "Had", "Hav", "Mash", "Pay", "Rah", "Tayl", "Ual", "Za", "Lei", "Ya", "Nur", "Ti'",
];
const GHARU_FEMALE_LAST: &[&str] = &[
    "mala", "arafa", "liya", "bah", "naba", "qiya", "rifa", "ima", "la'ida", "aha", "ra", "isha",
    "la", "iya", "ala", "ira", "ina", "za", "ama", "ida", "ana", "alla",
];
/// After "al-": the retail examples and the NPCs' family names.
const GHARU_FAMILIES: &[&str] = &[
    "Qutuz",
    "Zaraf",
    "Tuwah",
    "Halras",
    "Thudadh",
    "Ruz",
    "Juhir",
    "Fariq",
    "Evv",
    "Nishadina",
    "Kesh",
    "Haddash",
    "Taal",
    "Luq",
    "Rakh",
    "Bhavarn",
    "Din",
    "Jaluzi",
    "Qorra",
];
/// After "ibn" or "bint": the retail examples and the NPCs' fathers' names.
const GHARU_FATHERS: &[&str] = &[
    "Mauki", "Yufa", "Qa'yam", "Yazman", "Bashr", "Tiqiq", "Qasid", "Balthar", "Salizim", "Jalaq",
    "Janath", "Rashid", "Dariz", "Dumandi", "Ashud", "Hadram", "Makar", "Djimin", "Fezesh",
    "Abbas", "Aldan", "Anan", "Mylos", "Qumal", "Jumadd", "Tamur", "Tinun", "Tulani", "Muyzal",
    "Laurma", "Safsan", "Zalanis", "Sakim", "Kazm", "Zayi", "Jaffa", "Kassim", "Tamarek",
];

fn gharundim_first(male: bool, rng: &mut NameRng) -> String {
    if male {
        two(rng, GHARU_MALE_FIRST, GHARU_MALE_LAST)
    } else {
        two(rng, GHARU_FEMALE_FIRST, GHARU_FEMALE_LAST)
    }
}

fn gharundim(male: bool, rng: &mut NameRng) -> String {
    let first = gharundim_first(male, rng);
    match rng.below(3) {
        0 => first,
        1 => format!("{first} al-{}", rng.pick(GHARU_FAMILIES)),
        _ => {
            let of = if male { "ibn" } else { "bint" };
            format!("{first} {of} {}", rng.pick(GHARU_FATHERS))
        }
    }
}

// Sho. The surname first and the "first name" last, which may be two syllables joined by a hyphen
// (Fenping Banli-Zan, Shui Chon-Po, Chueh Zho-Gau).

/// Surnames: the retail examples and NPCs.
const SHO_SURNAMES: &[&str] = &[
    "Ninwa",
    "Fenping",
    "Shui",
    "Jyiigo",
    "Chueh",
    "Houlai",
    "Tanada",
    "Mitsuhara",
    "Ayaname",
    "Hoshino",
    "Zho",
    "Lo",
    "Jin",
    "Xun",
    "Lu",
    "Tsua",
    "Hasin",
    "Kousu",
    "Kyujo",
    "Nawamara",
    "Higashiyama",
    "Daisei",
    "Manto",
    "Sayuji",
    "Koga",
    "Hizuki",
    "Tomo",
];
const SHO_SYLLABLES: &[&str] = &[
    "xao", "ban", "li", "chon", "an", "zho", "shou", "hei", "kei", "lin", "tai", "ryu", "ken",
    "wa", "ping", "fen", "hou", "lai", "chu", "nin", "ko", "mi", "ra", "yo", "sai", "do", "hua",
    "shan", "gan", "tsu", "ma", "sen", "to", "pei", "sho",
];
const SHO_MALE_ENDS: &[&str] = &[
    "jhen", "zan", "po", "jen", "hao", "wei", "shan", "tai", "kei", "ro", "ten", "gan", "sho",
    "dou", "fo", "ren", "ji", "chi",
];
const SHO_FEMALE_ENDS: &[&str] = &[
    "ming", "gau", "youh", "ko", "mi", "na", "ka", "hei", "lan", "li", "yu", "sen", "ri", "ra",
    "hana", "lei", "ai", "mei",
];

fn sho_first(male: bool, rng: &mut NameRng) -> String {
    let ends = if male { SHO_MALE_ENDS } else { SHO_FEMALE_ENDS };
    let head = capital(rng.pick(SHO_SYLLABLES));
    let end = rng.pick(ends);
    if rng.chance(25) {
        format!("{head}-{}", capital(end))
    } else {
        format!("{head}{end}")
    }
}

fn sho_surname(rng: &mut NameRng) -> String {
    if rng.chance(40) {
        rng.pick(SHO_SURNAMES).to_owned()
    } else {
        capital(rng.pick(SHO_SYLLABLES)) + rng.pick(SHO_SYLLABLES)
    }
}

fn sho(male: bool, rng: &mut NameRng) -> String {
    let surname = sho_surname(rng);
    format!("{surname} {}", sho_first(male, rng))
}

// Viamontian. Commoners: a given name and a surname. Nobles: a given name, "di" (men) or "du"
// (women) and their fief.

const VIA_MALE: &[&str] = &[
    "Anton",
    "Lorca",
    "Daved",
    "Darren",
    "Lorenzo",
    "Carlo",
    "Aldoro",
    "Alessandro",
    "Andrea",
    "Armand",
    "Benedino",
    "Berrando",
    "Ciriaco",
    "Corrado",
    "Donatello",
    "Enrico",
    "Ennio",
    "Enzo",
    "Fabian",
    "Felizio",
    "Francois",
    "Gianni",
    "Giordonni",
    "Lucari",
    "Lucio",
    "Marcello",
    "Niccolo",
    "Orfeo",
    "Otto",
    "Piero",
    "Ricaldo",
    "Romeo",
    "Sandino",
    "Stuven",
    "Tito",
    "Turino",
    "Vettorio",
    "Vittore",
    "Varicci",
    "Rylane",
];
const VIA_FEMALE: &[&str] = &[
    "Lurissa",
    "Dinala",
    "Sina",
    "Eleonora",
    "Catlina",
    "Marena",
    "Annelina",
    "Arialle",
    "Aurellia",
    "Belinda",
    "Benedetta",
    "Brizinna",
    "Caprice",
    "Colista",
    "Elena",
    "Elise",
    "Eliza",
    "Elloisa",
    "Fiorella",
    "Francesca",
    "Gallatria",
    "Lacarra",
    "Lamorda",
    "Lila",
    "Luciana",
    "Mariabella",
    "Meerana",
    "Melaverre",
    "Mesalina",
    "Pia",
    "Renata",
    "Rufina",
    "Saphine",
    "Susana",
    "Tamara",
    "Valentina",
    "Vierana",
    "Viviana",
    "Monique",
    "Elda",
];
/// Commoners' surnames: the retail examples and NPCs.
const VIA_SURNAMES: &[&str] = &[
    "Silezzi",
    "Sammel",
    "Coppocilla",
    "Falzohn",
    "Roma",
    "Luchezzi",
    "Brasi",
    "Cellini",
    "Linante",
    "Malloni",
    "Murrio",
    "Piatelli",
    "Vernice",
    "Gallo",
    "Osemmi",
    "Ribini",
    "Montegarre",
    "Bilcone",
    "Fabarin",
    "Quios",
    "Arlean",
    "Avanthi",
    "Loschi",
    "Fiori",
    "Varanese",
    "Fortiore",
    "Molanti",
    "Brauno",
    "Fluress",
    "Calmasi",
    "Strinjelli",
    "Marinni",
    "Ilario",
    "Bernarri",
    "Cenizzi",
    "Dumalia",
    "Detante",
    "Mardor",
    "Dilazo",
    "Sillezi",
];
/// Fiefs and houses: the retail examples, NPCs and the great houses of the lore.
const VIA_FIEFS: &[&str] = &[
    "Marden",
    "Ricci",
    "Cenza",
    "Bellenesse",
    "Lotila",
    "Belleforte",
    "Cinghalle",
    "Alduressa",
    "Ricard",
    "Loc",
    "Furza",
    "Maki",
    "Lucrezza",
    "Soludatis",
    "Terli",
    "Spesi",
    "Bonachien",
    "Hergeaux",
    "Suinn",
    "Corcosi",
    "Lamiere",
    "Curena",
    "Orza",
    "Ressetta",
    "Laur",
    "Renari",
    "Vorchass",
    "Canamorra",
    "Bressa",
    "Lurio",
    "Frodenci",
];

fn viamontian_first(male: bool, rng: &mut NameRng) -> &'static str {
    rng.pick(if male { VIA_MALE } else { VIA_FEMALE })
}

fn viamontian(male: bool, rng: &mut NameRng) -> String {
    let first = viamontian_first(male, rng);
    if rng.chance(60) {
        format!("{first} {}", rng.pick(VIA_SURNAMES))
    } else {
        let of = if male { "di" } else { "du" };
        format!("{first} {of} {}", rng.pick(VIA_FIEFS))
    }
}

// Umbraen and Penumbraen. Names like the other Isparian heritages', or ones for their new chaotic
// nature (Uor Darkbound, Leikny Shadowsworn, Kurayami Mika, Elda du Nerezza).

const SHADOW_HEADS: &[&str] = &[
    "Uor", "Leik", "Vor", "Ess", "Mor", "Kael", "Syl", "Rhan", "Vey", "Dral", "Iss", "Thal", "Zev",
    "Sar", "Ul", "Ner", "Ves", "Kur",
];
const SHADOW_TAILS: &[&str] = &["", "", "ny", "a", "en", "is", "eth", "iyah", "ath", "e"];
const SHADOW_FIRST: &[&str] = &[
    "Dark", "Shadow", "Night", "Void", "Gloom", "Dusk", "Black", "Shade", "Grim", "Ash", "Umbral",
    "Hollow", "Murk",
];
const SHADOW_LAST: &[&str] = &[
    "bound", "sworn", "born", "walker", "veil", "cloak", "heart", "blade", "weaver", "fall",
    "song", "mantle", "thorn", "wake", "step", "fang",
];
/// The examples' words for darkness taken as names (Kurayami Mika, Kurasa Fudo, Elda du Nerezza).
const SHADOW_SHO_SURNAMES: &[&str] = &["Kurayami", "Kurasa"];

fn shadow(male: bool, rng: &mut NameRng) -> String {
    match rng.below(10) {
        0..=3 => format!(
            "{}{} {}",
            rng.pick(SHADOW_HEADS),
            rng.pick(SHADOW_TAILS),
            two(rng, SHADOW_FIRST, SHADOW_LAST)
        ),
        4 => format!("{} {}", rng.pick(SHADOW_SHO_SURNAMES), sho_first(male, rng)),
        5 => {
            let of = if male { "di" } else { "du" };
            let first = viamontian_first(male, rng);
            format!("{first} {of} Nerezza")
        }
        6 => aluvian(male, rng),
        7 => gharundim(male, rng),
        8 => sho(male, rng),
        _ => viamontian(male, rng),
    }
}

// Gear Knights. A very short first name; men a surname of the materials they are made of, women
// of the energies.

const GEAR_MALE: &[&str] = &[
    "Ulm", "Sor", "Kor", "Vol", "Kul", "Tor", "Dro", "Brak", "Gorm", "Hald", "Jor", "Krel", "Mok",
    "Orm", "Rusk", "Stav", "Tok", "Vek", "Zor", "Bram", "Dorn", "Grel", "Holt", "Kag", "Lund",
    "Narv", "Ost", "Rok", "Tarn", "Vorn", "Aur",
];
const GEAR_FEMALE: &[&str] = &[
    "Bei", "Loe", "Mila", "Elt", "Ast", "Bri", "Cela", "Dia", "Eko", "Fen", "Isa", "Kia", "Lio",
    "Nim", "Ora", "Rei", "Sia", "Tali", "Vey", "Zea", "Yla", "Ena", "Ilo", "Qia", "Tra",
];
const GEAR_MATERIALS: &[&str] = &[
    "Brass", "Iron", "Red", "Bolt", "Bronze", "Copper", "Steel", "Tin", "Gold", "Silver", "Cog",
    "Gear", "Rivet", "Lead", "Plate", "Spring", "Anvil", "Cobalt", "Pewter", "Chain", "Nickel",
];
const GEAR_MALE_WORDS: &[&str] = &[
    "guard", "strider", "gear", "hammer", "forge", "fist", "wright", "wheel", "plate", "shield",
    "heart", "spring", "cog", "mantle", "crank", "maul", "bolt", "breaker",
];
const GEAR_ENERGIES: &[&str] = &[
    "Flame",
    "Lightning",
    "Light",
    "Steam",
    "Spark",
    "Storm",
    "Ember",
    "Frost",
    "Arc",
    "Fire",
    "Thunder",
    "Blaze",
    "Glow",
    "Sun",
    "Star",
    "Heat",
    "Cinder",
    "Bright",
];
const GEAR_FEMALE_WORDS: &[&str] = &[
    "walker", "born", "dancer", "rent", "singer", "heart", "weaver", "song", "flare", "wake",
    "spire", "rider", "caller", "kissed", "veil",
];

fn gear_knight(male: bool, rng: &mut NameRng) -> String {
    let (first, a, b) = if male {
        (rng.pick(GEAR_MALE), GEAR_MATERIALS, GEAR_MALE_WORDS)
    } else {
        (rng.pick(GEAR_FEMALE), GEAR_ENERGIES, GEAR_FEMALE_WORDS)
    };
    loop {
        let (x, y) = (rng.pick(a), rng.pick(b));
        if !x.eq_ignore_ascii_case(y) {
            return format!("{first} {x}{y}");
        }
    }
}

// Tumerok. Tumerok words for a deed or occasion (Faranua, Taguauri); women's are short (Noua, Aeu,
// Kuri, Faun). A tribal prefix only on formal occasions: the heritage's own, Aun.

const TUMEROK_ROOTS: &[&str] = &[
    "Far", "Am", "Mir", "Tag", "Kir", "Quan", "Tah", "Taw", "Re", "Ur", "It", "Kal", "Mak", "Ten",
    "Hu", "Sar", "Pal", "Ko", "Tum", "Aw", "Nu", "Ow", "Hal", "Mau",
];
const TUMEROK_LINKS: &[&str] = &["a", "e", "u", "i", "ua", ""];
const TUMEROK_MALE_ENDS: &[&str] = &[
    "nua", "ona", "uri", "awa", "tea", "rea", "tal", "nah", "ua", "ahu", "oa", "ari", "tah", "ura",
];
const TUMEROK_FEMALE_HEADS: &[&str] = &[
    "No", "Ae", "Ku", "Fa", "Ri", "Ta", "Wa", "Ea", "Ia", "Mi", "Ha", "Ne", "Te", "Sa", "Ki", "Lu",
    "Oa", "Re", "Ma", "Pu", "Hi", "Ko", "Ao", "Ya",
];
const TUMEROK_FEMALE_ENDS: &[&str] = &[
    "ua", "u", "ri", "un", "a", "e", "i", "ah", "ia", "na", "uh", "ea",
];

fn tumerok(male: bool, rng: &mut NameRng) -> String {
    let name = if male {
        format!(
            "{}{}{}",
            rng.pick(TUMEROK_ROOTS),
            rng.pick(TUMEROK_LINKS),
            rng.pick(TUMEROK_MALE_ENDS)
        )
    } else {
        two(rng, TUMEROK_FEMALE_HEADS, TUMEROK_FEMALE_ENDS)
    };
    if rng.chance(20) {
        format!("Aun {name}")
    } else {
        name
    }
}

// Lugians. One name, of the Lugian's strength or skill. The rows as shipped: Nimias, Arstuus,
// Riekal, Namlos for men; Kresovous, Turikas, Menloth, Kanteroth for women.

const LUGIAN_MALE_ROOTS: &[&str] = &[
    "Nim", "Arst", "Riek", "Nam", "Dro", "Fren", "Treb", "Lin", "Grak", "Bor", "Kor", "Mag", "Vor",
    "Gol", "Rek", "Hurl", "Brun", "Tok", "Durg", "Sket", "Hav", "Urk", "Gonst", "Mok", "Thrak",
    "Bev", "Kurn",
];
const LUGIAN_MALE_ENDS: &[&str] = &[
    "ias", "uus", "al", "los", "ak", "us", "ok", "as", "vak", "uk", "aal", "ost", "orn", "ius",
    "eld", "ask",
];
const LUGIAN_FEMALE_ROOTS: &[&str] = &[
    "Kres", "Tur", "Men", "Kant", "Mal", "Ghar", "Sor", "Ven", "Thul", "Rag", "Bel", "Drak", "Ost",
    "Hes", "Var", "Tuk", "Sev", "Mor", "Krel", "Ilt", "Zha", "Ner", "Brak", "Dun",
];
const LUGIAN_FEMALE_ENDS: &[&str] = &[
    "ovous", "ikas", "loth", "eroth", "oth", "ova", "ika", "ethra", "isa", "uun", "ora", "alka",
    "evous", "ira", "asta",
];

fn lugian(male: bool, rng: &mut NameRng) -> String {
    if male {
        two(rng, LUGIAN_MALE_ROOTS, LUGIAN_MALE_ENDS)
    } else {
        two(rng, LUGIAN_FEMALE_ROOTS, LUGIAN_FEMALE_ENDS)
    }
}

// Empyreans. Their names vary considerably: an ancient culture of many empires. One name, some with
// an apostrophe (the NPCs Tarkor'in and Dan'eel).

const EMPYREAN_MALE_HEADS: &[&str] = &[
    "Ash", "Har", "Kel", "At", "Fian", "Til", "Mor", "Zab", "Ty", "Eor", "Tar", "Le", "Bre", "Al",
    "Ilo", "Var", "Cor", "Ser", "Thal", "Gael",
];
const EMPYREAN_MALE_ENDS: &[&str] = &[
    "eron", "lune", "lin", "lan", "he", "lahan", "gethais", "ien", "ragar", "linde", "korin",
    "arth", "em", "andar", "ion", "edrin",
];
const EMPYREAN_FEMALE_HEADS: &[&str] = &[
    "Ad", "Nal", "Mal", "Aer", "Lad", "Daen", "Ki", "Mash", "Sher", "Ila", "Ser", "Thal", "Ely",
    "Ves", "Cor", "Tri", "Dan",
];
const EMPYREAN_FEMALE_ENDS: &[&str] = &[
    "ja", "icana", "ia", "falle", "ice", "erah", "ara", "al", "ath", "ienne", "essa", "ine", "ura",
    "anna", "eel",
];

fn empyrean(male: bool, rng: &mut NameRng) -> String {
    let (head, end) = if male {
        (rng.pick(EMPYREAN_MALE_HEADS), rng.pick(EMPYREAN_MALE_ENDS))
    } else {
        (
            rng.pick(EMPYREAN_FEMALE_HEADS),
            rng.pick(EMPYREAN_FEMALE_ENDS),
        )
    };
    if rng.chance(15) {
        format!("{head}'{end}")
    } else {
        format!("{head}{end}")
    }
}

// Undead. Formal names like the Dericost they flee, or new ones in the Isparian manner; some keep
// their House name, Mhoire (Lord Davarri Mhoire, Joseph, Mhoire Atsuko).

const DERICOST_HEADS: &[&str] = &[
    "Dav", "Aer", "Ger", "Var", "Mor", "Cal", "Ser", "Hal", "Ven", "Ith", "Dor", "El", "Tav", "Ul",
];
const DERICOST_ENDS: &[&str] = &[
    "arri", "fenye", "aine", "enne", "ias", "orin", "ethe", "avar", "ielle", "anor", "esse", "uin",
];

fn undead(male: bool, rng: &mut NameRng) -> String {
    match rng.below(20) {
        0..=6 => {
            let first = two(rng, DERICOST_HEADS, DERICOST_ENDS);
            if rng.chance(30) {
                let title = if male { "Lord" } else { "Lady" };
                format!("{title} {first} Mhoire")
            } else {
                format!("{first} Mhoire")
            }
        }
        7..=10 => format!("Mhoire {}", sho_first(male, rng)),
        11..=13 => aluvian_first(male, rng),
        14..=15 => gharundim_first(male, rng),
        16..=17 => sho_first(male, rng),
        _ => viamontian_first(male, rng).to_owned(),
    }
}

// Olthoi. They know each other by scent and clicks; a player's name is the title the other races
// give them, a description.

const OLTHOI_HEADS: &[&str] = &[
    "Marrow", "Blood", "Skull", "Shell", "Ash", "Mire", "Dune", "Sand", "Black", "Red", "Pale",
    "Grey", "Iron", "Swift", "Hollow", "Bitter", "Night", "Dust", "Hive", "Stone", "Scar", "Grave",
];
const OLTHOI_ACID_HEADS: &[&str] = &[
    "Acid", "Bile", "Venom", "Gall", "Blister", "Rot", "Sour", "Smoke", "Burn", "Melt", "Green",
];
const OLTHOI_TAILS: &[&str] = &[
    "render", "ripper", "gnasher", "cleaver", "crusher", "stalker", "digger", "skitter", "maw",
    "claw", "sting", "burrower", "eater", "biter", "hunter", "shell", "jaw", "pincer", "taker",
    "breaker", "chitter",
];
const OLTHOI_ACID_TAILS: &[&str] = &["spitter", "spray", "burner", "melter", "drip", "spit"];
const OLTHOI_WORDS: &[&str] = &[
    "Old", "Pale", "Black", "Red", "Great", "Silent", "Swift", "Grim", "Hungry", "Bitter", "Lame",
    "Scarred", "Twice",
];

fn olthoi(acid: bool, rng: &mut NameRng) -> String {
    let head = if acid && rng.chance(60) {
        rng.pick(OLTHOI_ACID_HEADS)
    } else {
        rng.pick(OLTHOI_HEADS)
    };
    let tail = if acid && rng.chance(50) {
        rng.pick(OLTHOI_ACID_TAILS)
    } else {
        rng.pick(OLTHOI_TAILS)
    };
    let title = format!("{head}{tail}");
    if rng.chance(30) {
        format!("{} {title}", rng.pick(OLTHOI_WORDS))
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const ALL: [u32; 13] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];

    fn many(heritage: u32, sex: u32, seed: u64, n: usize) -> Vec<String> {
        let mut rng = NameRng::new(seed);
        (0..n)
            .map(|_| generate(heritage, sex, &mut rng).expect("a name"))
            .collect()
    }

    #[test]
    fn every_name_is_one_the_name_box_takes_unchanged() {
        for h in ALL {
            for sex in [SEX_MALE, SEX_FEMALE] {
                for name in many(h, sex, u64::from(h * 10 + sex), 2000) {
                    let mut state = crate::CharGenState::default();
                    assert!(state.set_name(&name), "{h}/{sex}: {name:?} refused");
                    assert_eq!(state.name, name, "{h}/{sex}: formatting changed it");
                    assert!(fits_name_filter(&name), "{name}");
                    assert!(name.len() <= 32);
                }
            }
        }
    }

    #[test]
    fn the_same_seed_draws_the_same_names() {
        for h in ALL {
            assert_eq!(many(h, SEX_MALE, 7, 50), many(h, SEX_MALE, 7, 50));
            assert_ne!(many(h, SEX_FEMALE, 7, 50), many(h, SEX_FEMALE, 8, 50));
        }
    }

    #[test]
    fn each_heritage_and_sex_has_hundreds_of_names() {
        for h in ALL {
            for sex in [SEX_MALE, SEX_FEMALE] {
                let names = many(h, sex, 99, 1000);
                let distinct: BTreeSet<_> = names.iter().collect();
                assert!(distinct.len() >= 300, "{h}/{sex}: {}", distinct.len());
            }
        }
    }

    #[test]
    fn the_forms_follow_the_game_s_naming_text() {
        let has = |names: &[String], f: &dyn Fn(&str) -> bool| names.iter().any(|n| f(n));
        let aluvian_m = many(1, SEX_MALE, 1, 400);
        assert!(has(&aluvian_m, &|n| !n.contains(' ')));
        assert!(has(&aluvian_m, &|n| n.contains(" the ")));
        assert!(has(&aluvian_m, &|n| n.ends_with("sun")));
        assert!(!has(&aluvian_m, &|n| n.ends_with("mad")));
        assert!(has(&many(1, SEX_FEMALE, 1, 400), &|n| n.ends_with("mad")));
        let gharu_m = many(2, SEX_MALE, 1, 400);
        assert!(has(&gharu_m, &|n| n.contains(" al-")));
        assert!(has(&gharu_m, &|n| n.contains(" ibn ")));
        assert!(!has(&gharu_m, &|n| n.contains(" bint ")));
        assert!(has(&many(2, SEX_FEMALE, 1, 400), &|n| n.contains(" bint ")));
        assert!(many(3, SEX_MALE, 1, 400).iter().all(|n| n.contains(' ')));
        assert!(has(&many(3, SEX_FEMALE, 1, 400), &|n| n.contains('-')));
        let via_m = many(4, SEX_MALE, 1, 400);
        assert!(has(&via_m, &|n| n.contains(" di ")));
        assert!(!has(&via_m, &|n| n.contains(" du ")));
        assert!(has(&many(4, SEX_FEMALE, 1, 400), &|n| n.contains(" du ")));
        assert!(has(&many(11, SEX_MALE, 1, 400), &|n| n.contains("Mhoire")));
        assert!(has(&many(7, SEX_MALE, 1, 400), &|n| n.starts_with("Aun ")));
        assert!(many(8, SEX_FEMALE, 1, 400).iter().all(|n| !n.contains(' ')));
    }

    #[test]
    fn only_the_original_heritages_are_offered_before_throne_of_destiny() {
        for era in [EraId::Launch, EraId::Infiltration] {
            assert!((1..=3).all(|h| offered(era, h)));
            assert!((4..=13).all(|h| !offered(era, h)));
        }
        assert!(ALL.iter().all(|h| offered(EraId::Eor, *h)));
        assert!(!offered(EraId::Eor, 14));
        assert_eq!(generate(14, SEX_MALE, &mut NameRng::new(1)), None);
    }

    #[test]
    fn the_game_s_own_examples_keep_the_name_filter() {
        for name in [
            "Wothgryn",
            "Osgeld the Clever",
            "Khayla'ida bint Tiqiq",
            "Fenping Banli-Zan",
            "Jyiigo Anming",
            "Ulm Brassguard",
            "Taguauri",
            "Aeu",
            "Kresovous",
            "Lord Davarri Mhoire",
        ] {
            assert!(fits_name_filter(name), "{name}");
        }
        for name in ["Aaab", "Brrrm", "Strndk", "Euoia"] {
            assert!(!fits_name_filter(name), "{name}");
        }
    }

    #[test]
    fn a_refused_name_is_drawn_again() {
        let mut rng = NameRng::new(3);
        let first = generate(1, SEX_MALE, &mut rng.clone()).unwrap();
        let other = generate_where(1, SEX_MALE, &mut rng, |n| n != first).unwrap();
        assert_ne!(first, other);
    }

    #[test]
    fn a_typed_name_the_formatting_changes_is_not_valid() {
        assert!(is_valid("Osgeld the Clever"));
        assert!(is_valid("Ma'whar al-Qutuz"));
        assert!(is_valid("Fenping Banli-Zan"));
        assert!(!is_valid("osgeld"));
        assert!(!is_valid("Two  Spaces"));
        assert!(!is_valid(""));
        assert!(!is_valid(&"A".repeat(33)));
    }
}
