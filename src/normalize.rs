//! Ingredient-name normalisation shared by recipe matching and price lookup.

const ALIASES: &[(&str, &str)] = &[
    ("scallion", "green onion"), ("spring onion", "green onion"), ("cilantro", "coriander"),
    ("bell pepper", "pepper"), ("capsicum", "pepper"), ("garbanzo", "chickpea"), ("garbanzo bean", "chickpea"),
    ("ground beef", "beef"), ("mince", "beef"), ("chicken breast", "chicken"), ("chicken thigh", "chicken"),
    ("olive oil", "oil"), ("vegetable oil", "oil"), ("canola oil", "oil"), ("extra virgin olive oil", "oil"),
    ("all purpose flour", "flour"), ("all-purpose flour", "flour"), ("spaghetti", "pasta"), ("penne", "pasta"),
    ("cheddar cheese", "cheddar"), ("parmesan cheese", "parmesan"),
];

const DESCRIPTORS: &[&str] = &[
    "fresh", "large", "small", "medium", "chopped", "diced", "minced", "sliced", "ripe", "organic", "boneless",
    "skinless", "whole", "frozen", "canned", "dried", "cooked", "grated", "shredded", "lean", "unsalted",
    "salted", "low-sodium", "of", "a", "the",
];

/// Compounds whose head word must not satisfy a plain head-word need ("peanut butter" is not "butter").
const DISTINCT: &[&str] = &[
    "peanut butter", "almond butter", "coconut milk", "almond milk", "oat milk", "soy milk", "sweet potato",
    "cream cheese", "butter bean", "coconut oil", "sesame oil", "ice cream", "sour cream", "green onion",
    "soy sauce", "tomato sauce", "tomato paste", "hot sauce", "fish sauce",
];

fn alias(s: &str) -> Option<&'static str> {
    ALIASES.iter().find(|(k, _)| *k == s).map(|(_, v)| *v)
}

fn singular(w: &str) -> String {
    let n = w.len();
    if n > 3 && w.ends_with("ies") {
        format!("{}y", &w[..n - 3])
    } else if n > 3 && ["oes", "ches", "shes", "sses"].iter().any(|e| w.ends_with(e)) {
        w[..n - 2].to_string()
    } else if n > 3 && w.ends_with('s') && !w.ends_with("ss") {
        w[..n - 1].to_string()
    } else {
        w.to_string()
    }
}

/// Drop balanced "(...)" groups (unbalanced parens are left to be treated as punctuation).
fn strip_parens(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        match rest[open..].find(')') {
            Some(close) => {
                out.push_str(&rest[..open]);
                out.push(' ');
                rest = &rest[open + close + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

pub fn normalize(name: &str) -> String {
    let lowered = strip_parens(&name.to_lowercase());
    let cleaned: String = lowered
        .chars()
        .map(|c| if c.is_ascii_lowercase() || c == '-' { c } else { ' ' })
        .collect();
    let s = cleaned.split_whitespace().map(singular).collect::<Vec<_>>().join(" ");
    if let Some(a) = alias(&s) {
        return a.to_string();
    }
    let s = s
        .split(' ')
        .filter(|w| !w.is_empty() && !DESCRIPTORS.contains(w))
        .collect::<Vec<_>>()
        .join(" ");
    alias(&s).map(str::to_string).unwrap_or(s)
}

/// Does a thing the user has (pantry / list entry) satisfy a recipe ingredient?
pub fn matches(have: &str, need: &str) -> bool {
    let (a, b) = (normalize(have), normalize(need));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    if DISTINCT.contains(&a.as_str()) || DISTINCT.contains(&b.as_str()) {
        return false;
    }
    let (wa, wb): (Vec<_>, Vec<_>) = (a.split(' ').collect(), b.split(' ').collect());
    // "cherry tomato" <-> "tomato": one side generic, same head word.
    wa.last() == wb.last() && (wa.len() == 1 || wb.len() == 1)
}
