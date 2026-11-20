//! Built-in demo provider: a fake-but-plausible set of stores placed around the searched location,
//! with deterministic per-store pricing. Lets the whole app work offline with zero setup.
use super::{Product, Store, haversine_km, round};
use crate::normalize::{matches, normalize};
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
struct CatStore { id: String, name: String, chain: String, offset: [f64; 2], mult: f64, hours: String, tags: Vec<String> }
#[derive(Deserialize)]
struct CatProduct { name: String, category: String, price: f64, size: String }
#[derive(Deserialize)]
struct Catalog { stores: Vec<CatStore>, products: Vec<CatProduct> }

static CATALOG: LazyLock<Catalog> =
    LazyLock::new(|| serde_json::from_str(include_str!("../../data/sample-catalog.json")).expect("sample catalog is valid"));

const DEFAULT_CENTER: (f64, f64) = (40.7128, -74.006);

/// FNV-1a style hash mapped to 0..1, so prices are stable per (store, product).
fn hash(s: &str) -> f64 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    h as f64 / 4294967296.0
}

pub fn find_stores(loc: Option<(f64, f64)>, radius_km: f64) -> Vec<Store> {
    let center = loc.unwrap_or(DEFAULT_CENTER);
    let mut stores: Vec<Store> = CATALOG.stores.iter().map(|s| {
        let at = (center.0 + s.offset[0], center.1 + s.offset[1]);
        Store {
            id: s.id.clone(), provider: "sample", name: s.name.clone(), chain: s.chain.clone(),
            address: "Demo address".into(), hours: s.hours.clone(), tags: s.tags.clone(),
            lat: at.0, lng: at.1, distance_km: round(haversine_km(center, at), 1), has_products: true,
        }
    }).filter(|s| s.distance_km <= radius_km).collect();
    stores.sort_by(|a, b| a.distance_km.partial_cmp(&b.distance_km).unwrap());
    stores
}

pub fn search_products(store_id: &str, query: &str) -> Result<Vec<Product>, String> {
    let store = CATALOG.stores.iter().find(|s| s.id == store_id).ok_or_else(|| format!("Unknown store {store_id}"))?;
    let q = normalize(query);
    if q.is_empty() {
        return Ok(vec![]);
    }
    let mut hits: Vec<Product> = CATALOG.products.iter().filter(|p| matches(&p.name, &q)).map(|p| {
        let jitter = 0.92 + hash(&format!("{}{}", store.id, p.name)) * 0.16; // ±8% per store+product
        Product {
            name: p.name.clone(), category: Some(p.category.clone()), size: Some(p.size.clone()),
            price: round(p.price * store.mult * jitter, 2), store_id: store.id.clone(), ..Default::default()
        }
    }).collect();
    // exact name first, then cheapest
    hits.sort_by(|a, b| {
        (normalize(&b.name) == q).cmp(&(normalize(&a.name) == q)).then(a.price.partial_cmp(&b.price).unwrap())
    });
    Ok(hits)
}
