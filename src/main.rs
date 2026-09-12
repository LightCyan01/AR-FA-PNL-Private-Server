use std::io::Read;
use std::{env, sync::Arc};

use argon2::{PasswordHasher, PasswordVerifier};
use atelier_private_server::{
    api::ApiService, assets::AssetService, config::LoadedConfig, state::State, storage::Store,
    transport::ProtoRegistry,
};
use hyper::service::service_fn;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use tokio::{net::TcpListener, signal};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("account") => account_command(args.collect()).await?,
        Some("serve") | None => serve(args.collect()).await?,
        Some(command) => return Err(format!("unknown command: {command}").into()),
    }
    Ok(())
}

async fn serve(args: Vec<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config_path = value_after(&args, "--config").unwrap_or_else(|| "config.toml".into());
    let config = Arc::new(LoadedConfig::load(config_path)?);
    init_logging(&config.config.logging.level)?;
    let proto = ProtoRegistry::from_file(&config.config.paths.protoset)?;
    let store = Store::open(&config.config.storage.path)?;
    let state = Arc::new(State::new(store, proto, (*config).clone())?);
    let assets = AssetService::load(config.clone()).await?;
    let service = ApiService::new(state, assets);
    let api_listener = TcpListener::bind(config.api_addr).await?;
    let asset_listener = TcpListener::bind(config.asset_addr).await?;
    tracing::info!(event = "ready", api = %config.api_addr, assets = %config.asset_addr, redacted = true);

    let api_task = run_listener(api_listener, service.clone(), false);
    let asset_task = run_listener(asset_listener, service, true);
    tokio::select! {
        result = api_task => result?,
        result = asset_task => result?,
        _ = shutdown_signal() => tracing::info!(event = "shutdown", redacted = true),
    }
    Ok(())
}

async fn run_listener(
    listener: TcpListener,
    service: ApiService,
    assets: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        let (stream, _) = listener.accept().await?;
        let service = service.clone();
        tokio::spawn(async move {
            let handler = service_fn(move |request| {
                let service = service.clone();
                async move {
                    Ok::<_, std::convert::Infallible>(if assets {
                        service.asset(request).await
                    } else {
                        service.handle(request).await
                    })
                }
            });
            if let Err(error) = auto::Builder::new(TokioExecutor::new())
                .http1_only()
                .serve_connection(TokioIo::new(stream), handler)
                .await
            {
                tracing::debug!(event = "connection_closed", error = %error, redacted = true);
            }
        });
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install signal handler");
        tokio::select! { _ = signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = signal::ctrl_c().await;
    }
}

async fn account_command(
    args: Vec<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config_path = value_after(&args, "--config").unwrap_or_else(|| "config.toml".into());
    let config = Arc::new(LoadedConfig::load(config_path)?);
    let store = Store::open(&config.config.storage.path)?;
    match args.first().map(String::as_str) {
        Some("create") => {
            let username = required_after(&args, "--username")?;
            let password = password_from_stdin(&args)?;
            validate_account_input(&username, &password)?;
            let salt = argon2::password_hash::SaltString::generate(&mut rand_core::OsRng);
            let hash = argon2::Argon2::default()
                .hash_password(password.as_bytes(), &salt)
                .map_err(|_| "password hashing failed")?
                .to_string();
            let _ = store.create_account(&username, &hash)?;
            println!("ACCOUNT_CREATED");
        }
        Some("login") => {
            let username = required_after(&args, "--username")?;
            let password = password_from_stdin(&args)?;
            let handoff = required_after(&args, "--handoff")?;
            let Some((account_id, encoded)) = store.account_by_username(&username)? else {
                return Err("invalid account credentials".into());
            };
            let parsed = argon2::password_hash::PasswordHash::new(&encoded)
                .map_err(|_| "invalid account credentials")?;
            argon2::Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .map_err(|_| "invalid account credentials")?;
            let grant = uuid::Uuid::new_v4().to_string();
            let now = unix_now();
            store.issue_grant(
                account_id,
                grant.as_bytes(),
                now,
                now + config.config.account.handoff_ttl_seconds as i64,
            )?;
            write_handoff(&handoff, &grant)?;
            println!("ACCOUNT_LOGIN_OK");
        }
        _ => {
            return Err(
                "usage: account create|login --config <path> --username <name> --password-stdin"
                    .into(),
            )
        }
    }
    Ok(())
}

fn password_from_stdin(
    args: &[String],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if !args.iter().any(|arg| arg == "--password-stdin") {
        return Err("password must be provided with --password-stdin".into());
    }
    let mut password = String::new();
    std::io::stdin().read_to_string(&mut password)?;
    if password.is_empty() {
        return Err("missing password on stdin".into());
    }
    Ok(password)
}

fn validate_account_input(
    username: &str,
    password: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !(3..=32).contains(&username.len())
        || !username.is_ascii()
        || username.chars().any(char::is_whitespace)
        || password.len() < 8
    {
        return Err("invalid account credentials".into());
    }
    Ok(())
}

fn write_handoff(path: &str, grant: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::io::ErrorKind;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err("handoff file already exists; remove it before retrying".into())
        }
        Err(error) => return Err(error.into()),
    };
    use std::io::Write;
    file.write_all(grant.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}
fn required_after(
    args: &[String],
    flag: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    value_after(args, flag).ok_or_else(|| format!("missing {flag}").into())
}
fn init_logging(level: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_new(level)?)
        .json()
        .try_init()?;
    Ok(())
}
