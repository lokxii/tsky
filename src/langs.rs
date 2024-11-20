pub struct Lang {
    pub code: &'static str,
    pub name: &'static str,
}

pub const LANGS: [Lang; 204] = [
    Lang { code: "aa", name: "Afar", },
    Lang { code: "ab", name: "Abkhazian", },
    Lang { code: "af", name: "Afrikaans", },
    Lang { code: "ak", name: "Akan", },
    Lang { code: "sq", name: "Albanian", },
    Lang { code: "am", name: "Amharic", },
    Lang { code: "ar", name: "Arabic", },
    Lang { code: "an", name: "Aragonese", },
    Lang { code: "hy", name: "Armenian", },
    Lang { code: "as", name: "Assamese", },
    Lang { code: "av", name: "Avaric", },
    Lang { code: "ae", name: "Avestan", },
    Lang { code: "ay", name: "Aymara", },
    Lang { code: "az", name: "Azerbaijani", },
    Lang { code: "ba", name: "Bashkir", },
    Lang { code: "bm", name: "Bambara", },
    Lang { code: "eu", name: "Basque", },
    Lang { code: "be", name: "Belarusian", },
    Lang { code: "bn", name: "Bengali", },
    Lang { code: "bh", name: "Bihari languages", },
    Lang { code: "bi", name: "Bislama", },
    Lang { code: "bo", name: "Tibetan", },
    Lang { code: "bs", name: "Bosnian", },
    Lang { code: "br", name: "Breton", },
    Lang { code: "bg", name: "Bulgarian", },
    Lang { code: "my", name: "Burmese", },
    Lang { code: "ca", name: "Catalan; Valencian", },
    Lang { code: "cs", name: "Czech", },
    Lang { code: "ch", name: "Chamorro", },
    Lang { code: "ce", name: "Chechen", },
    Lang { code: "zh", name: "Chinese", },
    Lang { code: "cu", name: "Church Slavic; Old Slavonic; Church Slavonic; Old Bulgarian; Old Church Slavonic", },
    Lang { code: "cv", name: "Chuvash", },
    Lang { code: "kw", name: "Cornish", },
    Lang { code: "co", name: "Corsican", },
    Lang { code: "cr", name: "Cree", },
    Lang { code: "cy", name: "Welsh", },
    Lang { code: "cs", name: "Czech", },
    Lang { code: "da", name: "Danish", },
    Lang { code: "de", name: "German", },
    Lang { code: "dv", name: "Divehi; Dhivehi; Maldivian", },
    Lang { code: "nl", name: "Dutch; Flemish", },
    Lang { code: "dz", name: "Dzongkha", },
    Lang { code: "el", name: "Greek, Modern (1453-)", },
    Lang { code: "en", name: "English", },
    Lang { code: "eo", name: "Esperanto", },
    Lang { code: "et", name: "Estonian", },
    Lang { code: "eu", name: "Basque", },
    Lang { code: "ee", name: "Ewe", },
    Lang { code: "fo", name: "Faroese", },
    Lang { code: "fa", name: "Persian", },
    Lang { code: "fj", name: "Fijian", },
    Lang { code: "fi", name: "Finnish", },
    Lang { code: "fr", name: "French", },
    Lang { code: "fr", name: "French", },
    Lang { code: "fy", name: "Western Frisian", },
    Lang { code: "ff", name: "Fulah", },
    Lang { code: "ka", name: "Georgian", },
    Lang { code: "de", name: "German", },
    Lang { code: "gd", name: "Gaelic; Scottish Gaelic", },
    Lang { code: "ga", name: "Irish", },
    Lang { code: "gl", name: "Galician", },
    Lang { code: "gv", name: "Manx", },
    Lang { code: "el", name: "Greek, Modern (1453-)", },
    Lang { code: "gn", name: "Guarani", },
    Lang { code: "gu", name: "Gujarati", },
    Lang { code: "ht", name: "Haitian; Haitian Creole", },
    Lang { code: "ha", name: "Hausa", },
    Lang { code: "he", name: "Hebrew", },
    Lang { code: "hz", name: "Herero", },
    Lang { code: "hi", name: "Hindi", },
    Lang { code: "ho", name: "Hiri Motu", },
    Lang { code: "hr", name: "Croatian", },
    Lang { code: "hu", name: "Hungarian", },
    Lang { code: "hy", name: "Armenian", },
    Lang { code: "ig", name: "Igbo", },
    Lang { code: "is", name: "Icelandic", },
    Lang { code: "io", name: "Ido", },
    Lang { code: "ii", name: "Sichuan Yi; Nuosu", },
    Lang { code: "iu", name: "Inuktitut", },
    Lang { code: "ie", name: "Interlingue; Occidental", },
    Lang { code: "ia", name: "Interlingua (International Auxiliary Language Association)", },
    Lang { code: "id", name: "Indonesian", },
    Lang { code: "ik", name: "Inupiaq", },
    Lang { code: "is", name: "Icelandic", },
    Lang { code: "it", name: "Italian", },
    Lang { code: "jv", name: "Javanese", },
    Lang { code: "ja", name: "Japanese", },
    Lang { code: "kl", name: "Kalaallisut; Greenlandic", },
    Lang { code: "kn", name: "Kannada", },
    Lang { code: "ks", name: "Kashmiri", },
    Lang { code: "ka", name: "Georgian", },
    Lang { code: "kr", name: "Kanuri", },
    Lang { code: "kk", name: "Kazakh", },
    Lang { code: "km", name: "Central Khmer", },
    Lang { code: "ki", name: "Kikuyu; Gikuyu", },
    Lang { code: "rw", name: "Kinyarwanda", },
    Lang { code: "ky", name: "Kirghiz; Kyrgyz", },
    Lang { code: "kv", name: "Komi", },
    Lang { code: "kg", name: "Kongo", },
    Lang { code: "ko", name: "Korean", },
    Lang { code: "kj", name: "Kuanyama; Kwanyama", },
    Lang { code: "ku", name: "Kurdish", },
    Lang { code: "lo", name: "Lao", },
    Lang { code: "la", name: "Latin", },
    Lang { code: "lv", name: "Latvian", },
    Lang { code: "li", name: "Limburgan; Limburger; Limburgish", },
    Lang { code: "ln", name: "Lingala", },
    Lang { code: "lt", name: "Lithuanian", },
    Lang { code: "lb", name: "Luxembourgish; Letzeburgesch", },
    Lang { code: "lu", name: "Luba-Katanga", },
    Lang { code: "lg", name: "Ganda", },
    Lang { code: "mk", name: "Macedonian", },
    Lang { code: "mh", name: "Marshallese", },
    Lang { code: "ml", name: "Malayalam", },
    Lang { code: "mi", name: "Maori", },
    Lang { code: "mr", name: "Marathi", },
    Lang { code: "ms", name: "Malay", },
    Lang { code: "mk", name: "Macedonian", },
    Lang { code: "mg", name: "Malagasy", },
    Lang { code: "mt", name: "Maltese", },
    Lang { code: "mn", name: "Mongolian", },
    Lang { code: "mi", name: "Maori", },
    Lang { code: "ms", name: "Malay", },
    Lang { code: "my", name: "Burmese", },
    Lang { code: "na", name: "Nauru", },
    Lang { code: "nv", name: "Navajo; Navaho", },
    Lang { code: "nr", name: "Ndebele, South; South Ndebele", },
    Lang { code: "nd", name: "Ndebele, North; North Ndebele", },
    Lang { code: "ng", name: "Ndonga", },
    Lang { code: "ne", name: "Nepali", },
    Lang { code: "nl", name: "Dutch; Flemish", },
    Lang { code: "nn", name: "Norwegian Nynorsk; Nynorsk, Norwegian", },
    Lang { code: "nb", name: "Bokmål, Norwegian; Norwegian Bokmål", },
    Lang { code: "no", name: "Norwegian", },
    Lang { code: "ny", name: "Chichewa; Chewa; Nyanja", },
    Lang { code: "oc", name: "Occitan (post 1500)", },
    Lang { code: "oj", name: "Ojibwa", },
    Lang { code: "or", name: "Oriya", },
    Lang { code: "om", name: "Oromo", },
    Lang { code: "os", name: "Ossetian; Ossetic", },
    Lang { code: "pa", name: "Panjabi; Punjabi", },
    Lang { code: "fa", name: "Persian", },
    Lang { code: "pi", name: "Pali", },
    Lang { code: "pl", name: "Polish", },
    Lang { code: "pt", name: "Portuguese", },
    Lang { code: "ps", name: "Pushto; Pashto", },
    Lang { code: "qu", name: "Quechua", },
    Lang { code: "rm", name: "Romansh", },
    Lang { code: "ro", name: "Romanian; Moldavian; Moldovan", },
    Lang { code: "ro", name: "Romanian; Moldavian; Moldovan", },
    Lang { code: "rn", name: "Rundi", },
    Lang { code: "ru", name: "Russian", },
    Lang { code: "sg", name: "Sango", },
    Lang { code: "sa", name: "Sanskrit", },
    Lang { code: "si", name: "Sinhala; Sinhalese", },
    Lang { code: "sk", name: "Slovak", },
    Lang { code: "sk", name: "Slovak", },
    Lang { code: "sl", name: "Slovenian", },
    Lang { code: "se", name: "Northern Sami", },
    Lang { code: "sm", name: "Samoan", },
    Lang { code: "sn", name: "Shona", },
    Lang { code: "sd", name: "Sindhi", },
    Lang { code: "so", name: "Somali", },
    Lang { code: "st", name: "Sotho, Southern", },
    Lang { code: "es", name: "Spanish", },
    Lang { code: "sq", name: "Albanian", },
    Lang { code: "sc", name: "Sardinian", },
    Lang { code: "sr", name: "Serbian", },
    Lang { code: "ss", name: "Swati", },
    Lang { code: "su", name: "Sundanese", },
    Lang { code: "sw", name: "Swahili", },
    Lang { code: "sv", name: "Swedish", },
    Lang { code: "ty", name: "Tahitian", },
    Lang { code: "ta", name: "Tamil", },
    Lang { code: "tt", name: "Tatar", },
    Lang { code: "te", name: "Telugu", },
    Lang { code: "tg", name: "Tajik", },
    Lang { code: "tl", name: "Tagalog", },
    Lang { code: "th", name: "Thai", },
    Lang { code: "bo", name: "Tibetan", },
    Lang { code: "ti", name: "Tigrinya", },
    Lang { code: "to", name: "Tonga (Tonga Islands)", },
    Lang { code: "tn", name: "Tswana", },
    Lang { code: "ts", name: "Tsonga", },
    Lang { code: "tk", name: "Turkmen", },
    Lang { code: "tr", name: "Turkish", },
    Lang { code: "tw", name: "Twi", },
    Lang { code: "ug", name: "Uighur; Uyghur", },
    Lang { code: "uk", name: "Ukrainian", },
    Lang { code: "ur", name: "Urdu", },
    Lang { code: "uz", name: "Uzbek", },
    Lang { code: "ve", name: "Venda", },
    Lang { code: "vi", name: "Vietnamese", },
    Lang { code: "vo", name: "Volapük", },
    Lang { code: "cy", name: "Welsh", },
    Lang { code: "wa", name: "Walloon", },
    Lang { code: "wo", name: "Wolof", },
    Lang { code: "xh", name: "Xhosa", },
    Lang { code: "yi", name: "Yiddish", },
    Lang { code: "yo", name: "Yoruba", },
    Lang { code: "za", name: "Zhuang; Chuang", },
    Lang { code: "zh", name: "Chinese", },
    Lang { code: "zu", name: "Zulu", },
];
