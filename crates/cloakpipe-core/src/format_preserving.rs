//! Format-preserving token generators.
//!
//! Generates realistic-looking fake values that preserve the format of the
//! original (phone stays phone-shaped, email stays email-shaped) while
//! containing no real PII.

use crate::EntityCategory;

static FAKE_DOMAINS: &[&str] = &[
    "gmail.com",
    "outlook.com",
    "proton.me",
    "fastmail.com",
    "icloud.com",
    "aol.com",
    "mail.com",
];

static FAKE_NAMES: &[&str] = &[
    "alex", "jordan", "taylor", "morgan", "casey", "riley", "chris", "lee", "dana", "jamie",
];

static FAKE_SURNAMES: &[&str] = &[
    "miller", "wilson", "moore", "taylor", "anderson", "hall", "young", "king", "wright", "clark",
];

/// Generate a format-preserving fake token for the given category and id.
/// The id comes from the vault counter and ensures determinism.
pub fn generate(original: &str, category: &EntityCategory, id: u32) -> String {
    match category {
        EntityCategory::PhoneNumber => fake_phone(original, id),
        EntityCategory::Email => format!("user{:03}@masked.invalid", id),
        EntityCategory::IpAddress => format!("10.{}.{}.1", (id / 256) % 256, id % 256),
        EntityCategory::Amount => fake_amount(original, id),
        EntityCategory::Date => format!("DATE_{:03}", id),
        EntityCategory::Url => format!("https://masked-{:03}.invalid", id),
        EntityCategory::Secret => fake_secret(original, id),
        EntityCategory::Person => format!("User-{:03}", id),
        EntityCategory::Organization => format!("Org-{:03}", id),
        EntityCategory::Location => format!("Location-{:03}", id),
        EntityCategory::Custom(name) => {
            // For custom categories like Aadhaar, PAN, GSTIN, detect by name
            match name.to_uppercase().as_str() {
                "AADHAAR" | "AADHAAR_NUMBER" => fake_aadhaar(id),
                "PAN" | "PAN_CARD" => fake_pan(id),
                "GSTIN" => fake_gstin(id),
                "UPI" | "UPI_ID" => format!("user{:03}@okmasked", id),
                _ => format!("{}-{:03}", name.to_uppercase(), id),
            }
        }
        _ => format!("{}-{:03}", category_prefix(category), id),
    }
}

/// Generate a plausible fake value that preserves the original value's structure.
pub fn generate_similar(original: &str, category: &EntityCategory, id: u32) -> String {
    match category {
        EntityCategory::PhoneNumber => fake_similar_phone(original, id),
        EntityCategory::Email => fake_similar_email(original, id),
        EntityCategory::IpAddress => fake_similar_ip(id),
        EntityCategory::Secret => fake_similar_secret(original, id),
        EntityCategory::Custom(name) => match name.to_uppercase().as_str() {
            "SSN" | "SOCIAL_SECURITY_NUMBER" => fake_ssn(id),
            "CREDIT_CARD" | "CREDIT_CARD_NUMBER" | "PAYMENT_CARD" => fake_credit_card(original, id),
            _ => generate(original, category, id),
        },
        _ => generate(original, category, id),
    }
}

fn category_prefix(category: &EntityCategory) -> &'static str {
    match category {
        EntityCategory::Person => "PERSON",
        EntityCategory::Organization => "ORG",
        EntityCategory::Location => "LOC",
        EntityCategory::Amount => "AMOUNT",
        EntityCategory::Percentage => "PCT",
        EntityCategory::Date => "DATE",
        EntityCategory::Email => "EMAIL",
        EntityCategory::PhoneNumber => "PHONE",
        EntityCategory::IpAddress => "IP",
        EntityCategory::Secret => "SECRET",
        EntityCategory::Url => "URL",
        EntityCategory::Project => "PROJECT",
        EntityCategory::Business => "BUSINESS",
        EntityCategory::Infra => "INFRA",
        EntityCategory::Custom(_) => "CUSTOM",
    }
}

