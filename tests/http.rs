use serde_json::{Value, json};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};

async fn serve() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = beans_mostly::api::router(concat!(env!("CARGO_MANIFEST_DIR"), "/public"));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    port
}

/// Tiny raw HTTP/1.1 client so tests need no extra dependencies.
async fn http(port: u16, method: &str, path: &str, body: Option<Value>) -> (u16, String) {
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let mut c = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let req = format!("{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    c.write_all(req.as_bytes()).await.unwrap();
    let mut out = String::new();
    c.read_to_string(&mut out).await.unwrap();
    let status = out[9..12].parse().unwrap();
    (status, out.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
}

#[tokio::test]
async fn api_end_to_end() {
    let port = serve().await;

    let (st, b) = http(port, "GET", "/api/config", None).await;
    let cfg: Value = serde_json::from_str(&b).unwrap();
    assert_eq!(st, 200);
    assert_eq!(cfg["providers"][0]["id"], "sample");
    assert!(cfg["tags"].as_array().unwrap().contains(&json!("vegan")));

    let (_, b) = http(port, "GET", "/api/stores?provider=sample", None).await;
    let v: Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["stores"].as_array().unwrap().len(), 4);
    assert_eq!(v["stores"][0]["hasProducts"], true);

    let (_, b) = http(port, "POST", "/api/recipes/match", Some(json!({"pantry": ["pasta", "garlic"], "shopping": ["tomatoes", "basil", "parmesan"]}))).await;
    let v: Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["recipes"][0]["id"], "tomato-basil-pasta");
    assert_eq!(v["recipes"][0]["usesPantry"], 2); // camelCase contract the frontend relies on

    let (_, b) = http(port, "POST", "/api/prices", Some(json!({"items": ["milk"], "storeIds": ["sample:valuemart"]}))).await;
    let v: Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["stores"][0]["storeId"], "sample:valuemart");
    assert!(v["stores"][0]["rows"][0]["product"]["price"].as_f64().unwrap() > 0.0);

    let (_, b) = http(port, "POST", "/api/recipes/shopping-list", Some(json!({"picks": [{"id": "tomato-basil-pasta", "servings": 3}], "pantry": ["pasta"]}))).await;
    let v: Value = serde_json::from_str(&b).unwrap();
    assert!(v["items"].as_array().unwrap().iter().any(|i| i["name"] == "parmesan" && i["forRecipes"][0] == "Tomato Basil Pasta"));
}

#[tokio::test]
async fn errors_are_json_and_static_files_are_served() {
    let port = serve().await;
    let (st, b) = http(port, "POST", "/api/prices", Some(json!("x"))).await;
    assert_eq!(st, 200, "{b}"); // non-object body is tolerated as empty lists
    let (st, b) = http(port, "GET", "/api/products", None).await;
    assert_eq!((st, serde_json::from_str::<Value>(&b).unwrap()["error"].as_str().is_some()), (400, true));
    let (st, _) = http(port, "GET", "/api/recipes?id=nope", None).await;
    assert_eq!(st, 404);
    let (st, _) = http(port, "GET", "/api/stores?provider=osm", None).await;
    assert_eq!(st, 400); // needs a location
    let (st, html) = http(port, "GET", "/", None).await;
    assert!(st == 200 && html.contains("Beans"));
    let (st, _) = http(port, "GET", "/../Cargo.toml", None).await;
    assert_ne!(st, 200); // no path traversal
}
