use crate::normalize::{matches, normalize};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::LazyLock};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Ingredient {
    pub name: String,
    #[serde(default)]
    pub qty: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub staple: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Recipe {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub minutes: u32,
    #[serde(default = "one")]
    pub servings: u32,
    #[serde(default)]
    pub tags: Vec<String>,
    pub ingredients: Vec<Ingredient>,
    #[serde(default)]
    pub steps: Vec<String>,
}
fn one() -> u32 { 1 }

pub static RECIPES: LazyLock<Vec<Recipe>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../data/recipes.json")).expect("data/recipes.json is valid"));

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status { Pantry, Shopping, Missing }

#[derive(Serialize, Clone, Debug)]
pub struct IngredientStatus { pub name: String, pub status: Status }

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MatchedRecipe {
    pub id: String,
    pub title: String,
    pub minutes: u32,
    pub servings: u32,
    pub tags: Vec<String>,
    pub coverage: f64,
    pub ingredients: Vec<IngredientStatus>,
    pub missing: Vec<String>,
    pub uses_pantry: usize,
}

fn covered(ing: &Ingredient, have: &[String]) -> bool {
    have.iter().any(|h| matches(h, &ing.name))
}

pub struct MatchOpts<'a> {
    pub pantry: &'a [String],
    pub shopping: &'a [String],
    pub tags: &'a [String],
    pub min_coverage: f64,
    pub limit: usize,
}

/// Direction 1: "what can I cook with what I have / am buying?"
/// Recipes are ranked by how much of their non-staple ingredients are covered.
/// Staples (salt, oil…) never count against a recipe.
pub fn match_recipes(recipes: &[Recipe], o: &MatchOpts) -> Vec<MatchedRecipe> {
    let wanted: Vec<String> = o.tags.iter().map(|t| t.to_lowercase()).collect();
    let nothing_given = o.pantry.is_empty() && o.shopping.is_empty();
    let mut out: Vec<MatchedRecipe> = recipes
        .iter()
        .filter(|r| wanted.iter().all(|t| r.tags.contains(t)))
        .map(|r| {
            let rows: Vec<IngredientStatus> = r
                .ingredients
                .iter()
                .filter(|i| !i.staple)
                .map(|i| IngredientStatus {
                    name: i.name.clone(),
                    status: if covered(i, o.pantry) { Status::Pantry }
                        else if covered(i, o.shopping) { Status::Shopping }
                        else { Status::Missing },
                })
                .collect();
            let have = rows.iter().filter(|x| x.status != Status::Missing).count();
            MatchedRecipe {
                id: r.id.clone(), title: r.title.clone(), minutes: r.minutes, servings: r.servings, tags: r.tags.clone(),
                coverage: if rows.is_empty() { 1.0 } else { have as f64 / rows.len() as f64 },
                missing: rows.iter().filter(|x| x.status == Status::Missing).map(|x| x.name.clone()).collect(),
                uses_pantry: rows.iter().filter(|x| x.status == Status::Pantry).count(),
                ingredients: rows,
            }
        })
        .filter(|r| r.coverage >= o.min_coverage && (nothing_given || r.ingredients.iter().any(|i| i.status != Status::Missing)))
        .collect();
    out.sort_by(|a, b| {
        b.coverage.partial_cmp(&a.coverage).unwrap()
            .then(a.missing.len().cmp(&b.missing.len()))
            .then(a.minutes.cmp(&b.minutes))
    });
    out.truncate(o.limit);
    out
}

