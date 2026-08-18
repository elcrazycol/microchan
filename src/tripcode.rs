//! Трипкоды: классический (совместим с vichan/4chan, DES crypt)
//! и secure (extended DES с секретом из конфига).
//!
//! Классический трипкод — традиционный Unix crypt(3) (алгоритм eay/FreeSec):
//! соль из символов 1-2 текста, 25 итераций DES над нулевым блоком,
//! результат кодируется в 11 символов, берутся последние 10.
//!
//! Secure трипкод — BSD-style extended DES (`_` + 4 символа счёта + 4 соли),
//! как в vichan: соль = первые 4 символа base64(sha1(текст + secure_salt)).

use base64::Engine;
use encoding_rs::SHIFT_JIS;
use hmac::{digest::KeyInit, Hmac};
use hmac::Mac;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// Классическая и secure части трипкода, если они есть.
pub struct Tripcodes {
    /// Классический трипкод (без префикса "!").
    pub classic: Option<String>,
    /// Secure трипкод (без префикса "!!").
    pub secure: Option<String>,
}

/// Разбирает поле "name": `Имя#классический` или `Имя##secure`.
/// Возвращает (имя, трипкоды).
pub fn split_name(raw: &str) -> (String, Tripcodes) {
    let Some(idx) = raw.find('#') else {
        return (
            raw.trim().to_string(),
            Tripcodes { classic: None, secure: None },
        );
    };
    let name = raw[..idx].trim().to_string();
    let rest = &raw[idx + 1..];
    if let Some(seed) = rest.strip_prefix('#') {
        (name, Tripcodes { classic: None, secure: Some(seed.to_string()) })
    } else {
        (name, Tripcodes { classic: Some(rest.to_string()), secure: None })
    }
}

impl Tripcodes {
    /// Вычисляет итоговый трипкод для отображения (с префиксом `!` или `!!`).
    pub fn render(self, secure_salt: &str) -> Option<String> {
        if let Some(seed) = self.secure {
            return secure_tripcode(&seed, secure_salt).map(|t| format!("!!{t}"));
        }
        if let Some(seed) = self.classic {
            return classic_tripcode(&seed).map(|t| format!("!{t}"));
        }
        None
    }
}

/// Вычисляет классический трипкод (без префикса "!").
pub fn classic_tripcode(seed: &str) -> Option<String> {
    let text = to_shift_jis(seed);
    if text.is_empty() {
        return None;
    }
    // Соль: substr(text + "H..", 1, 2) с фиксацией таблицы.
    let mut salt_src = Vec::with_capacity(text.len() + 3);
    salt_src.extend_from_slice(&text);
    salt_src.extend_from_slice(b"H..");
    let salt = [
        fix_salt_char(salt_src[1]),
        fix_salt_char(salt_src[2]),
    ];

    let mut key = [0u8; 8];
    key[..text.len().min(8)].copy_from_slice(&text[..text.len().min(8)]);

    let out = des_crypt(&key, &salt, 25);
    Some(out[out.len() - 10..].to_string())
}

