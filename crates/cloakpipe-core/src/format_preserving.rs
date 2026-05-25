//! Format-preserving token generators.
//!
//! Generates realistic-looking fake values that preserve the format of the
//! original (phone stays phone-shaped, email stays email-shaped) while
//! containing no real PII.

use crate::EntityCategory;

static FAKE_DOMAINS: &[&str] = &[
    "atlasmail.example",
    "beaconmail.example",
    "cedarmail.example",
    "driftmail.example",
    "embermail.example",
    "fieldmail.example",
    "grovepost.example",
    "harbormail.example",
    "ironpost.example",
    "junipermail.example",
    "keystonemail.example",
    "laurelpost.example",
    "meadowmail.example",
    "northpost.example",
    "oakmail.example",
    "pinepost.example",
    "quartzmail.example",
    "riverpost.example",
    "stonemail.example",
    "timbermail.example",
    "uplandpost.example",
    "vistamail.example",
    "westpost.example",
    "yarrowmail.example",
    "zenithmail.example",
    "aldermail.example",
    "birchpost.example",
    "canyonmail.example",
    "dovemail.example",
    "eastonpost.example",
    "foxmail.example",
    "glenmail.example",
    "heathpost.example",
    "irismail.example",
    "jasperpost.example",
    "kingsmail.example",
    "lindenpost.example",
    "mapletonmail.example",
    "newpost.example",
    "orchardmail.example",
    "prescottpost.example",
    "quarrymail.example",
    "redmail.example",
    "silverpost.example",
    "thistlemail.example",
    "unionmail.example",
    "veridianpost.example",
    "windwardmail.example",
    "yorkmail.example",
    "zephyrpost.example",
    "ashmail.example",
    "copperpost.example",
    "edgepost.example",
    "flintmail.example",
    "greenpost.example",
    "hollowmail.example",
    "ivypost.example",
    "kestrelmail.example",
    "lagoonpost.example",
    "marblemail.example",
];

static FAKE_NAMES: &[&str] = &[
    "alex", "harper", "taylor", "morgan", "casey", "riley", "chris", "lee", "dana", "jamie",
    "blair", "cameron", "devon", "ellis", "finley", "gray", "hayden", "indigo", "kai", "logan",
    "marlow", "noel", "parker", "quinn", "reese", "sage", "shawn", "skyler", "teagan", "val",
    "winter", "arden", "ashton", "brady", "carter", "drew", "emerson", "frankie", "greer",
    "hollis", "ira", "jules", "kendall", "lane", "micah", "nikki", "oakley", "peyton", "remy",
    "robin", "sawyer", "terry", "uma", "vaughn", "wren", "yael", "zion", "bellamy", "elliot",
    "phoenix", "rowan", "selby", "tatum", "averyn", "briar", "darcy", "eden", "lennox", "marin",
    "sloan",
];

static FAKE_SURNAMES: &[&str] = &[
    "miller", "wilson", "moore", "taylor", "anderson", "hall", "young", "king", "wright", "clark",
    "adams", "baker", "brooks", "carter", "cooper", "davis", "edwards", "fisher", "foster",
    "garcia", "green", "griffin", "harris", "hayes", "hill", "hughes", "jenkins", "kelly", "lewis",
    "long", "martin", "mitchell", "murphy", "nelson", "parker", "price", "reed", "rivera",
    "roberts", "ross", "sanders", "scott", "stewart", "turner", "walker", "ward", "watson",
    "white", "wood", "bennett", "bell", "coleman", "diaz", "evans", "flores", "gray", "howard",
    "james", "powell", "simmons", "thomas",
];

