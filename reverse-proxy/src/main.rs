use std::env;

use hyper::Uri;

use openai_reverse_proxy::{Router, files::FileServer, openai::proxy::ReverseProxy, serve};

#[tokio::main]
async fn main() {
    env_logger::builder().init();
    if let Err(e) = dotenvy::dotenv() {
        log::warn!("Cannot load .env file: {e}");
    }

    let server_addr = "0.0.0.0:4000";
    let proxy_url = "http://127.0.0.1:8080"
        .parse::<Uri>()
        .expect("Cannot parse proxy URL");
    let static_path = "../client-example/web";
    let system_prompt = env::var("SYSTEM_PROMPT").ok();
    log::info!("System prompt: {system_prompt:?}");

    let res = serve(server_addr, async move || {
        Ok(Router::new(FileServer::new(static_path)).push(
            "/chat/completions",
            ReverseProxy::new(proxy_url.clone()).with_system_prompt(system_prompt.clone()),
        ))
    })
    .await;
    if let Err(e) = res {
        log::error!("Error running server: {e}");
        panic!();
    }
}