fn seeded_index(id: u32, offset: usize, len: usize) -> usize {
    (id as usize * 31 + offset * 7) % len
}

fn seeded_digit(id: u32, offset: usize) -> char {
    (b'0' + seeded_index(id, offset, 10) as u8) as char
}

fn seeded_char(id: u32, offset: usize, chars: &[u8]) -> char {
    chars[seeded_index(id, offset, chars.len())] as char
}

fn fake_similar_email(original: &str, id: u32) -> String {
    let name = FAKE_NAMES[seeded_index(id, 0, FAKE_NAMES.len())];
    let surname = FAKE_SURNAMES[seeded_index(id, 1, FAKE_SURNAMES.len())];
    let domain = FAKE_DOMAINS[seeded_index(id, 2, FAKE_DOMAINS.len())];
    let digit_count = original
        .split('@')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .count()
        .min(6);
    let digits: String = (0..digit_count).map(|i| seeded_digit(id, i)).collect();
    format!("{name}.{surname}{digits}@{domain}")
}

fn fake_similar_phone(original: &str, id: u32) -> String {
    let digits: Vec<char> = original.chars().filter(|c| c.is_ascii_digit()).collect();
    let mut replacements: Vec<char> = (0..digits.len()).map(|i| seeded_digit(id, i + 1)).collect();

    if original.starts_with('+') && !digits.is_empty() {
        replacements[0] = digits[0];
    }

    apply_digit_format(original, &replacements)
}

fn fake_ssn(id: u32) -> String {
    let area = 100 + (id * 37 % 799);
    let group = 10 + (id * 53 % 89);
    let serial = 1000 + (id * 71 % 8999);
    format!("{area:03}-{group:02}-{serial:04}")
}

fn fake_credit_card(original: &str, id: u32) -> String {
    let original_digits: Vec<char> = original.chars().filter(|c| c.is_ascii_digit()).collect();
    let len = original_digits.len();
    if len == 0 {
        return generate(original, &EntityCategory::Custom("CREDIT_CARD".into()), id);
    }

    let mut digits = Vec::with_capacity(len);
    digits.push(original_digits[0]);
    digits.extend((1..len).map(|i| seeded_digit(id, i)));
    if len >= 2 {
        let check = luhn_check_digit(&digits[..len - 1]);
        digits[len - 1] = check;
    }

    apply_digit_format(original, &digits)
}