static FAKE_ORG_ROOTS: &[&str] = &[
    "Summit Valley",
    "Harbor Point",
    "Pioneer Grove",
    "Evergreen Ridge",
    "Clearwater Lane",
    "Atlas Grove",
    "Beacon Hill",
    "Crestview Harbor",
    "Driftwood Point",
    "Elmstone Valley",
    "Granite Bay",
    "Highland Park",
    "Ironwood Trail",
    "Juniper Field",
    "Keystone Lake",
    "Laurel Bridge",
    "Meadowbrook Heights",
    "Norwood Grove",
    "Oakmont Springs",
    "Pinecrest Shore",
    "Quartz Meadow",
    "Riverbend Terrace",
    "Stonegate Valley",
    "Timberline Point",
    "Union Crest",
    "Vista Haven",
    "Westbridge Grove",
    "Yarrow Ridge",
    "Zenith Harbor",
    "Alder Point",
    "Birch Valley",
    "Canyon Grove",
    "Dovetail Ridge",
    "Easton Harbor",
    "Foxglove Point",
    "Glenhaven Crest",
    "Hawthorne Valley",
    "Iris Grove",
    "Jasper Ridge",
    "Kingsley Point",
    "Linden Harbor",
    "Mapleton Grove",
    "Newbridge Crest",
    "Orchard Valley",
    "Prescott Ridge",
    "Quarry Point",
    "Redstone Grove",
    "Silverlake Harbor",
    "Thistle Valley",
    "Upland Crest",
    "Veridian Grove",
    "Yorkfield Ridge",
    "Zephyr Valley",
    "Ashford Grove",
    "Bristlecone Point",
    "Copperfield Ridge",
    "Dunhill Harbor",
    "Edgewater Valley",
    "Flint Ridge",
    "Greenfield Point",
];

static FAKE_STREET_NAMES: &[&str] = &[
    "Maple Ridge",
    "Pine Hollow",
    "Oak Meadow",
    "Alder Brook",
    "Birch Hollow",
    "Canyon Crest",
    "Dove Field",
    "Elm Grove",
    "Fern Valley",
    "Granite Hill",
    "Harbor View",
    "Iris Glen",
    "Juniper Bend",
    "Keystone Park",
    "Laurel Springs",
    "Meadow View",
    "Norwood Bend",
    "Orchard Hill",
    "Parker Glen",
    "Quartz Field",
    "River Stone",
    "Summit Brook",
    "Timber Glen",
    "Union Meadow",
    "Vista Park",
    "Westfield Brook",
    "Yarrow Bend",
    "Zephyr Hill",
    "Aspen Field",
    "Briar Meadow",
    "Clover Park",
    "Dunhill View",
    "Easton Grove",
    "Fulton Ridge",
    "Gable Brook",
    "Haven Crest",
    "Ivory Bend",
    "Jasper Meadow",
    "Kingsley Grove",
    "Linden Park",
    "Marble Ridge",
    "Newport Field",
    "Olive Brook",
    "Pebble View",
    "Quail Ridge",
    "Redstone Hill",
    "Silver Field",
    "Thorn Valley",
    "Upland Grove",
    "Veridian Park",
    "Windward Brook",
    "Yorkfield Meadow",
    "Amber Grove",
    "Beacon Ridge",
    "Copper Glen",
    "Driftwood Park",
    "Ember Hill",
    "Foxglove Bend",
    "Glenhaven View",
    "Hearthstone Field",
];

static FAKE_STATES: &[&str] = &[
    "Alabama",
    "Alaska",
    "Arizona",
    "Arkansas",
    "California",
    "Colorado",
    "Connecticut",
    "Delaware",
    "Florida",
    "Georgia",
    "Hawaii",
    "Idaho",
    "Illinois",
    "Indiana",
    "Iowa",
    "Kansas",
    "Kentucky",
    "Louisiana",
    "Maine",
    "Maryland",
    "Massachusetts",
    "Michigan",
    "Minnesota",
    "Mississippi",
    "Missouri",
    "Montana",
    "Nebraska",
    "Nevada",
    "New Hampshire",
    "New Jersey",
    "New Mexico",
    "New York",
    "North Carolina",
    "North Dakota",
    "Ohio",
    "Oklahoma",
    "Oregon",
    "Pennsylvania",
    "Rhode Island",
    "South Carolina",
    "South Dakota",
    "Tennessee",
    "Texas",
    "Utah",
    "Vermont",
    "Virginia",
    "Washington",
    "West Virginia",
    "Wisconsin",
    "Wyoming",
];

