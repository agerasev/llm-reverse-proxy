use std::{env, path::Path};

use clap::Parser;
use hyper::Uri;

use openai_reverse_proxy::{
    Router,
    files::FileServer,
    openai::proxy::{ReverseProxy, ServerKind},
    serve,
};
use tokio::{fs::File, io::AsyncReadExt};

#[derive(Clone, Debug, Parser)]
struct Args {
    /// Address to listen to client connections
    #[arg(short, long, default_value = "0.0.0.0:4000")]
    addr: String,
    /// Server URL where client connection should be forwarded
    #[arg(short, long)]
    server: String,
    /// HTTP proxy address
    #[arg(long)]
    proxy: Option<String>,
    /// System prompt
    #[arg(long)]
    prompt: Option<String>,
    /// Static file server root path
    #[arg(long)]
    files: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::builder().init();
    let args = Args::parse();

    let server_url = args.server.parse::<Uri>().expect("Cannot parse server URL");
    assert!(matches!(server_url.scheme_str(), Some("http" | "https")));
    assert!(server_url.authority().is_some());
    assert!(server_url.path() == "/");
    assert!(server_url.query().is_none());

    let (server_kind, model_name) = if server_url.host() == Some("api.openai.com") {
        (ServerKind::OpenAi, "gpt-4o-mini".to_string())
    } else {
        (ServerKind::LlamaCpp, String::new())
    };
    log::info!("{server_kind:?}");
    log::info!("Model_name: {model_name}");

    let proxy_url = args
        .proxy
        .map(|s| s.parse::<Uri>())
        .transpose()
        .expect("Cannot parse HTTP proxy URL")
        .inspect(|url| log::info!("Using HTTP proxy: {url}"));

    let file_server = if let Some(path) = &args.files {
        let path = Path::new(path);
        assert!(
            path.is_dir(),
            "Static path doesn't exist or is not a directory"
        );
        Some(FileServer::new(path))
    } else {
        None
    };

    if let Err(e) = dotenvy::dotenv() {
        log::warn!("Cannot load .env file: {e}");
    }
    let api_key = if let ServerKind::OpenAi { .. } = &server_kind {
        assert!(server_url.scheme_str() == Some("https"));
        Some(env::var("OPENAI_API_KEY").expect("OpenAI API key is not set"))
    } else {
        None
    };
    let system_prompt = if let Some(prompt) = args.prompt.or_else(|| env::var("SYSTEM_PROMPT").ok())
    {
        Some(match prompt.strip_prefix("file:") {
            Some(path) => {
                let mut content = String::new();
                File::open(&path)
                    .await
                    .expect("Cannot open prompt file")
                    .read_to_string(&mut content)
                    .await
                    .expect("Cannot read prompt from file");
                content
            }
            None => prompt,
        })
    } else {
        None
    };
    log::info!("System prompt: {system_prompt:?}");

    let res = serve(args.addr, async move || {
        Ok(Router::new(file_server.clone()).push(
            "/chat/completions",
            ReverseProxy::new(server_url.clone())
                .proxy(proxy_url.clone())
                .kind(server_kind)
                .model(model_name.clone())
                .api_key(api_key.clone())
                .system_prompt(system_prompt.clone()),
        ))
    })
    .await;
    if let Err(e) = res {
        log::error!("Error running server: {e}");
        panic!();
    }
}
