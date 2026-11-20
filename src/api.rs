use crate::providers::{Product, Provider, osm, round};
use crate::recipes::{self, GenError, MatchOpts, Pick, RECIPES, Recipe};
use axum::{Json, Router, body::Bytes, extract::Query, http::StatusCode, response::{IntoResponse, Response}, routing::{get, post}};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use tower_http::services::ServeDir;

pub struct ApiError(StatusCode, String);
impl ApiError {
    fn new(code: u16, msg: impl Into<String>) -> Self { Self(StatusCode::from_u16(code).unwrap(), msg.into()) }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response { (self.0, Json(json!({ "error": self.1 }))).into_response() }
}
type Res = Result<Json<Value>, ApiError>;

fn body(bytes: &Bytes) -> Result<Value, ApiError> {
    if bytes.is_empty() { return Ok(json!({})); }
    if bytes.len() > 1_000_000 { return Err(ApiError::new(413, "Body too large")); }
    serde_json::from_slice(bytes).map_err(|_| ApiError::new(400, "Invalid JSON"))
}

/// A JSON array of strings → trimmed, non-empty, capped (never trust client-sized input).
fn list(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten()
        .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty()).take(200).collect()
}

fn ok<T: Serialize>(v: T) -> Res { Ok(Json(serde_json::to_value(v).unwrap())) }

async fn config() -> Res {
    let tags: BTreeSet<&String> = RECIPES.iter().flat_map(|r| &r.tags).collect();
    Ok(Json(json!({
        "providers": Provider::ALL.iter().map(|p| json!({"id": p.id(), "label": p.label(), "available": p.available(), "hasProducts": p.has_products()})).collect::<Vec<_>>(),
        "ai": std::env::var("ANTHROPIC_API_KEY").is_ok_and(|k| !k.is_empty()),
        "tags": tags,
    })))
}

async fn stores(Query(q): Query<HashMap<String, String>>) -> Res {
    let p = Provider::from_id(q.get("provider").map_or("osm", String::as_str)).filter(|p| p.available())
        .ok_or_else(|| ApiError::new(400, "Provider unavailable"))?;
    let num = |k: &str| q.get(k).and_then(|v| v.parse::<f64>().ok());
    let loc = match (num("lat"), num("lng"), q.get("q").filter(|s| !s.is_empty())) {
        (Some(lat), Some(lng), _) => Some((lat, lng)),
        (_, _, Some(text)) => Some(osm::geocode(text).await.map_err(|e| ApiError::new(502, e))?),
        _ => None,
    };
    if loc.is_none() {
        return Err(ApiError::new(400, "Provide lat/lng or a q= place/zip"));
    }
    let radius = num("radiusKm").filter(|r| *r > 0.0).unwrap_or(8.0).min(50.0);
    let stores = p.find_stores(loc, radius).await.map_err(|e| ApiError::new(502, e))?;
    Ok(Json(json!({ "location": loc.map_or(json!({}), |(lat, lng)| json!({"lat": lat, "lng": lng})), "stores": stores })))
}

