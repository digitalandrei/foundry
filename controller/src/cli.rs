//! Operator CLI, run on the controller host with direct DB access:
//!
//! - `instance add` — bootstrap a GitLab instance before any admin can
//!   log in (secret via FOUNDRY_INSTANCE_CLIENT_SECRET, never argv).
//! - `admin add` / `admin set-password` — local operator accounts
//!   (password via FOUNDRY_ADMIN_PASSWORD, or generated and printed).

use std::collections::HashMap;

use crate::config::Config;
use crate::crypto::SecretBox;
use crate::repos::{instances, local_admins};

const USAGE: &str = "\
usage: foundry-controller instance add \\
         --name <display name> \\
         --base-url https://gitlab.example.com \\
         --registry-url https://registry.example.com
       (client id read from FOUNDRY_INSTANCE_CLIENT_ID or --client-id;
        client secret read from FOUNDRY_INSTANCE_CLIENT_SECRET)

       foundry-controller admin add --username <name> [--name <display>]
       foundry-controller admin set-password --username <name>
       (password read from FOUNDRY_ADMIN_PASSWORD; generated and
        printed when unset)";

fn parse_flags(args: &[String]) -> Result<HashMap<String, String>, String> {
    let mut flags = HashMap::new();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        flags.insert(flag.clone(), value.clone());
    }
    Ok(flags)
}

async fn open_pool() -> Result<(Config, sqlx::MySqlPool), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await?;
    crate::MIGRATOR.run(&pool).await?;
    Ok((config, pool))
}

pub async fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    match (
        args.first().map(String::as_str),
        args.get(1).map(String::as_str),
    ) {
        (Some("instance"), Some("add")) => instance_add(&args[2..]).await,
        (Some("admin"), Some("add")) => admin_add(&args[2..]).await,
        (Some("admin"), Some("set-password")) => admin_set_password(&args[2..]).await,
        _ => {
            eprintln!("{USAGE}");
            Err("unknown command".into())
        }
    }
}

async fn instance_add(rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let flags = parse_flags(rest)?;
    let get = |k: &str, env: &str| -> Result<String, String> {
        flags
            .get(k)
            .cloned()
            .or_else(|| std::env::var(env).ok())
            .ok_or_else(|| format!("missing {k} (or {env})\n{USAGE}"))
    };

    let name = get("--name", "FOUNDRY_INSTANCE_NAME")?;
    let base_url =
        instances::normalize_url(&get("--base-url", "FOUNDRY_INSTANCE_BASE_URL")?, "base_url")?;
    let registry_url = instances::normalize_url(
        &get("--registry-url", "FOUNDRY_INSTANCE_REGISTRY_URL")?,
        "registry_url",
    )?;
    let client_id = get("--client-id", "FOUNDRY_INSTANCE_CLIENT_ID")?;
    let client_secret = std::env::var("FOUNDRY_INSTANCE_CLIENT_SECRET")
        .map_err(|_| "FOUNDRY_INSTANCE_CLIENT_SECRET is not set")?;

    let (config, pool) = open_pool().await?;
    let secrets = SecretBox::from_base64_key(&config.encryption_key)?;

    let id = instances::insert(
        &pool,
        &secrets,
        instances::NewInstance {
            name: &name,
            base_url: &base_url,
            registry_url: &registry_url,
            oauth_client_id: client_id.trim(),
            oauth_client_secret: client_secret.trim(),
        },
        None,
        None,
    )
    .await?;

    println!("instance onboarded: {name} ({id})");
    println!();
    println!("GitLab OAuth application checklist for {base_url}:");
    println!("  Redirect URI : {}/auth/callback", config.public_url);
    println!("  Scopes       : openid profile email read_api read_registry");
    println!("  Confidential : yes");
    Ok(())
}

/// FOUNDRY_ADMIN_PASSWORD, or a generated one (printed by the caller).
fn password_from_env() -> (String, bool) {
    match std::env::var("FOUNDRY_ADMIN_PASSWORD") {
        Ok(p) if !p.trim().is_empty() => (p, false),
        _ => (crate::crypto::random_token(), true),
    }
}

async fn admin_add(rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let flags = parse_flags(rest)?;
    let username = flags
        .get("--username")
        .ok_or(format!("missing --username\n{USAGE}"))?
        .trim()
        .to_string();
    if username.is_empty() || username.len() > 64 {
        return Err("username must be 1-64 characters".into());
    }
    let display = flags
        .get("--name")
        .cloned()
        .unwrap_or_else(|| username.clone());

    let (password, generated) = password_from_env();
    let (_, pool) = open_pool().await?;
    let user_id = local_admins::create(&pool, &username, &display, &password).await?;

    println!("local admin created: {username} ({user_id})");
    if generated {
        println!("generated password: {password}");
        println!("(rotate any time: foundry-controller admin set-password --username {username})");
    }
    Ok(())
}

async fn admin_set_password(rest: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let flags = parse_flags(rest)?;
    let username = flags
        .get("--username")
        .ok_or(format!("missing --username\n{USAGE}"))?
        .trim()
        .to_string();

    let (password, generated) = password_from_env();
    let (_, pool) = open_pool().await?;
    local_admins::set_password(&pool, &username, &password).await?;

    println!("password updated for {username}");
    if generated {
        println!("generated password: {password}");
    }
    Ok(())
}
