//! `cluaiz provider`: the model providers in providers.json (services with an OpenAI-compatible API).
//! A provider's model is then used as `<provider>/<model>`: `cluaiz agent -m myapi/model-name`.

use clap::Subcommand;
use cluaiz_core::providers::{self, Provider, ProviderModel, Providers, Target};
use color_eyre::{eyre::eyre, Result};
use colored::Colorize;
use std::io::Read;

#[derive(Subcommand)]
pub enum ProviderCommand {
    /// Add a provider, or change one: its URL and where its key comes from
    Add {
        /// Name, used as the first part of its model names (`myapi` → `myapi/<model>`)
        name: String,
        /// OpenAI-compatible base URL, such as https://api.example.com/v1 or http://localhost:11434/v1 (Ollama)
        #[arg(long)]
        url: String,
        /// Environment variable that holds the key, such as MYAPI_KEY (the key stays out of the file)
        #[arg(long = "key-env", value_name = "VAR", conflicts_with = "key")]
        key_env: Option<String>,
        /// The key itself, kept in providers.json; `-` reads it from stdin
        #[arg(long, value_name = "KEY")]
        key: Option<String>,
        /// A model to list, with its context size: `model-name` or `model-name=32768`
        #[arg(long = "model", value_name = "ID[=CONTEXT]")]
        models: Vec<String>,
    },
    /// List the providers
    #[command(alias = "ls")]
    List,
    /// Remove a provider
    #[command(alias = "rm")]
    Remove { name: String },
    /// The models a provider offers, with their context size
    Models {
        name: String,
        /// Only models whose id contains this text
        filter: Option<String>,
    },
    /// Send one short request: the URL, the key and a model work
    Test {
        name: String,
        /// The model to try (default: the first listed one)
        model: Option<String>,
    },
}

pub async fn execute(command: ProviderCommand) -> Result<()> {
    let mut all = Providers::load().map_err(|e| eyre!(e))?;
    match command {
        ProviderCommand::Add { name, url, key_env, key, models } => {
            if !providers::valid_name(&name) {
                return Err(eyre!("'{name}': a provider name has letters, digits, - and _ only"));
            }
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(eyre!("'{url}': the URL starts with http:// or https://"));
            }
            let mut provider = all.providers.get(&name).cloned().unwrap_or_else(|| Provider::new(&url));
            provider.base_url = url.trim().trim_end_matches('/').to_string();
            if let Some(var) = key_env {
                provider.api_key_env = Some(var);
                provider.api_key = None;
            }
            if let Some(key) = key {
                let key = if key == "-" {
                    let mut read = String::new();
                    std::io::stdin().read_to_string(&mut read)?;
                    read.trim().to_string()
                } else {
                    key
                };
                provider.api_key = Some(key).filter(|k| !k.is_empty());
                provider.api_key_env = None;
            }
            for model in models {
                let (id, context) = match model.rsplit_once('=') {
                    Some((id, n)) => (id.to_string(), Some(n.parse::<u32>().map_err(|_| eyre!("'{model}': the context is a number of tokens"))?)),
                    None => (model, None),
                };
                provider.models.insert(id, ProviderModel { context });
            }
            all.providers.insert(name.clone(), provider.clone());
            all.save().map_err(|e| eyre!(e))?;
            println!("{} {name} ({}), key: {}", "Saved".green().bold(), provider.base_url, provider.key_source());
            println!("{}", format!("Use its models as {name}/<model>; see them with `cluaiz provider models {name}`.").dimmed());
            warn_if_open();
        }
        ProviderCommand::List => {
            if all.providers.is_empty() {
                println!("No providers. Add one: cluaiz provider add myapi --url https://api.example.com/v1 --key-env MYAPI_KEY");
                return Ok(());
            }
            for (name, provider) in &all.providers {
                println!("{}  {}  key: {}", name.bold(), provider.base_url, provider.key_source());
                for (model, info) in &provider.models {
                    let context = info.context.map(|n| format!("  context {n}")).unwrap_or_default();
                    println!("    {name}/{model}{}", context.dimmed());
                }
            }
            println!("{}", format!("File: {}", Providers::path().display()).dimmed());
            warn_if_open();
        }
        ProviderCommand::Remove { name } => {
            if all.providers.remove(&name).is_none() {
                return Err(eyre!("no provider '{name}' (see `cluaiz provider list`)"));
            }
            all.save().map_err(|e| eyre!(e))?;
            println!("Removed {name}.");
        }
        ProviderCommand::Models { name, filter } => {
            let provider = all.providers.get(&name).ok_or_else(|| eyre!("no provider '{name}' (see `cluaiz provider list`)"))?;
            let offered = cluaiz_api::providers::models(&name, provider).await.map_err(|e| eyre!(e))?;
            let shown: Vec<_> = offered.iter().filter(|m| filter.as_deref().is_none_or(|f| m.id.to_lowercase().contains(&f.to_lowercase()))).collect();
            for model in &shown {
                let context = model.context.map(|n| format!("  context {n}")).unwrap_or_default();
                println!("{name}/{}{}", model.id, context.dimmed());
            }
            println!("{}", format!("{} of {} models", shown.len(), offered.len()).dimmed());
        }
        ProviderCommand::Test { name, model } => {
            let provider = all.providers.get(&name).ok_or_else(|| eyre!("no provider '{name}' (see `cluaiz provider list`)"))?;
            let model = model
                .or_else(|| provider.models.keys().next().cloned())
                .ok_or_else(|| eyre!("name a model: cluaiz provider test {name} <model> (see `cluaiz provider models {name}`)"))?;
            let target = Target { provider: name.clone(), config: provider.clone(), model };
            let started = std::time::Instant::now();
            let answer = cluaiz_api::providers::test(&target).await.map_err(|e| eyre!(e))?;
            println!("{} {} answered in {:.1} s: {}", "OK".green().bold(), target.full_name(), started.elapsed().as_secs_f64(), answer);
        }
    }
    Ok(())
}

/// The model's context size from the provider's model list, saved in providers.json, when the
/// file does not have it yet. Quiet when the list cannot be read: the default context is used.
pub async fn with_context(mut target: Target) -> Target {
    if target.config.models.get(&target.model).is_some_and(|m| m.context.is_some()) {
        return target;
    }
    let listed = tokio::time::timeout(std::time::Duration::from_secs(10), cluaiz_api::providers::models(&target.provider, &target.config)).await;
    let Some(context) = listed.ok().and_then(Result::ok).and_then(|models| models.into_iter().find(|m| m.id == target.model)?.context) else {
        return target;
    };
    target.config.models.insert(target.model.clone(), ProviderModel { context: Some(context) });
    if let Ok(mut all) = Providers::load() {
        if let Some(provider) = all.providers.get_mut(&target.provider) {
            provider.models.insert(target.model.clone(), ProviderModel { context: Some(context) });
            let _ = all.save();
        }
    }
    target
}

fn warn_if_open() {
    let required = cluaiz_security::PermissionSchema::load().api_auth.required;
    if let Some(warning) = cluaiz_api::providers::open_server_warning(required) {
        eprintln!("{} {warning}", "Note:".yellow().bold());
    }
}