async fn products(Query(q): Query<HashMap<String, String>>) -> Res {
    let (Some(store), Some(term)) = (q.get("store"), q.get("q").filter(|s| !s.is_empty())) else {
        return Err(ApiError::new(400, "store and q are required"));
    };
    let p = Provider::for_store(store).ok_or_else(|| ApiError::new(400, "store and q are required"))?;
    let products = p.search_products(store, term).await.map_err(|e| ApiError::new(502, e))?;
    Ok(Json(json!({ "products": products })))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRow { pub item: String, pub product: Option<Product>, #[serde(skip_serializing_if = "Option::is_none")] pub error: Option<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorePrices { pub store_id: String, pub rows: Vec<PriceRow>, pub total: f64, pub found: usize }

/// Cheapest matching product per item at each store; stores ranked by items found, then total.
pub async fn cheapest_per_item(items: &[String], store_ids: &[String]) -> Vec<StorePrices> {
    let per_store = store_ids.iter().filter_map(|id| Provider::for_store(id).filter(|p| p.has_products()).map(|p| (id, p))).map(|(id, p)| async move {
        let rows: Vec<PriceRow> = futures::future::join_all(items.iter().map(|item| async move {
            match p.search_products(id, item).await {
                Ok(hits) => PriceRow { item: item.clone(), product: hits.into_iter().next(), error: None },
                Err(e) => PriceRow { item: item.clone(), product: None, error: Some(e) },
            }
        })).await;
        StorePrices {
            store_id: id.clone(), total: round(rows.iter().filter_map(|r| r.product.as_ref()).map(|p| p.price).sum(), 2),
            found: rows.iter().filter(|r| r.product.is_some()).count(), rows,
        }
    });
    rank_stores(futures::future::join_all(per_store).await)
}

/// Best store first: most items found, then lowest total.
pub fn rank_stores(mut stores: Vec<StorePrices>) -> Vec<StorePrices> {
    stores.sort_by(|a, b| b.found.cmp(&a.found).then(a.total.partial_cmp(&b.total).unwrap()));
    stores
}

async fn prices(b: Bytes) -> Res {
    let b = body(&b)?;
    let (items, ids) = (list(&b["items"]), list(&b["storeIds"]));
    ok(json!({ "stores": cheapest_per_item(&items[..items.len().min(40)], &ids[..ids.len().min(8)]).await }))
}

async fn match_endpoint(b: Bytes) -> Res {
    let b = body(&b)?;
    let (pantry, shopping, tags) = (list(&b["pantry"]), list(&b["shopping"]), list(&b["tags"]));
    let matched = recipes::match_recipes(&RECIPES, &MatchOpts {
        pantry: &pantry, shopping: &shopping, tags: &tags, min_coverage: b["minCoverage"].as_f64().unwrap_or(0.4), limit: 12,
    });
    ok(json!({ "recipes": matched }))
}

async fn recipes_get(Query(q): Query<HashMap<String, String>>) -> Res {
    match q.get("id") {
        Some(id) => match RECIPES.iter().find(|r| &r.id == id) {
            Some(r) => ok(r),
            None => Err(ApiError::new(404, "No such recipe")),
        },
        None => ok(json!({ "recipes": *RECIPES })),
    }
}

async fn shopping_list(b: Bytes) -> Res {
    let b = body(&b)?;
    let custom: Vec<Recipe> = b["custom"].as_array().into_iter().flatten().filter_map(|r| serde_json::from_value(r.clone()).ok()).collect();
    let picks: Vec<Pick> = b["picks"].as_array().into_iter().flatten().take(30).filter_map(|p| serde_json::from_value(p.clone()).ok()).collect();
    let all: Vec<Recipe> = RECIPES.iter().cloned().chain(custom).collect();
    let items = recipes::build_shopping_list(&all, &picks, &list(&b["pantry"]), &list(&b["shopping"]), b["includeStaples"].as_bool().unwrap_or(false));
    ok(json!({ "items": items }))
}

async fn generate(b: Bytes) -> Res {
    let b = body(&b)?;
    let (pantry, shopping) = (list(&b["pantry"]), list(&b["shopping"]));
    let prefs: String = b["preferences"].as_str().unwrap_or("").chars().take(300).collect();
    let count = (b["count"].as_u64().unwrap_or(3) as usize).clamp(1, 5);
    let generated = recipes::generate_recipes(&pantry, &shopping, &prefs, count).await.map_err(|e| match e {
        GenError::NoAi => ApiError::new(501, "AI recipes are disabled: set ANTHROPIC_API_KEY on the server."),
        GenError::Failed(m) => ApiError::new(502, format!("Recipe generation failed: {m}")),
    })?;
    let matched = recipes::match_recipes(&generated, &MatchOpts { pantry: &pantry, shopping: &shopping, tags: &[], min_coverage: 0.0, limit: 10 });
    let out: Vec<Value> = matched.iter().map(|m| {
        let g = generated.iter().find(|g| g.id == m.id).expect("matched recipe came from generated");
        let mut v = serde_json::to_value(m).unwrap();
        v["steps"] = json!(g.steps);
        v["fullIngredients"] = json!(g.ingredients); // original quantities; `ingredients` above carries match status
        v
    }).collect();
    Ok(Json(json!({ "recipes": out })))
}

pub fn router(public_dir: &str) -> Router {
    Router::new()
        .route("/api/config", get(config))
        .route("/api/stores", get(stores))
        .route("/api/products", get(products))
        .route("/api/prices", post(prices))
        .route("/api/recipes", get(recipes_get))
        .route("/api/recipes/match", post(match_endpoint))
        .route("/api/recipes/shopping-list", post(shopping_list))
        .route("/api/recipes/generate", post(generate))
        .fallback_service(ServeDir::new(public_dir))
}