static FAKE_PLACES: &[&str] = &[
    "Riverton",
    "Ashland",
    "Brookfield",
    "Franklin",
    "Lakeside",
    "Georgetown",
    "Alderport",
    "Bayfield",
    "Crestmont",
    "Dunwich",
    "Eastvale",
    "Fairmont",
    "Glenbrook",
    "Hartwell",
    "Ironton",
    "Jasperton",
    "Kingsport",
    "Lakewood",
    "Millbrook",
    "Newhaven",
    "Oakdale",
    "Pinehurst",
    "Quarryville",
    "Redford",
    "Silverton",
    "Timberlake",
    "Unionville",
    "Valewood",
    "Westport",
    "Yorktown",
    "Zephyrhills",
    "Arborfield",
    "Briarcliff",
    "Copperton",
    "Driftwood",
    "Emberton",
    "Foxborough",
    "Graniteville",
    "Harborview",
    "Ivydale",
    "Juniper",
    "Keystone",
    "Larkspur",
    "Meadowvale",
    "Norcross",
    "Orchardview",
    "Prescott",
    "Quailwood",
    "Rockport",
    "Stonehaven",
    "Thornfield",
    "Upland",
    "Veridian",
    "Windcrest",
    "Yarrow",
    "Ashbourne",
    "Bellview",
    "Clearport",
    "Doverton",
    "Elmhurst",
];

static FAKE_CARD_ISSUERS: &[&str] = &[
    "Atlas Card Services",
    "Beacon Credit",
    "Cedar Payment Network",
    "Driftwood Bankcard",
    "Ember Financial",
    "Fieldstone Card",
    "GrovePay",
    "Harbor Credit",
    "Ironwood Card",
    "Juniper Payments",
    "Keystone Credit",
    "Laurel Card Services",
    "MeadowPay",
    "Northstar Credit",
    "Oakmont Card",
    "Pinecrest Payments",
    "Quartz Credit",
    "Riverbend Card",
    "Stonegate Payments",
    "Timberline Credit",
    "Union Card Services",
    "VistaPay",
    "Westbridge Credit",
    "Yarrow Card",
    "Zenith Payments",
    "Alder Credit",
    "Birch Bankcard",
    "Canyon Payments",
    "Dovetail Credit",
    "Easton Card",
    "Foxglove Pay",
    "Glenhaven Credit",
    "Hawthorne Card",
    "Iris Payments",
    "Jasper Credit",
    "Kingsley Card",
    "LindenPay",
    "Mapleton Credit",
    "Newbridge Card",
    "Orchard Payments",
    "Prescott Credit",
    "Quarry Card",
    "Redstone Pay",
    "Silverlake Credit",
    "Thistle Card",
    "Upland Payments",
    "Veridian Credit",
    "Windward Card",
    "Yorkfield Pay",
    "Zephyr Credit",
    "Ashford Card",
    "Copperfield Payments",
    "Dunhill Credit",
    "Edgewater Card",
    "FlintPay",
    "Greenfield Credit",
    "Hearthstone Card",
    "Ivory Payments",
    "Kestrel Credit",
    "Lagoon Card",
];