fn luhn_check_digit(prefix: &[char]) -> char {
    let mut sum = 0;
    let parity = (prefix.len() + 1) % 2;
    for (idx, digit) in prefix.iter().enumerate() {
        let mut value = digit.to_digit(10).unwrap_or(0);
        if idx % 2 == parity {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
    }
    char::from_digit((10 - (sum % 10)) % 10, 10).unwrap_or('0')
}

fn fake_similar_ip(id: u32) -> String {
    let third = 1 + (id * 71 % 254);
    let fourth = 1 + (id * 97 % 254);
    format!("172.18.{third}.{fourth}")
}

fn fake_similar_secret(original: &str, id: u32) -> String {
    if original.starts_with("AKIA") && original.len() == 20 {
        let suffix: String = (0..16)
            .map(|i| seeded_char(id, i, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"))
            .collect();
        return format!("AKIA{suffix}");
    }

    let prefixes = [
        "github_pat_",
        "ghp_",
        "gho_",
        "ghs_",
        "sk-proj-",
        "sk-live-",
        "sk-test-",
        "sk-prod-",
        "sk-",
        "Bearer ",
        "glpat-",
    ];
    let prefix = prefixes
        .iter()
        .find(|prefix| original.starts_with(**prefix))
        .copied()
        .unwrap_or_default();
    let suffix: String = original[prefix.len()..]
        .chars()
        .enumerate()
        .map(|(i, c)| fake_like_char(c, id, i))
        .collect();
    format!("{prefix}{suffix}")
}

fn fake_like_char(c: char, id: u32, offset: usize) -> char {
    match c {
        'a'..='z' => seeded_char(id, offset, b"abcdefghijklmnopqrstuvwxyz"),
        'A'..='Z' => seeded_char(id, offset, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
        '0'..='9' => seeded_digit(id, offset),
        '_' | '-' | '.' => c,
        _ => seeded_char(
            id,
            offset,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
        ),
    }
}

fn apply_digit_format(original: &str, digits: &[char]) -> String {
    let mut next_digit = digits.iter();
    original
        .chars()
        .map(|c| {
            if c.is_ascii_digit() {
                *next_digit.next().unwrap_or(&c)
            } else {
                c
            }
        })
        .collect()
}

fn fake_phone(original: &str, id: u32) -> String {
    let n = id as u64;
    if original.starts_with("+91") || original.contains("91 ") {
        // India format: +91 XXXXX XXXXX
        let a = 55500 + (n % 99999);
        let b = 10000 + (n * 7 % 89999);
        format!("+91 {:05} {:05}", a, b)
    } else if original.starts_with("+1") || original.starts_with("1-") {
        // US format: +1-555-XXX-XXXX
        let a = 100 + (n % 899);
        let b = 1000 + (n * 3 % 8999);
        format!("+1-555-{:03}-{:04}", a, b)
    } else {
        // Generic
        let a = 5_550_000_000_u64 + (n * 13 % 9_999_999);
        format!("{:010}", a)
    }
}

fn fake_aadhaar(id: u32) -> String {
    let n = id as u64;
    let a = 5555 + (n % 4444);
    let b = 1000 + (n * 7 % 8999);
    let c = 1000 + (n * 13 % 8999);
    format!("{:04} {:04} {:04}", a, b, c)
}

fn fake_pan(id: u32) -> String {
    // PAN format: AAAAA9999A (5 letters, 4 digits, 1 letter)
    let letters = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
    let n = id as usize;
    let l = |i: usize| letters[(n + i * 7) % letters.len()] as char;
    format!(
        "{}{}{}{}{}{}{}{}{}{}",
        l(0),
        l(1),
        l(2),
        l(3),
        l(4),
        (id % 10),
        (id / 10 % 10),
        (id / 100 % 10),
        (id / 1000 % 10),
        l(5)
    )
}

fn fake_gstin(id: u32) -> String {
    // GSTIN: 2 digits + 10 PAN chars + 1 digit + 1 char + 1 char
    let pan = fake_pan(id);
    format!("{:02}{}1ZV", (id % 36) + 1, pan)
}

fn fake_amount(original: &str, id: u32) -> String {
    let n = (id as u64) * 1234 + 100;
    if original.contains('₹') || original.to_lowercase().contains("inr") {
        format!("₹{}", n)
    } else if original.contains('$') {
        format!("${}.{:02}", n / 100, n % 100)
    } else if original.contains('€') {
        format!("€{}.{:02}", n / 100, n % 100)
    } else if original.contains('£') {
        format!("£{}.{:02}", n / 100, n % 100)
    } else {
        format!("{}.{:02}", n / 100, n % 100)
    }
}

fn fake_secret(original: &str, id: u32) -> String {
    // Preserve the prefix (sk-, AKIA, etc.) if recognizable, mask the rest
    if let Some(prefix) = ["sk-", "pk-", "AKIA", "Bearer ", "ghp_", "glpat-"]
        .iter()
        .find(|p| original.starts_with(*p))
    {
        let suffix_len = original.len().saturating_sub(prefix.len()).min(20);
        let suffix: String = (0..suffix_len)
            .map(|i| {
                let c = b"ABCDEFGHJKLMNPQRSTUVWXYZ0123456789";
                c[(id as usize + i * 7) % c.len()] as char
            })
            .collect();
        format!("{}{}", prefix, suffix)
    } else {
        format!("MASKED-SECRET-{:04}", id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityCategory;

    #[test]
    fn test_india_phone_format() {
        let fake = generate("+91 98765 43210", &EntityCategory::PhoneNumber, 1);
        assert!(
            fake.starts_with("+91 "),
            "India phone should start with +91"
        );
        assert_ne!(fake, "+91 98765 43210", "Should not return original");
    }

    #[test]
    fn test_aadhaar_format() {
        let fake = generate(
            "2345 6789 0123",
            &EntityCategory::Custom("Aadhaar".into()),
            1,
        );
        // Should be XXXX XXXX XXXX format
        let parts: Vec<&str> = fake.split_whitespace().collect();
        assert_eq!(parts.len(), 3);
        assert!(parts
            .iter()
            .all(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_digit())));
    }

    #[test]
    fn test_pan_format() {
        let fake = generate("BNZPM2501F", &EntityCategory::Custom("PAN".into()), 1);
        assert_eq!(fake.len(), 10, "PAN should be 10 chars");
    }

    #[test]
    fn test_email_format() {
        let fake = generate("priya@example.com", &EntityCategory::Email, 5);
        assert!(fake.contains('@'), "Email fake should contain @");
        assert!(fake.ends_with(".invalid"), "Masked emails use .invalid TLD");
    }

    #[test]
    fn test_determinism() {
        // Same id → same output
        let a = generate("test@example.com", &EntityCategory::Email, 3);
        let b = generate("other@example.com", &EntityCategory::Email, 3);
        assert_eq!(a, b, "Same id should produce same format-preserving token");
    }

    #[test]
    fn test_different_ids_differ() {
        let a = generate("test@example.com", &EntityCategory::Email, 1);
        let b = generate("test@example.com", &EntityCategory::Email, 2);
        assert_ne!(a, b, "Different ids should produce different tokens");
    }

    #[test]
    fn generate_similar_returns_plausible_email() {
        let fake = generate_similar("lee.taylor56789@aol.com", &EntityCategory::Email, 1);
        assert!(fake.contains('@'), "Similar email should contain @");
        assert_ne!(fake, "lee.taylor56789@aol.com");
    }

    #[test]
    fn generate_similar_returns_plausible_phone() {
        let fake = generate_similar("+1-501-369-6183", &EntityCategory::PhoneNumber, 1);
        assert!(
            fake.starts_with("+1-"),
            "Similar phone should preserve country format"
        );
        assert_ne!(fake, "+1-501-369-6183");
    }

    #[test]
    fn generate_similar_returns_plausible_ssn() {
        let fake = generate_similar("927-83-6041", &EntityCategory::Custom("SSN".into()), 1);
        assert_eq!(fake.len(), "927-83-6041".len());
        assert_ne!(fake, "927-83-6041");
    }

    #[test]
    fn generate_similar_returns_plausible_credit_card() {
        let fake = generate_similar(
            "4890 1234 5678 9012",
            &EntityCategory::Custom("CREDIT_CARD".into()),
            1,
        );
        assert_eq!(fake.len(), "4890 1234 5678 9012".len());
        assert!(
            fake.starts_with('4'),
            "Similar card should preserve card type"
        );
        assert_ne!(fake, "4890 1234 5678 9012");
    }

    #[test]
    fn generate_similar_returns_plausible_ip_address() {
        let fake = generate_similar("10.0.1.42", &EntityCategory::IpAddress, 1);
        let octets: Vec<&str> = fake.split('.').collect();
        assert_eq!(octets.len(), 4);
        assert_ne!(fake, "10.0.1.42");
    }

    #[test]
    fn generate_similar_returns_plausible_aws_key() {
        let fake = generate_similar("AKIAQX4BIPW3AHOV29GN", &EntityCategory::Secret, 1);
        assert!(
            fake.starts_with("AKIA"),
            "Similar AWS key should preserve prefix"
        );
        assert_eq!(fake.len(), "AKIAQX4BIPW3AHOV29GN".len());
        assert_ne!(fake, "AKIAQX4BIPW3AHOV29GN");
    }

    #[test]
    fn generate_similar_returns_plausible_github_token() {
        let fake = generate_similar("ghp_abc123secrettoken", &EntityCategory::Secret, 1);
        assert!(
            fake.starts_with("ghp_"),
            "Similar GitHub token should preserve prefix"
        );
        assert_eq!(fake.len(), "ghp_abc123secrettoken".len());
        assert_ne!(fake, "ghp_abc123secrettoken");
    }
}
