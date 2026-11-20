//! Store-data providers. Each knows how to find stores and (optionally) search a store's products.
pub mod kroger;
pub mod osm;

use serde::Serialize;
use serde_json::Value;
use std::{sync::LazyLock, time::Duration};

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder().timeout(Duration::from_secs(10)).user_agent("beans-mostly/0.2 (grocery recipe app)").build().unwrap()
});
pub fn client() -> &'static reqwest::Client { &CLIENT }

pub async fn fetch_json(req: reqwest::RequestBuilder) -> Result<Value, String> {
    let res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status();
    if !status.is_success() {
        return Err(format!("{} responded {}", res.url().host_str().unwrap_or("upstream"), status.as_u16()));
    }
    res.json().await.map_err(|e| e.to_string())
}

pub fn round(n: f64, decimals: i32) -> f64 {
    let m = 10f64.powi(decimals);
    (n * m).round() / m
}

pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let rad = f64::to_radians;
    let (d_lat, d_lng) = (rad(b.0 - a.0), rad(b.1 - a.1));
    let h = (d_lat / 2.0).sin().powi(2) + rad(a.0).cos() * rad(b.0).cos() * (d_lng / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().asin()
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Store {
    pub id: String,
    pub provider: &'static str,
    pub name: String,
    pub chain: String,
    pub address: String,
    pub hours: String,
    pub tags: Vec<String>,
    pub lat: f64,
    pub lng: f64,
    pub distance_km: f64,
    pub has_products: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub name: String,
    pub price: f64,
    pub store_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_sale: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider { Osm, Kroger }

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::Osm, Provider::Kroger];

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }
    /// Store ids look like "kroger:01400943" / "osm:node/123".
    pub fn for_store(store_id: &str) -> Option<Self> {
        Self::from_id(store_id.split(':').next().unwrap_or(""))
    }
    pub fn id(self) -> &'static str {
        match self { Self::Osm => "osm", Self::Kroger => "kroger" }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Osm => "OpenStreetMap (real stores, no prices)",
            Self::Kroger => "Kroger family (live prices)",
        }
    }
    pub fn has_products(self) -> bool { self == Self::Kroger }
    pub fn available(self) -> bool {
        match self { Self::Kroger => kroger::configured(), _ => true }
    }

    pub async fn find_stores(self, loc: Option<(f64, f64)>, radius_km: f64, chain: Option<&str>) -> Result<Vec<Store>, String> {
        match self {
            Self::Osm => osm::find_stores(loc.ok_or("A location is required")?, radius_km, chain).await,
            Self::Kroger => kroger::find_stores(loc.ok_or("A location is required")?, radius_km, chain).await,
        }
    }

    pub async fn search_products(self, store_id: &str, query: &str) -> Result<Vec<Product>, String> {
        match self {
            Self::Osm => Ok(vec![]),
            Self::Kroger => kroger::search_products(store_id, query).await,
        }
    }
}