#[derive(Deserialize, Debug)]
pub struct Pick {
    pub id: String,
    pub servings: Option<f64>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShoppingItem {
    pub name: String,
    pub qty: f64,
    pub unit: String,
    pub for_recipes: Vec<String>,
}

/// Direction 2: "I picked these recipes — what do I need to buy?"
/// Scales quantities, merges duplicates, subtracts the pantry and anything already on the list.
pub fn build_shopping_list(recipes: &[Recipe], picks: &[Pick], pantry: &[String], shopping: &[String], include_staples: bool) -> Vec<ShoppingItem> {
    let mut merged: HashMap<String, ShoppingItem> = HashMap::new();
    for pick in picks {
        let Some(r) = recipes.iter().find(|r| r.id == pick.id) else { continue };
        let scale = pick.servings.map_or(1.0, |s| s / r.servings.max(1) as f64);
        for ing in &r.ingredients {
            if (ing.staple && !include_staples) || covered(ing, pantry) || covered(ing, shopping) {
                continue;
            }
            let row = merged.entry(format!("{}|{}", normalize(&ing.name), ing.unit)).or_insert_with(|| ShoppingItem {
                name: ing.name.clone(), qty: 0.0, unit: ing.unit.clone(), for_recipes: vec![],
            });
            row.qty += ing.qty * scale;
            if !row.for_recipes.contains(&r.title) {
                row.for_recipes.push(r.title.clone());
            }
        }
    }
    let mut items: Vec<ShoppingItem> = merged.into_values().map(|mut x| { x.qty = (x.qty * 100.0).round() / 100.0; x }).collect();
    items.sort_by(|a, b| a.name.cmp(&b.name).then(a.unit.cmp(&b.unit)));
    items
}

#[derive(Debug)]
pub enum GenError { NoAi, Failed(String) }

/// Optional: ask Claude for new recipes.
pub async fn generate_recipes(pantry: &[String], shopping: &[String], preferences: &str, count: usize) -> Result<Vec<Recipe>, GenError> {
    let key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty()).ok_or(GenError::NoAi)?;
    let model = std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-5-5".into());
    let none = |v: &[String]| if v.is_empty() { "none".to_string() } else { v.join(", ") };
    let prompt = format!(
        "Create {count} practical home recipes using mostly these ingredients.\n\
         Pantry (already owned): {}\nShopping list (will buy): {}\nPreferences/restrictions: {}\n\
         Prefer recipes that need few extra ingredients; ordinary staples (salt, pepper, oil, water) are assumed.\n\
         Reply with ONLY a JSON array. Each element: {{\"title\":string,\"minutes\":number,\"servings\":number,\"tags\":string[],\n\
         \"ingredients\":[{{\"name\":string (singular, generic e.g. \"tomato\"),\"qty\":number,\"unit\":string,\"staple\":boolean (optional)}}],\"steps\":string[]}}",
        none(pantry), none(shopping), if preferences.is_empty() { "none" } else { preferences });
    let res = crate::providers::client()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .timeout(std::time::Duration::from_secs(60))
        .json(&json!({"model": model, "max_tokens": 4000, "messages": [{"role": "user", "content": prompt}]}))
        .send().await.map_err(|e| GenError::Failed(e.to_string()))?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(GenError::Failed(format!("Anthropic API {status}: {}", body.chars().take(200).collect::<String>())));
    }
    let v: Value = res.json().await.map_err(|e| GenError::Failed(e.to_string()))?;
    let text: String = v["content"].as_array().map(|a| a.iter().filter_map(|b| b["text"].as_str()).collect()).unwrap_or_default();
    parse_generated(&text).map_err(GenError::Failed)
}

pub fn parse_generated(text: &str) -> Result<Vec<Recipe>, String> {
    let (start, end) = (text.find('['), text.rfind(']'));
    let (Some(s), Some(e)) = (start, end) else { return Err("Model did not return a JSON array".into()) };
    if e < s { return Err("Model did not return a JSON array".into()); }
    let arr: Vec<Value> = serde_json::from_str(&text[s..=e]).map_err(|e| format!("Model returned invalid JSON: {e}"))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    Ok(arr.iter()
        .filter(|r| r["title"].is_string() && r["ingredients"].is_array() && r["steps"].is_array())
        .enumerate()
        .map(|(i, r)| {
            let num = |k: &str, d: u32| r[k].as_f64().filter(|n| *n >= 1.0).map_or(d, |n| n.round() as u32);
            let mut tags: Vec<String> = r["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default();
            tags.push("ai".into());
            Recipe {
                id: format!("ai-{stamp:x}-{i}"),
                title: r["title"].as_str().unwrap().to_string(),
                minutes: num("minutes", 30), servings: num("servings", 2), tags,
                steps: r["steps"].as_array().unwrap().iter().map(|s| s.as_str().map(String::from).unwrap_or_else(|| s.to_string())).collect(),
                ingredients: r["ingredients"].as_array().unwrap().iter()
                    .filter(|g| g["name"].as_str().is_some_and(|n| !n.is_empty()))
                    .map(|g| Ingredient {
                        name: g["name"].as_str().unwrap().to_string(),
                        qty: g["qty"].as_f64().unwrap_or(0.0),
                        unit: g["unit"].as_str().unwrap_or("").to_string(),
                        staple: g["staple"].as_bool().unwrap_or(false),
                    }).collect(),
            }
        })
        .collect())
}
