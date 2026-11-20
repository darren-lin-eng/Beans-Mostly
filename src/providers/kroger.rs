//! Kroger-family stores (Kroger, Ralphs, Fred Meyer, Harris Teeter, ...) via the official public API.
//! Needs KROGER_CLIENT_ID / KROGER_CLIENT_SECRET (free at developer.kroger.com).
use super::{Product, Store, client, fetch_json, haversine_km, round};
use std::{sync::LazyLock, time::{Duration, Instant}};
use tokio::sync::Mutex;

const BASE: &str = "https://api.kroger.com/v1";
static TOKEN: LazyLock<Mutex<Option<(String, Instant)>>> = LazyLock::new(|| Mutex::new(None));

fn env(k: &str) -> Option<String> { std::env::var(k).ok().filter(|v| !v.is_empty()) }
pub fn configured() -> bool { env("KROGER_CLIENT_ID").is_some() && env("KROGER_CLIENT_SECRET").is_some() }

async fn token() -> Result<String, String> {
    let mut cached = TOKEN.lock().await;
    if let Some((t, exp)) = cached.as_ref() {
        if Instant::now() < *exp { return Ok(t.clone()); }
    }
    let (id, secret) = (env("KROGER_CLIENT_ID").ok_or("KROGER_CLIENT_ID not set")?, env("KROGER_CLIENT_SECRET").ok_or("KROGER_CLIENT_SECRET not set")?);
    let v = fetch_json(client().post(format!("{BASE}/connect/oauth2/token")).basic_auth(id, Some(secret))
        .form(&[("grant_type", "client_credentials"), ("scope", "product.compact")])).await?;
    let t = v["access_token"].as_str().ok_or("Kroger returned no access token")?.to_string();
    let ttl = v["expires_in"].as_u64().unwrap_or(1800).saturating_sub(60);
    *cached = Some((t.clone(), Instant::now() + Duration::from_secs(ttl)));
    Ok(t)
}

pub async fn find_stores(here: (f64, f64), radius_km: f64, chain: Option<&str>) -> Result<Vec<Store>, String> {
    let miles = ((radius_km * 0.621).round() as i64).max(1);
    let v = fetch_json(client().get(format!("{BASE}/locations")).bearer_auth(token().await?).query(&[
        ("filter.latLong.near", format!("{},{}", here.0, here.1)), ("filter.radiusInMiles", miles.to_string()), ("filter.limit", "10".into()),
    ])).await?;
    let wanted = chain.map(|c| c.trim().to_lowercase()).filter(|c| !c.is_empty());
    let mut stores: Vec<Store> = v["data"].as_array().into_iter().flatten().filter_map(|l| {
        let lat = l["geolocation"]["latitude"].as_f64()?;
        let lng = l["geolocation"]["longitude"].as_f64()?;
        let a = &l["address"];
        let g = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
        Some(Store {
            id: format!("kroger:{}", l["locationId"].as_str()?), provider: "kroger", name: g(&l["name"]), chain: g(&l["chain"]),
            address: format!("{}, {} {}", g(&a["addressLine1"]), g(&a["city"]), g(&a["state"])), hours: String::new(),
            tags: vec![g(&l["chain"]).to_lowercase()], lat, lng, distance_km: round(haversine_km(here, (lat, lng)), 1), has_products: true,
        })
    }).collect();
    if let Some(w) = wanted { stores.retain(|s| s.name.to_lowercase().contains(&w) || s.chain.to_lowercase().contains(&w)); }
    stores.sort_by(|a, b| a.distance_km.partial_cmp(&b.distance_km).unwrap());
    Ok(stores)
}

pub async fn search_products(store_id: &str, query: &str) -> Result<Vec<Product>, String> {
    let loc = store_id.trim_start_matches("kroger:");
    let v = fetch_json(client().get(format!("{BASE}/products")).bearer_auth(token().await?)
        .query(&[("filter.term", query), ("filter.locationId", loc), ("filter.limit", "8")])).await?;
    let mut out: Vec<Product> = v["data"].as_array().into_iter().flatten().filter_map(|p| {
        let item = &p["items"][0];
        let promo = item["price"]["promo"].as_f64().filter(|x| *x > 0.0);
        let price = promo.or(item["price"]["regular"].as_f64())?;
        Some(Product {
            name: p["description"].as_str()?.into(), brand: p["brand"].as_str().map(Into::into), size: item["size"].as_str().map(Into::into),
            price, on_sale: Some(promo.is_some()), category: p["categories"][0].as_str().map(Into::into), store_id: store_id.into(),
        })
    }).collect();
    out.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
    Ok(out)
}