static FAKE_DEMOGRAPHIC_VALUES: &[&str] = &[
    "undisclosed",
    "not specified",
    "private",
    "withheld",
    "unreported",
    "not provided",
    "confidential",
    "declined",
    "unspecified",
    "not listed",
    "masked",
    "redacted",
    "restricted",
    "patient declined",
    "self described",
    "not recorded",
    "data withheld",
    "privacy requested",
    "unavailable",
    "unknown",
    "not collected",
    "deferred",
    "protected",
    "suppressed",
    "not disclosed",
    "record sealed",
    "intentionally blank",
    "clinician withheld",
    "administrative hold",
    "not answered",
    "pending update",
    "masked value",
    "privacy masked",
    "secure value",
    "hidden",
    "not stated",
    "prefer not to say",
    "field withheld",
    "registry private",
    "patient private",
    "internal use only",
    "restricted field",
    "sensitive field",
    "confidential field",
    "masked field",
    "privacy field",
    "not available",
    "review required",
    "secured",
    "omitted",
    "protected field",
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
        EntityCategory::Person => fake_person(original, id),
        EntityCategory::Organization => fake_organization(original, id),
        EntityCategory::Location => fake_location(original, id),
        EntityCategory::Date => fake_similar_date(original, id),
        EntityCategory::Percentage => fake_percentage(original, id),
        EntityCategory::Amount => fake_amount(original, id),
        EntityCategory::PhoneNumber => fake_similar_phone(original, id),
        EntityCategory::Email => fake_similar_email(original, id),
        EntityCategory::IpAddress => fake_similar_ip(id),
        EntityCategory::Secret => fake_similar_secret(original, id),
        EntityCategory::Custom(name) => match name.to_uppercase().as_str() {
            "SSN" | "SOCIAL_SECURITY_NUMBER" => fake_ssn(id),
            "CREDIT_CARD" | "CREDIT_CARD_NUMBER" | "PAYMENT_CARD" => fake_credit_card(original, id),
            "ID_NUMBER" | "LICENSE_NUMBER" | "NPI" | "MRN" | "DEA" => {
                fake_digits_preserving_literals(original, id)
            }
            "IBAN" => fake_iban(original, id),
            "ROUTING_NUMBER" | "ABA_ROUTING_NUMBER" => {
                fake_digits_preserving_literals(original, id)
            }
            "SWIFT_CODE" | "BIC" => fake_swift_code(original, id),
            "ISIN" => fake_isin(original, id),
            "ACCOUNT_NUMBER" | "BANK_ACCOUNT" | "ACCOUNTNAME" => fake_account_number(original, id),
            "USERNAME" => fake_username(original, id),
            "DEVICE_ID" | "PHONEIMEI" | "IMEI" => fake_imei(original, id),
            "PIN" => fake_digits_preserving_literals(original, id),
            "CARD_VERIFICATION_CODE" | "CREDIT_CARD_CVV" | "CVV" | "CVC" => {
                fake_digits_preserving_literals(original, id)
            }
            "CREDIT_CARD_ISSUER" => fake_card_issuer(original, id),
            "AGE" => fake_age(original, id),
            "GENDER" | "SEX" => fake_demographic_word(original, id),
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

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
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

fn fake_person(original: &str, id: u32) -> String {
    let trimmed = original.trim();
    let (prefix, name) = trimmed
        .strip_prefix("Dr. ")
        .map(|name| ("Dr. ", name))
        .unwrap_or(("", trimmed));
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.is_empty() {
        return format!(
            "{}{}",
            prefix,
            title_case(FAKE_NAMES[seeded_index(id, 0, FAKE_NAMES.len())])
        );
    }

    let first = title_case(FAKE_NAMES[seeded_index(id, 0, FAKE_NAMES.len())]);
    let last = title_case(FAKE_SURNAMES[seeded_index(id, 1, FAKE_SURNAMES.len())]);
    let fake = match words.len() {
        1 => first,
        2 => format!("{first} {last}"),
        _ => {
            let mut parts = vec![first, last];
            parts.extend(words.iter().skip(2).enumerate().map(|(idx, word)| {
                if word.chars().all(|c| c.is_ascii_uppercase()) {
                    (*word).to_string()
                } else {
                    title_case(FAKE_NAMES[seeded_index(id, idx + 2, FAKE_NAMES.len())])
                }
            }));
            parts.join(" ")
        }
    };

    ensure_changed(trimmed, format!("{prefix}{fake}"), id)
}

fn fake_organization(original: &str, id: u32) -> String {
    let suffix = organization_suffix(original).unwrap_or("Group");
    ensure_changed(
        original.trim(),
        format!(
            "{} {suffix}",
            FAKE_ORG_ROOTS[seeded_index(id, 2, FAKE_ORG_ROOTS.len())]
        ),
        id,
    )
}

fn organization_suffix(original: &str) -> Option<&'static str> {
    let upper = original.to_ascii_uppercase();
    if upper.contains("HEALTH") {
        Some("Health")
    } else if upper.contains("INSURANCE") {
        Some("Insurance")
    } else if upper.contains("MEDICINE") {
        Some("Medicine")
    } else if upper.contains("CLINIC") {
        Some("Clinic")
    } else if upper.contains("HOSPITAL") {
        Some("Hospital")
    } else {
        None
    }
}

fn fake_location(original: &str, id: u32) -> String {
    let trimmed = original.trim();
    if trimmed.chars().all(|c| c.is_ascii_digit()) && trimmed.len() == 5 {
        return format!("{:05}", 10000 + (id * 7919 % 89999));
    }

    if trimmed.to_ascii_lowercase().starts_with("apt ") {
        let unit = 1 + (id * 7 % 89);
        let letter = seeded_char(id, 4, b"ABCDEFGHJKLMNPQRSTUVWXYZ");
        return format!("Apt {unit}{letter}");
    }

    if trimmed.chars().any(|c| c.is_ascii_digit()) {
        let suffix = street_suffix(trimmed).unwrap_or("Street");
        let number = 1000 + (id * 137 % 8999);
        return ensure_changed(
            trimmed,
            format!(
                "{number} {} {suffix}",
                FAKE_STREET_NAMES[seeded_index(id, 3, FAKE_STREET_NAMES.len())]
            ),
            id,
        );
    }

    if is_us_state(trimmed) {
        return ensure_changed(
            trimmed,
            FAKE_STATES[seeded_index(id, 5, FAKE_STATES.len())].to_string(),
            id,
        );
    }

    ensure_changed(
        trimmed,
        FAKE_PLACES[seeded_index(id, 6, FAKE_PLACES.len())].to_string(),
        id,
    )
}

fn street_suffix(original: &str) -> Option<&'static str> {
    let upper = original.to_ascii_uppercase();
    if upper.ends_with(" DRIVE") {
        Some("Drive")
    } else if upper.ends_with(" ROAD") {
        Some("Road")
    } else if upper.ends_with(" AVENUE") {
        Some("Avenue")
    } else if upper.ends_with(" STREET") {
        Some("Street")
    } else if upper.ends_with(" LANE") {
        Some("Lane")
    } else {
        None
    }
}