/// Вычисляет secure трипкод (без префикса "!!").
pub fn secure_tripcode(seed: &str, secure_salt: &str) -> Option<String> {
    let text = to_shift_jis(seed);
    if text.is_empty() {
        return None;
    }
    // base64(sha1(text + secure_salt)), первые 4 символа, '+' -> '.'
    let mut hasher = Sha1::new();
    hasher.update(&text);
    hasher.update(secure_salt.as_bytes());
    let digest = hasher.finalize();
    let b64 = base64::engine::general_purpose::STANDARD.encode(digest);
    let mut salt4 = [b'.'; 4];
    for (i, c) in b64.bytes().take(4).enumerate() {
        salt4[i] = if c == b'+' { b'.' } else { c };
    }

    // setting: '_' + "..A." + salt4 (как в vichan)
    let setting = [b'_', b'.', b'.', b'A', b'.', salt4[0], salt4[1], salt4[2], salt4[3]];
    let count = decode_count(&setting);

    // Фолдинг пароля (Merkle–Damgård), соль = 0 на этом этапе.
    // Эталон (libxcrypt): keybuf_ref = pkbuf ^ (ch << 1) — это и plaintext шифрования,
    // а в ключ идут биты 7..1 keybuf_ref, т.е. (pkbuf >> 1) ^ ch в битах 6..0.
    let mut keybuf = [0u8; 8];
    let mut plain = [0u8; 8];
    let mut pkbuf = [0u8; 8];
    let mut phrase = text.as_slice();
    loop {
        for i in 0..8 {
            let ch = phrase.first().copied().unwrap_or(0);
            plain[i] = pkbuf[i] ^ (ch << 1);
            keybuf[i] = (pkbuf[i] >> 1) ^ ch;
            if !phrase.is_empty() {
                phrase = &phrase[1..];
            }
        }
        if phrase.is_empty() {
            break;
        }
        // DES с солью 0, 1 итерация: шифруем plain -> pkbuf.
        let encoded = des_crypt_with_salt(&keybuf, &[], 1, &plain);
        pkbuf = decode_hash_bytes(&encoded);
    }

    // Основной хэш: 4-символьная соль, count итераций, нулевой блок.
    let encoded = des_crypt_with_salt(&keybuf, &salt4, count, &[0u8; 8]);
    Some(encoded[encoded.len() - 10..].to_string())
}

/// HMAC-SHA256 от IP с секретом — хэш для банов (без хранения IP).
pub fn hash_ip(ip: &str, secret: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("hmac key from any bytes");
    mac.update(ip.as_bytes());
    let out = mac.finalize().into_bytes();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 файла (hex) — для банов по файлу.
pub fn file_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let out = hasher.finalize();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

/// Конвертирует UTF-8 в Shift_JIS (как vichan); неразличимые символы -> '?'.
fn to_shift_jis(s: &str) -> Vec<u8> {
    let (bytes, _, had_errors) = SHIFT_JIS.encode(s);
    if !had_errors {
        return bytes.into_owned();
    }
    // Если были ошибки — заменяем непредставимые символы на '?'.
    let utf8 = String::from_utf8_lossy(bytes.as_ref());
    utf8.chars()
        .flat_map(|c| {
            if c == '\u{FFFD}' {
                vec![b'?']
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        })
        .collect()
}

/// Фиксация символа соли: [^.-z] -> '.', затем таблица замен.
fn fix_salt_char(c: u8) -> u8 {
    let c = if !(b'.'..=b'z').contains(&c) { b'.' } else { c };
    const FROM: &[u8] = b":;<=>?@[\\]^_`";
    const TO: &[u8] = b"ABCDEFGabcdef";
    match FROM.iter().position(|&x| x == c) {
        Some(i) => TO[i],
        None => c,
    }
}

/// Декодирует 4 символа счёта из setting (позиции 1..5).
fn decode_count(setting: &[u8; 9]) -> u32 {
    let mut count: u32 = 0;
    for (i, &ch) in setting[1..5].iter().enumerate() {
        count |= (ascii_to_bin(ch) as u32) << (i * 6);
    }
    count
}

fn ascii_to_bin(ch: u8) -> u8 {
    match ch {
        b'.'..=b'9' => ch - b'.',
        b'A'..=b'Z' => ch - b'A' + 12,
        b'a'..=b'z' => ch - b'a' + 38,
        _ => 0,
    }
}

// ---------------------------------------------------------------- DES crypt

const PC1_C: [usize; 28] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60,
    52, 44, 36,
];
const PC1_D: [usize; 28] = [
    63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 28,
    20, 12, 4,
];
const PC2_C: [usize; 24] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2,
];
const PC2_D: [usize; 24] = [
    41, 52, 31, 37, 47, 55, 30, 40, 51, 45, 33, 48, 44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];
const E: [usize; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17, 16, 17, 18,
    19, 20, 21, 20, 21, 22, 23, 24, 25, 24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];
const IP: [usize; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6, 64,
    56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61, 53,
    45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];
const FP: [usize; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30, 37,
    5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27, 34, 2,
    42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];
