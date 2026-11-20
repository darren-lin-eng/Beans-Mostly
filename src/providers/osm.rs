//! Real nearby grocery stores from OpenStreetMap (Nominatim geocoding + Overpass). No API key needed.
//! OSM has locations but no product/price data, so stores from here are for discovery only.
use super::{Store, client, fetch_json, haversine_km, round};
use std::time::Duration;

/// Geocode a ZIP/city/address. Restricted to GEOCODE_COUNTRIES (default "us") because bare ZIPs are
/// ambiguous worldwide ("10001" alone resolves to Algeria).
pub async fn geocode(text: &str) -> Result<(f64, f64), String> {
    let countries = std::env::var("GEOCODE_COUNTRIES").unwrap_or_else(|_| "us".into());
    let v = fetch_json(client().get("https://nominatim.openstreetmap.org/search")
        .query(&[("format", "json"), ("limit", "1"), ("countrycodes", countries.as_str()), ("q", text)])).await?;
    let hit = v.get(0).ok_or_else(|| format!("Could not find a location for \"{text}\""))?;
    let num = |k: &str| hit[k].as_str().and_then(|s| s.parse::<f64>().ok()).ok_or("Bad geocoder response");
    Ok((num("lat")?, num("lon")?))
}

/// Turn user text like "Sam's Club" into a case-insensitive Overpass regex that is safe to embed in a query.
fn chain_regex(chain: &str) -> String {
    chain.trim().chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '.' }).collect()
}

pub async fn find_stores(here: (f64, f64), radius_km: f64, chain: Option<&str>) -> Result<Vec<Store>, String> {
    let url = std::env::var("OVERPASS_URL").unwrap_or_else(|_| "https://overpass-api.de/api/interpreter".into());
    let around = format!("(around:{},{},{})", (radius_km * 1000.0).round() as i64, here.0, here.1);
    let shop = "[\"shop\"~\"^(supermarket|grocery|greengrocer|convenience|butcher|bakery|deli|wholesale)$\"]";
    // With a chain filter, ask Overpass for just that chain (name or brand) so the result cap can't hide it.
    let q = match chain.map(chain_regex).filter(|c| !c.is_empty()) {
        Some(c) => format!("[out:json][timeout:20];(nwr{shop}[\"name\"~\"{c}\",i]{around};nwr{shop}[\"brand\"~\"{c}\",i]{around};);out center 60;"),
        None => format!("[out:json][timeout:15];nwr{shop}{around};out center 60;"),
    };
    let data = fetch_json(client().post(url).timeout(Duration::from_secs(20)).form(&[("data", q)])).await?;
    let mut stores: Vec<Store> = data["elements"].as_array().into_iter().flatten().filter_map(|e| {
        let t = &e["tags"];
        let name = t["name"].as_str()?;
        let lat = e["lat"].as_f64().or(e["center"]["lat"].as_f64())?;
        let lng = e["lon"].as_f64().or(e["center"]["lon"].as_f64())?;
        let s = |k: &str| t[k].as_str().unwrap_or("");
        let address = [s("addr:housenumber"), s("addr:street"), s("addr:city")].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" ");
        Some(Store {
            id: format!("osm:{}/{}", e["type"].as_str()?, e["id"]), provider: "osm", name: name.into(), chain: s("brand").into(),
            address: if address.is_empty() { "Address not listed".into() } else { address }, hours: s("opening_hours").into(),
            tags: vec![s("shop").into()], lat, lng, distance_km: round(haversine_km(here, (lat, lng)), 1), has_products: false,
        })
    }).collect();
    stores.sort_by(|a, b| a.distance_km.partial_cmp(&b.distance_km).unwrap());
    Ok(stores)
}