fn is_us_state(value: &str) -> bool {
    matches!(
        value,
        "Alabama"
            | "Alaska"
            | "Arizona"
            | "Arkansas"
            | "California"
            | "Colorado"
            | "Connecticut"
            | "Delaware"
            | "Florida"
            | "Georgia"
            | "Hawaii"
            | "Idaho"
            | "Illinois"
            | "Indiana"
            | "Iowa"
            | "Kansas"
            | "Kentucky"
            | "Louisiana"
            | "Maine"
            | "Maryland"
            | "Massachusetts"
            | "Michigan"
            | "Minnesota"
            | "Mississippi"
            | "Missouri"
            | "Montana"
            | "Nebraska"
            | "Nevada"
            | "New Hampshire"
            | "New Jersey"
            | "New Mexico"
            | "New York"
            | "North Carolina"
            | "North Dakota"
            | "Ohio"
            | "Oklahoma"
            | "Oregon"
            | "Pennsylvania"
            | "Rhode Island"
            | "South Carolina"
            | "South Dakota"
            | "Tennessee"
            | "Texas"
            | "Utah"
            | "Vermont"
            | "Virginia"
            | "Washington"
            | "West Virginia"
            | "Wisconsin"
            | "Wyoming"
    )
}

fn fake_similar_date(original: &str, id: u32) -> String {
    let trimmed = original.trim();
    if let Some(fake) = fake_iso_date(trimmed, id) {
        return fake;
    }
    if let Some(fake) = fake_slash_date(trimmed, id) {
        return fake;
    }
    if let Some(fake) = fake_month_date(trimmed, id) {
        return fake;
    }
    if let Some(fake) = fake_fiscal_period(trimmed, id) {
        return fake;
    }
    let year = 2024 + (id * 3 % 8);
    let month = 1 + (id * 5 % 12);
    let day = 1 + (id * 7 % 28);
    format!("{year:04}-{month:02}-{day:02}")
}

