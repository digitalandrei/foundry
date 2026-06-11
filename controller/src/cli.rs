//! Operator CLI: `foundry-controller instance add ...` — bootstrap an
//! instance before any admin can log in (the chicken-and-egg of
//! onboarding the very first GitLab instance). Runs on the controller
//! host with direct DB access; the client secret is read from the
//! FOUNDRY_INSTANCE_CLIENT_SECRET env var so it never lands in shell
//! history.

use std::collections::HashMap;

use crate::config::Config;
use crate::crypto::SecretBox;
use crate::repos::instances;

const USAGE: &str = "\
usage: foundry-controller instance add \\
         --name <display name> \\
         --base-url https://gitlab.example.com \\
         --registry-url https://registry.example.com
       (client id read from FOUNDRY_INSTANCE_CLIENT_ID or --client-id;
        client secret read from FOUNDRY_INSTANCE_CLIENT_SECRET)";

pub async fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 2 || args[0] != "instance" || args[1] != "add" {
        eprintln!("{USAGE}");
        return Err("unknown command".into());
    }

    let mut flags: HashMap<String, String> = HashMap::new();
    let mut it = args[2..].iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        flags.insert(flag.clone(), value.clone());
    }
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

    let config = Config::from_env()?;
    let secrets = SecretBox::from_base64_key(&config.encryption_key)?;
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await?;
    crate::MIGRATOR.run(&pool).await?;

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
