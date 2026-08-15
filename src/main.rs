use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use rand::RngCore;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use url::Url;
use uuid::Uuid;

const RESERVED_SLUGS: &[&str] = &[
    "api",
    "auth",
    "autoconfig",
    "autodiscover",
    "cli",
    "mail",
    "micro",
    "runner",
    "status",
    "www",
];

#[derive(Parser)]
#[command(
    name = "micro",
    version,
    about = "Deploy small WebAssembly functions to micro.do"
)]
struct Cli {
    #[arg(long, env = "MICRO_API", default_value = "https://micro.do")]
    api: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pair this CLI with your micro.do account in the browser.
    Login,
    /// Remove locally stored credentials.
    Logout,
    /// Deploy a micro.wasm.v1 module.
    Deploy { slug: String, wasm: PathBuf },
    /// List functions owned by the current account.
    Functions,
}

#[derive(Debug, Serialize, Deserialize)]
struct Credentials {
    access_token: String,
    refresh_token: String,
}

#[derive(Serialize)]
struct PairStart<'a> {
    state: &'a str,
    code_challenge: &'a str,
    redirect_uri: &'a str,
}

#[derive(Deserialize)]
struct PairStarted {
    pairing_id: Uuid,
    authorize_url: String,
}

#[derive(Serialize)]
struct PairExchange<'a> {
    pairing_id: Uuid,
    state: &'a str,
    code: &'a str,
    code_verifier: &'a str,
}

#[derive(Deserialize)]
struct TokenPairResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct DeployResponse {
    deployment_id: String,
    artifact_sha256: String,
    url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let api = cli.api.trim_end_matches('/');
    validate_api_url(api)?;
    let client = Client::builder()
        .user_agent(concat!("micro-cli/", env!("CARGO_PKG_VERSION")))
        .build()?;
    match cli.command {
        Command::Login => login(&client, api).await,
        Command::Logout => logout().await,
        Command::Deploy { slug, wasm } => deploy(&client, api, &slug, &wasm).await,
        Command::Functions => functions(&client, api).await,
    }
}

async fn login(client: &Client, api: &str) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/callback",
        listener.local_addr()?.port()
    );
    let state = random_token(32);
    let verifier = random_token(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let started: PairStarted = checked_json(
        client
            .post(format!("{api}/api/v1/cli/pair/start"))
            .json(&PairStart {
                state: &state,
                code_challenge: &challenge,
                redirect_uri: &redirect_uri,
            })
            .send()
            .await?,
    )
    .await?;
    println!("Opening {}", started.authorize_url);
    if open::that(&started.authorize_url).is_err() {
        println!("Open that URL in your browser to continue.");
    }
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(300), listener.accept())
        .await
        .context("browser authorization timed out")??;
    let mut buffer = vec![0_u8; 8192];
    let bytes = socket.read(&mut buffer).await?;
    let request = std::str::from_utf8(&buffer[..bytes]).context("invalid browser callback")?;
    let target = request
        .split_whitespace()
        .nth(1)
        .context("invalid browser callback")?;
    let callback = Url::parse(&format!("http://127.0.0.1{target}"))?;
    let values: std::collections::HashMap<_, _> = callback.query_pairs().into_owned().collect();
    if callback.path() != "/callback"
        || values.get("state") != Some(&state)
        || values.get("pairing_id") != Some(&started.pairing_id.to_string())
    {
        bail!("browser callback state did not match");
    }
    let code = values
        .get("code")
        .context("browser callback did not contain a code")?;
    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><title>micro CLI paired</title><p>You can close this window and return to the terminal.</p>").await?;
    let pair: TokenPairResponse = checked_json(
        client
            .post(format!("{api}/api/v1/cli/pair/exchange"))
            .json(&PairExchange {
                pairing_id: started.pairing_id,
                state: &state,
                code,
                code_verifier: &verifier,
            })
            .send()
            .await?,
    )
    .await?;
    save_credentials(&Credentials {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
    })
    .await?;
    println!("Paired with micro.do.");
    Ok(())
}