fn fake_iso_date(original: &str, id: u32) -> Option<String> {
    let parts: Vec<&str> = original.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    let year = 1980 + (id * 3 % 45);
    let month = 1 + (id * 5 % 12);
    let day = 1 + (id * 7 % 28);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn fake_slash_date(original: &str, id: u32) -> Option<String> {
    let parts: Vec<&str> = original.split('/').collect();
    if parts.len() != 3
        || !parts
            .iter()
            .all(|part| part.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let month = 1 + (id * 5 % 12);
    let day = 1 + (id * 7 % 28);
    let year = 2024 + (id * 3 % 8);
    Some(format!("{month:02}/{day:02}/{year:04}"))
}

fn fake_month_date(original: &str, id: u32) -> Option<String> {
    let months = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let original_month = months.iter().find(|month| original.starts_with(**month))?;
    let month_idx = (months
        .iter()
        .position(|month| month == original_month)
        .unwrap_or(0)
        + id as usize)
        % months.len();
    let day = 1 + (id * 7 % 28);
    let year = 2024 + (id * 3 % 8);
    Some(format!("{} {day}, {year}", months[month_idx]))
}

fn fake_fiscal_period(original: &str, id: u32) -> Option<String> {
    if let Some(year) = original.strip_prefix("FY") {
        if year.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("FY{}", 2024 + (id * 3 % 8)));
        }
    }

    let mut parts = original.split_whitespace();
    let quarter = parts.next()?;
    let year = parts.next()?;
    if !matches!(quarter, "Q1" | "Q2" | "Q3" | "Q4") || !year.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("Q{} {}", 1 + (id % 4), 2024 + (id * 3 % 8)))
}

fn fake_percentage(original: &str, id: u32) -> String {
    let value = 5 + (id * 13 % 90);
    if original.contains('.') {
        format!("{value}.{}%", id * 7 % 10)
    } else {
        format!("{value}%")
    }
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

fn fake_digits_preserving_literals(original: &str, id: u32) -> String {
    let mut digit_offset = 0usize;
    ensure_changed(
        original,
        original
            .chars()
            .map(|c| {
                if c.is_ascii_digit() {
                    let digit = seeded_digit(id, digit_offset);
                    digit_offset += 1;
                    digit
                } else {
                    c
                }
            })
            .collect(),
        id,
    )
}

fn fake_iban(original: &str, id: u32) -> String {
    let mut alnum_offset = 0usize;
    ensure_changed(
        original,
        original
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    let fake = if alnum_offset < 2 && c.is_ascii_alphabetic() {
                        c
                    } else {
                        fake_like_char(c, id, alnum_offset)
                    };
                    alnum_offset += 1;
                    fake
                } else {
                    c
                }
            })
            .collect(),
        id,
    )
}

fn fake_swift_code(original: &str, id: u32) -> String {
    let mut offset = 0usize;
    ensure_changed(
        original,
        original
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    let fake = fake_like_char(c, id, offset);
                    offset += 1;
                    fake
                } else {
                    c
                }
            })
            .collect(),
        id,
    )
}

fn fake_isin(original: &str, id: u32) -> String {
    let mut offset = 0usize;
    ensure_changed(
        original,
        original
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    let fake = if offset < 2 && c.is_ascii_alphabetic() {
                        c
                    } else {
                        fake_like_char(c, id, offset)
                    };
                    offset += 1;
                    fake
                } else {
                    c
                }
            })
            .collect(),
        id,
    )
}