const S: [[u8; 64]; 8] = [
    [
        14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12,
        11, 9, 5, 3, 8, 4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9, 1,
        7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
        15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1,
        10, 6, 9, 11, 5, 0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15,
        4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
        10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14,
        12, 11, 15, 1, 13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6, 9, 8,
        7, 4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
        7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2,
        12, 1, 10, 14, 9, 10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10, 1,
        13, 8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
        2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15,
        10, 3, 9, 8, 6, 4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14,
        2, 13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
        12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13,
        14, 0, 11, 3, 8, 9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5,
        15, 10, 11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
        4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5,
        12, 2, 15, 8, 6, 1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4,
        10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
        13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6,
        11, 0, 14, 9, 2, 7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10,
        8, 13, 15, 12, 9, 0, 3, 5, 6, 11,
    ],
];
const P: [usize; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19, 13,
    30, 6, 22, 11, 4, 25,
];
const SHIFTS: [u8; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

const ASCII64: &[u8] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Традиционный DES crypt: 2-символьная соль, 25 итераций, нулевой блок.
/// Возвращает полную строку "SS" + 11 символов.
fn des_crypt(key: &[u8; 8], salt: &[u8; 2], iterations: u32) -> String {
    let mut out = des_crypt_with_salt(key, salt, iterations, &[0u8; 8]);
    out.insert_str(0, &String::from_utf8_lossy(salt));
    out
}

/// DES crypt с произвольной солью (0, 2 или 4 символа) и числом итераций.
/// Возвращает 11 закодированных символов.
fn des_crypt_with_salt(key: &[u8; 8], salt: &[u8], iterations: u32, plaintext: &[u8; 8]) -> String {
    // Раскладка ключа.
    let mut block = [0u8; 66];
    for i in 0..8 {
        for j in 0..7 {
            block[8 * i + j] = (key[i] >> (6 - j)) & 1;
        }
    }
    let mut c = [0u8; 28];
    let mut d = [0u8; 28];
    for i in 0..28 {
        c[i] = block[PC1_C[i] - 1];
        d[i] = block[PC1_D[i] - 1];
    }
    let mut ks = [[0u8; 48]; 16];
    for i in 0..16 {
        for _ in 0..SHIFTS[i] {
            c.rotate_left(1);
            d.rotate_left(1);
        }
        for j in 0..24 {
            ks[i][j] = c[PC2_C[j] - 1];
            ks[i][j + 24] = d[PC2_D[j] - 28 - 1];
        }
    }

    // Соль складывается в E-таблицу (эквивалент классического crypt).
    let mut e = E;
    for (i, &ch) in salt.iter().enumerate() {
        let val = ascii_to_bin(ch);
        for j in 0..6 {
            if (val >> j) & 1 == 1 {
                e.swap(6 * i + j, 6 * i + j + 24);
            }
        }
    }

    // Входной блок: обнуляем (важно!) и раскладываем plaintext по битам.
    block = [0u8; 66];
    for i in 0..8 {
        for j in 0..8 {
            block[8 * i + j] = (plaintext[i] >> (7 - j)) & 1;
        }
    }

    let mut l = [0u8; 32];
    let mut r = [0u8; 32];
    let mut dmy = [0u8; 32];
    let mut pres = [0u8; 48];
    let mut f = [0u8; 32];

    for _ in 0..iterations {
        for i in 0..32 {
            l[i] = block[IP[i] - 1];
        }
        for i in 32..64 {
            r[i - 32] = block[IP[i] - 1];
        }
        for i in 0..16 {
            dmy.copy_from_slice(&r);
            for j in 0..48 {
                pres[j] = r[e[j] - 1] ^ ks[i][j];
            }
            for j in 0..8 {
                let t = 6 * j;
                let k = S[j]
                    [((pres[t] as usize) << 5)
                        | ((pres[t + 1] as usize) << 3)
                        | ((pres[t + 2] as usize) << 2)
                        | ((pres[t + 3] as usize) << 1)
                        | (pres[t + 4] as usize)
                        | ((pres[t + 5] as usize) << 4)];
                let t2 = 4 * j;
                f[t2] = (k >> 3) & 1;
                f[t2 + 1] = (k >> 2) & 1;
                f[t2 + 2] = (k >> 1) & 1;
                f[t2 + 3] = k & 1;
            }
            for j in 0..32 {
                r[j] = l[j] ^ f[P[j] - 1];
            }
            l.copy_from_slice(&dmy);
        }
        // Финальный обмен L/R.
        for i in 0..32 {
            let tmp = l[i];
            l[i] = r[i];
            r[i] = tmp;
        }
        // FP.
        let mut dmy_block = [0u8; 64];
        for i in 0..32 {
            dmy_block[i] = l[i];
        }
        for i in 32..64 {
            dmy_block[i] = r[i - 32];
        }
        for i in 0..64 {
            block[i] = dmy_block[FP[i] - 1];
        }
    }

    // Кодирование 66 бит в 11 символов.
    let mut out = String::with_capacity(11);
    for i in 0..11 {
        let mut val: u8 = 0;
        for j in 0..6 {
            val = (val << 1) | block[6 * i + j];
        }
        out.push(ASCII64[val as usize] as char);
    }
    out
}

/// Декодирует 11 закодированных символов обратно в 8 байт (для фолдинга).
fn decode_hash_bytes(encoded: &str) -> [u8; 8] {
    let mut block = [0u8; 66];
    for (i, ch) in encoded.bytes().take(11).enumerate() {
        let val = ascii_to_bin(ch);
        for j in 0..6 {
            block[6 * i + j] = (val >> (5 - j)) & 1;
        }
    }
    let mut out = [0u8; 8];
    for i in 0..8 {
        for j in 0..8 {
            out[i] = (out[i] << 1) | block[8 * i + j];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_crypt_vectors() {
        // Эталоны с macOS (perl crypt):
        assert_eq!(des_crypt(b"asd\0\0\0\0\0", b"..", 25), "..yXBEO806ZEA");
        assert_eq!(des_crypt(b"a\0\0\0\0\0\0\0", b"H.", 25), "H.6ZnBI2EKkq.");
        assert_eq!(des_crypt(b"asd\0\0\0\0\0", b"sd", 25), "sdOTAPy3blMsc");
    }

    #[test]
    fn split_name_basic() {
        let (name, trips) = split_name("Anonymous");
        assert_eq!(name, "Anonymous");
        assert!(trips.classic.is_none() && trips.secure.is_none());

        let (name, trips) = split_name("Foo#bar");
        assert_eq!(name, "Foo");
        assert_eq!(trips.classic.as_deref(), Some("bar"));

        let (name, trips) = split_name("Foo##secret");
        assert_eq!(name, "Foo");
        assert_eq!(trips.secure.as_deref(), Some("secret"));

        // Имя без трипкода, но с "!" — не должно разбираться как трипкод.
        let (name, trips) = split_name("Foo!bar");
        assert_eq!(name, "Foo!bar");
        assert!(trips.classic.is_none() && trips.secure.is_none());
    }

    #[test]
    fn classic_known_vectors() {
        // Векторы из aquilax/tripcode (Go, eay-совместимый).
        assert_eq!(classic_tripcode("asd").as_deref(), Some("TAPy3blMsc"));
        assert_eq!(classic_tripcode("adasd").as_deref(), Some("IOuORdzMKw"));
        assert_eq!(classic_tripcode("!").as_deref(), Some("KNs1o0VDv6"));
        assert_eq!(classic_tripcode("#").as_deref(), Some("u2YjtUz8MU"));
        assert_eq!(classic_tripcode("%").as_deref(), Some("1t98deumW."));
        // perl: crypt("rasmusle", "as") = as7P8/.5y85.M (соль "as" из имени)
        assert_eq!(classic_tripcode("rasmuslerdorf"), Some("P8/.5y85.M".into()));
    }

    #[test]
    fn secure_tripcode_long_password() {
        // php -r "echo crypt('rasmuslerdorf', '_J9..rasm');" -> _J9..rasmBYk8r9AiWNc
        // Наш secure_tripcode использует фиксированный счёт '_..A.' — вектор ниже
        // проверяет совместимость фолдинга через прямой вызов с солью "rasm" и счётом 725.
        let text = to_shift_jis("rasmuslerdorf");
        let mut keybuf = [0u8; 8];
        let mut plain = [0u8; 8];
        let mut pkbuf = [0u8; 8];
        let mut phrase = text.as_slice();
        loop {
            for i in 0..8 {
                let ch = phrase.first().copied().unwrap_or(0);
                plain[i] = pkbuf[i] ^ (ch << 1);
                keybuf[i] = (pkbuf[i] >> 1) ^ ch;
                if !phrase.is_empty() {
                    phrase = &phrase[1..];
                }
            }
            if phrase.is_empty() {
                break;
            }
            let encoded = des_crypt_with_salt(&keybuf, &[], 1, &plain);
            pkbuf = decode_hash_bytes(&encoded);
        }
        let encoded = des_crypt_with_salt(&keybuf, &[b'r', b'a', b's', b'm'], 725, &[0u8; 8]);
        assert_eq!(encoded, "BYk8r9AiWNc");
    }

    #[test]
    fn extended_short_password() {
        // perl: crypt("rasmusle", "_J9..rasm") = _J9..rasmqYJvZGUjATQ (без фолдинга)
        // ключ "rasmusle" напрямую (bits 6..0)
        let encoded = des_crypt_with_salt(b"rasmusle", &[b'r', b'a', b's', b'm'], 725, &[0u8; 8]);
        assert_eq!(encoded, "qYJvZGUjATQ");
        // vichan-счёт: '_..A.' = 53248
        let vichan = des_crypt_with_salt(b"rasmusle", &[b'r', b'a', b's', b'm'], 53248, &[0u8; 8]);
        eprintln!("vichan-count short: {vichan}");
        assert_eq!(vichan.len(), 11);
    }

    #[test]
    fn extended_one_fold_block() {
        // perl: crypt("rasmusleX", "_J9..rasm") = _J9..rasmRgre5XRegKI
        // Фолдинг одного блока: "rasmusle" -> pkbuf, затем "X" финализирует ключ.
        let text = to_shift_jis("rasmusleX");
        let mut keybuf = [0u8; 8];
        let mut plain = [0u8; 8];
        let mut pkbuf = [0u8; 8];
        let mut phrase = text.as_slice();
        loop {
            for i in 0..8 {
                let ch = phrase.first().copied().unwrap_or(0);
                plain[i] = pkbuf[i] ^ (ch << 1);
                keybuf[i] = (pkbuf[i] >> 1) ^ ch;
                if !phrase.is_empty() {
                    phrase = &phrase[1..];
                }
            }
            if phrase.is_empty() {
                break;
            }
            let encoded = des_crypt_with_salt(&keybuf, &[], 1, &plain);
            eprintln!("fold step 1 -> {encoded}");
            pkbuf = decode_hash_bytes(&encoded);
        }
        let encoded = des_crypt_with_salt(&keybuf, &[b'r', b'a', b's', b'm'], 725, &[0u8; 8]);
        assert_eq!(encoded, "Rgre5XRegKI");
    }

    #[test]
    fn secure_tripcode_deterministic() {
        let a = secure_tripcode("password", "sitesalt");
        let b = secure_tripcode("password", "sitesalt");
        let c = secure_tripcode("password", "other");
        assert_eq!(a, b);
        assert!(a.is_some());
        assert_ne!(a, c);
        assert_eq!(a.unwrap().len(), 10);
    }

    #[test]
    fn tripcodes_differ_between_classic_and_secure() {
        let classic = classic_tripcode("password").unwrap();
        let secure = secure_tripcode("password", "sitesalt").unwrap();
        assert_ne!(classic, secure);
    }

    #[test]
    fn ip_hash_is_stable_and_hex() {
        let h1 = hash_ip("1.2.3.4", "secret");
        let h2 = hash_ip("1.2.3.4", "secret");
        let h3 = hash_ip("1.2.3.4", "other");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