async fn deploy(client: &Client, api: &str, slug: &str, path: &Path) -> Result<()> {
    validate_slug(slug)?;
    let wasm = fs::read(path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    let response = authorized(
        client,
        api,
        reqwest::Method::PUT,
        &format!("/api/v1/functions/{slug}"),
    )
    .await?
    .header(reqwest::header::CONTENT_TYPE, "application/wasm")
    .body(wasm)
    .send()
    .await?;
    let deployed: DeployResponse = checked_json(response).await?;
    println!("{}", deployed.url);
    println!(
        "deployment {} · sha256 {}",
        deployed.deployment_id, deployed.artifact_sha256
    );
    Ok(())
}

async fn functions(client: &Client, api: &str) -> Result<()> {
    let response = authorized(client, api, reqwest::Method::GET, "/api/v1/functions")
        .await?
        .send()
        .await?;
    let functions: serde_json::Value = checked_json(response).await?;
    println!("{}", serde_json::to_string_pretty(&functions)?);
    Ok(())
}

async fn logout() -> Result<()> {
    let path = credentials_path()?;
    match fs::remove_file(&path).await {
        Ok(()) => println!("Removed local micro.do credentials."),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("Already logged out.")
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

async fn authorized(
    client: &Client,
    api: &str,
    method: reqwest::Method,
    path: &str,
) -> Result<reqwest::RequestBuilder> {
    let credentials_path = credentials_path()?;
    let credentials: Credentials = serde_json::from_slice(
        &fs::read(&credentials_path)
            .await
            .context("not logged in; run `micro login`")?,
    )
    .context("stored credentials are invalid; run `micro login`")?;
    let refreshed: TokenPairResponse = checked_json(
        client
            .post(format!("{api}/api/v1/auth/refresh"))
            .bearer_auth(&credentials.refresh_token)
            .send()
            .await?,
    )
    .await?;
    let credentials = Credentials {
        access_token: refreshed.access_token,
        refresh_token: refreshed.refresh_token,
    };
    save_credentials(&credentials).await?;
    Ok(client
        .request(method, format!("{api}{path}"))
        .bearer_auth(credentials.access_token))
}

async fn checked_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let bytes = response.bytes().await?;
    if !status.is_success() {
        let message = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| value.get("message")?.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| String::from_utf8_lossy(&bytes).into_owned());
        if status == StatusCode::UNAUTHORIZED {
            bail!("not authorized: {message}; run `micro login`");
        }
        bail!("micro.do returned {status}: {message}");
    }
    serde_json::from_slice(&bytes).context("decode micro.do response")
}

fn credentials_path() -> Result<PathBuf> {
    let project = ProjectDirs::from("do", "micro", "micro-cli")
        .context("could not locate user config directory")?;
    Ok(project.config_dir().join("credentials.json"))
}

async fn save_credentials(credentials: &Credentials) -> Result<()> {
    let path = credentials_path()?;
    let parent = path.parent().context("credential path has no parent")?;
    fs::create_dir_all(parent).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)?;
        std::io::Write::write_all(&mut file, &serde_json::to_vec(credentials)?)?;
    }
    #[cfg(not(unix))]
    fs::write(&path, serde_json::to_vec(credentials)?).await?;
    Ok(())
}

fn validate_api_url(value: &str) -> Result<()> {
    let url = Url::parse(value).context("invalid API URL")?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    if url.scheme() != "https" && !(local && url.scheme() == "http") {
        bail!("API URL must use HTTPS (HTTP is allowed only for localhost)");
    }
    Ok(())
}

fn validate_slug(value: &str) -> Result<()> {
    if !(3..=63).contains(&value.len()) {
        bail!("slug must contain between 3 and 63 characters");
    }
    let bytes = value.as_bytes();
    if (!bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit())
        || (!bytes[bytes.len() - 1].is_ascii_lowercase()
            && !bytes[bytes.len() - 1].is_ascii_digit())
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        bail!("slug must use lowercase ASCII letters, digits, and interior hyphens");
    }
    if RESERVED_SLUGS.contains(&value) {
        bail!("slug is reserved by the platform");
    }
    Ok(())
}

fn random_token(size: usize) -> String {
    let mut bytes = vec![0_u8; size];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_api_transport() {
        assert!(validate_api_url("https://micro.do").is_ok());
        assert!(validate_api_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_api_url("http://micro.do").is_err());
    }

    #[test]
    fn validates_function_slugs() {
        assert!(validate_slug("hello-world").is_ok());
        assert!(validate_slug("api").is_err());
        assert!(validate_slug("Bad-Slug").is_err());
    }
}