fn fake_account_number(original: &str, id: u32) -> String {
    if original.chars().any(|c| c.is_ascii_digit()) {
        return fake_digits_preserving_literals(original, id);
    }
    fake_username(original, id)
}

fn fake_username(original: &str, id: u32) -> String {
    let name = FAKE_NAMES[seeded_index(id, 3, FAKE_NAMES.len())];
    let surname = FAKE_SURNAMES[seeded_index(id, 4, FAKE_SURNAMES.len())];
    let digit_count = original
        .chars()
        .filter(|c| c.is_ascii_digit())
        .count()
        .min(6);
    let digits: String = (0..digit_count).map(|i| seeded_digit(id, i + 5)).collect();
    let separator = if original.contains('_') { '_' } else { '.' };
    ensure_changed(original, format!("{name}{separator}{surname}{digits}"), id)
}

fn fake_imei(original: &str, id: u32) -> String {
    let digits: Vec<char> = original.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 15 {
        return fake_digits_preserving_literals(original, id);
    }

    let mut fake_digits: Vec<char> = (0..15).map(|i| seeded_digit(id, i + 9)).collect();
    fake_digits[14] = luhn_check_digit(&fake_digits[..14]);
    apply_digit_format(original, &fake_digits)
}

fn fake_card_issuer(original: &str, id: u32) -> String {
    let mut fake = FAKE_CARD_ISSUERS[seeded_index(id, 6, FAKE_CARD_ISSUERS.len())].to_string();
    if fake.eq_ignore_ascii_case(original.trim()) {
        fake = FAKE_CARD_ISSUERS
            [(seeded_index(id, 6, FAKE_CARD_ISSUERS.len()) + 1) % FAKE_CARD_ISSUERS.len()]
        .to_string();
    }
    fake
}

fn fake_age(original: &str, id: u32) -> String {
    if original.chars().all(|c| c.is_ascii_digit()) {
        return format!("{}", 21 + (id * 7 % 58));
    }
    fake_digits_preserving_literals(original, id)
}

fn fake_demographic_word(original: &str, id: u32) -> String {
    let mut fake =
        FAKE_DEMOGRAPHIC_VALUES[seeded_index(id, 7, FAKE_DEMOGRAPHIC_VALUES.len())].to_string();
    if fake.eq_ignore_ascii_case(original.trim()) {
        fake = FAKE_DEMOGRAPHIC_VALUES[(seeded_index(id, 7, FAKE_DEMOGRAPHIC_VALUES.len()) + 1)
            % FAKE_DEMOGRAPHIC_VALUES.len()]
        .to_string();
    }
    fake
}

