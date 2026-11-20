use std::{fs, path::Path};

/// Minimal .env loader (no dependency). Real environment variables win.
fn load_dotenv() {
    let Ok(text) = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(".env")) else { return };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if !k.is_empty() && !v.is_empty() && !k.starts_with('#') && std::env::var_os(k).is_none() {
            // SAFETY: runs at the top of main, before the async runtime spawns any threads.
            unsafe { std::env::set_var(k, v) };
        }
    }
}

fn main() {
    load_dotenv();
    let port = std::env::var("PORT").ok().and_then(|p| p.parse::<u16>().ok()).unwrap_or(3000);
    let public = Path::new(env!("CARGO_MANIFEST_DIR")).join("public");
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    rt.block_on(async {
        let app = beans_mostly::api::router(public.to_str().expect("utf-8 path"));
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.expect("bind port");
        println!("Beans Mostly running at http://localhost:{port}");
        axum::serve(listener, app).await.expect("server");
    });
}