fn ensure_changed(original: &str, mut fake: String, id: u32) -> String {
    if fake != original {
        return fake;
    }

    if let Some((idx, c)) = fake.char_indices().find(|(_, c)| c.is_ascii_digit()) {
        let replacement = ((c.to_digit(10).unwrap_or(0) + 1 + id % 8) % 10).to_string();
        fake.replace_range(idx..idx + c.len_utf8(), &replacement);
    } else if let Some((idx, c)) = fake.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
        let replacement = if c.is_ascii_uppercase() { 'X' } else { 'x' }.to_string();
        fake.replace_range(idx..idx + c.len_utf8(), &replacement);
    }
    fake
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
    fn fake_data_dictionaries_have_broad_variants() {
        for (name, len) in [
            ("domains", FAKE_DOMAINS.len()),
            ("first names", FAKE_NAMES.len()),
            ("last names", FAKE_SURNAMES.len()),
            ("organization roots", FAKE_ORG_ROOTS.len()),
            ("street names", FAKE_STREET_NAMES.len()),
            ("states", FAKE_STATES.len()),
            ("places", FAKE_PLACES.len()),
            ("card issuers", FAKE_CARD_ISSUERS.len()),
            ("demographic values", FAKE_DEMOGRAPHIC_VALUES.len()),
        ] {
            assert!(len >= 50, "{name} should have at least 50 variants");
        }
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
    fn generate_similar_returns_plausible_builtin_values() {
        let cases = [
            (
                "Avery Collins",
                EntityCategory::Person,
                r"^[A-Z][a-z]+ [A-Z][a-z]+$",
            ),
            (
                "Northwind Community Health",
                EntityCategory::Organization,
                r"^[A-Z][A-Za-z]+ [A-Z][A-Za-z]+ Health$",
            ),
            (
                "1842 Willow Creek Drive",
                EntityCategory::Location,
                r"^\d{4} [A-Z][a-z]+ [A-Z][a-z]+ Drive$",
            ),
            ("1984-07-16", EntityCategory::Date, r"^\d{4}-\d{2}-\d{2}$"),
            (
                "July 16, 1984",
                EntityCategory::Date,
                r"^[A-Z][a-z]+ \d{1,2}, \d{4}$",
            ),
            ("20%", EntityCategory::Percentage, r"^\d{1,2}%$"),
        ];

        for (original, category, pattern) in cases {
            let fake = generate_similar(original, &category, 1);
            assert_ne!(fake, original);
            assert!(
                regex::Regex::new(pattern).unwrap().is_match(&fake),
                "{fake} should match {pattern}"
            );
            assert!(!fake.contains("User-"));
            assert!(!fake.contains("Org-"));
            assert!(!fake.contains("Location-"));
            assert!(!fake.contains("DATE_"));
            assert!(!fake.contains("PCT-"));
        }
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
    fn generate_similar_returns_plausible_custom_identifiers() {
        let cases = [
            (
                "MRN-2026-443821",
                EntityCategory::Custom("ID_NUMBER".into()),
                r"^MRN-\d{4}-\d{6}$",
            ),
            (
                "CRC-1330841",
                EntityCategory::Custom("LICENSE_NUMBER".into()),
                r"^CRC-\d{7}$",
            ),
            (
                "NPI 1184729934",
                EntityCategory::Custom("LICENSE_NUMBER".into()),
                r"^NPI \d{10}$",
            ),
            (
                "GB82WEST12345698765432",
                EntityCategory::Custom("IBAN".into()),
                r"^GB[A-Z0-9]{20}$",
            ),
            (
                "021000021",
                EntityCategory::Custom("ROUTING_NUMBER".into()),
                r"^\d{9}$",
            ),
            (
                "DEUTDEFF500",
                EntityCategory::Custom("SWIFT_CODE".into()),
                r"^[A-Z0-9]{11}$",
            ),
            (
                "US0378331005",
                EntityCategory::Custom("ISIN".into()),
                r"^US[A-Z0-9]{10}$",
            ),
            (
                "4455667788990011",
                EntityCategory::Custom("ACCOUNT_NUMBER".into()),
                r"^\d{16}$",
            ),
            (
                "avery.collins42",
                EntityCategory::Custom("USERNAME".into()),
                r"^[a-z]+\.[a-z]+\d{2}$",
            ),
            (
                "356938035643809",
                EntityCategory::Custom("DEVICE_ID".into()),
                r"^\d{15}$",
            ),
            ("4821", EntityCategory::Custom("PIN".into()), r"^\d{4}$"),
            (
                "321",
                EntityCategory::Custom("CARD_VERIFICATION_CODE".into()),
                r"^\d{3}$",
            ),
        ];

        for (original, category, pattern) in cases {
            let fake = generate_similar(original, &category, 1);
            assert_ne!(fake, original);
            assert!(
                regex::Regex::new(pattern).unwrap().is_match(&fake),
                "{fake} should match {pattern}"
            );
        }
    }

    #[test]
    fn generate_similar_returns_plausible_card_issuer() {
        let fake = generate_similar(
            "Visa",
            &EntityCategory::Custom("CREDIT_CARD_ISSUER".into()),
            1,
        );

        assert_ne!(fake, "Visa");
        assert!(FAKE_CARD_ISSUERS.contains(&fake.as_str()));
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
