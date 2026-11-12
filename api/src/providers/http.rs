//! Requests to a provider: one shared client, and the key sent the way that provider takes it
//! (a header, a Google or Microsoft Entra token, or an AWS signature).
//! The key goes only to the provider's own URL, in its auth header, never into a log.

use std::time::Duration;

use cluaiz_core::providers::{Provider, Target};

use super::compat::{Compat, Wire};

/// The Anthropic API version sent with Anthropic-format requests (not on Vertex AI, which takes
/// `anthropic_version` in the body). `headers` in providers.json can set another.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";

/// One client for every provider call: it keeps connections open between requests
pub fn client() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .user_agent(concat!("cluaiz/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default()
        })
        .clone()
}

/// How a provider takes its key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// `Authorization: Bearer <key>` (OpenAI and most others, Bedrock API keys on its OpenAI URL)
    Bearer,
    /// `api-key: <key>` (Azure OpenAI and Microsoft Foundry keys)
    ApiKey,
    /// `x-api-key: <key>` (Anthropic, Claude in Amazon Bedrock)
    XApiKey,
    /// A Google Cloud access token (Vertex AI); no key in providers.json
    Google,
    /// AWS Signature Version 4 with the AWS credentials (Amazon Bedrock without an API key)
    Aws,
    /// A Microsoft Entra ID token as `Authorization: Bearer` (Azure without an API key)
    Azure,
    /// Nothing (a server on this computer)
    None,
}

impl Auth {
    /// providers.json's `auth`, else told from the base URL and the request format
    pub fn of(provider: &Provider) -> Self {
        match provider
            .auth
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("bearer") => return Auth::Bearer,
            Some("api-key") | Some("api_key") => return Auth::ApiKey,
            Some("x-api-key") | Some("x_api_key") => return Auth::XApiKey,
            Some("google") | Some("gcloud") => return Auth::Google,
            Some("aws") | Some("sigv4") => return Auth::Aws,
            Some("azure") | Some("entra") => return Auth::Azure,
            Some("none") => return Auth::None,
            _ => {}
        }
        let url = provider.base_url.to_ascii_lowercase();
        // A platform's own sign-in is used when no key is set for it
        let has_key = provider
            .api_key_env
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty())
            || provider
                .api_key
                .as_deref()
                .is_some_and(|k| !k.trim().is_empty());
        let aws = url.contains(".amazonaws.com") || url.contains(".api.aws");
        let azure = url.contains(".openai.azure.com")
            || url.contains(".cognitiveservices.azure.com")
            || url.contains(".services.ai.azure.com");
        if url.contains("aiplatform.googleapis.com") {
            Auth::Google
        } else if aws && !has_key {
            Auth::Aws
        } else if azure {
            if has_key {
                Auth::ApiKey
            } else {
                Auth::Azure
            }
        } else if Compat::wire_of(provider) == Wire::Anthropic {
            Auth::XApiKey
        } else {
            Auth::Bearer
        }
    }
}

/// A request to the provider with its key (or Google access token), the Anthropic version where
/// it belongs, and providers.json's `headers`. Returns the key too, so messages can hide it.
pub async fn request(
    provider: &Provider,
    method: reqwest::Method,
    url: &str,
) -> Result<(reqwest::RequestBuilder, Option<String>), String> {
    let auth = Auth::of(provider);
    let key = match auth {
        Auth::Google => Some(super::google::access_token().await?),
        Auth::Azure => Some(super::azure::access_token().await?),
        Auth::Aws => {
            // Checked here, so a missing sign-in is said before anything is sent; `send` signs
            let host = reqwest::Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_default();
            if super::aws::scope_of(&host).is_none() {
                return Err(format!("{host} is not an Amazon Bedrock host: the AWS region to sign for cannot be told; use a Bedrock API key with `auth: \"bearer\"`"));
            }
            super::aws::credentials().await?;
            None
        }
        Auth::None => None,
        _ => provider.key()?,
    };
    let mut builder = client().request(method, url);
    if let Some(key) = &key {
        builder = match auth {
            Auth::ApiKey => builder.header("api-key", key),
            Auth::XApiKey => builder.header("x-api-key", key),
            Auth::None | Auth::Aws => builder,
            Auth::Bearer | Auth::Google | Auth::Azure => builder.bearer_auth(key),
        };
    }
    let anthropic = Compat::wire_of(provider) == Wire::Anthropic && !Compat::is_vertex(provider);
    if anthropic
        && !provider
            .headers
            .keys()
            .any(|h| h.eq_ignore_ascii_case("anthropic-version"))
    {
        builder = builder.header("anthropic-version", ANTHROPIC_VERSION);
    }
    for (name, value) in &provider.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    Ok((builder, key))
}

/// Sends a request made by `request`: signed first when the provider takes AWS signatures
pub async fn send(
    builder: reqwest::RequestBuilder,
    provider: &Provider,
) -> reqwest::Result<reqwest::Response> {
    let mut request = builder.build()?;
    if Auth::of(provider) == Auth::Aws {
        match super::aws::credentials().await {
            Ok(credentials) => {
                if let Err(e) =
                    super::aws::sign(&mut request, &credentials, std::time::SystemTime::now())
                {
                    tracing::warn!("⚠️ [Provider] the request could not be signed: {e}");
                }
            }
            Err(e) => tracing::warn!("⚠️ [Provider] no AWS credentials to sign with: {e}"),
        }
    }
    client().execute(request).await
}

/// A streamed chat request: JSON in, server-sent events out, no time limit (a long answer may
/// take hours; the caller stops it)
pub(super) async fn chat_request(
    target: &Target,
) -> Result<(reqwest::RequestBuilder, Option<String>), String> {
    let url = super::compat::chat_url(target, true);
    let (builder, key) = request(&target.config, reqwest::Method::POST, &url).await?;
    let builder = builder
        .timeout(Duration::from_secs(24 * 3600))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "text/event-stream");
    Ok((builder, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(url: &str) -> Provider {
        Provider::new(url)
    }

    #[test]
    fn each_provider_gets_its_key_its_way() {
        assert_eq!(
            Auth::of(&provider("https://api.groq.com/openai/v1")),
            Auth::Bearer
        );
        let keyed = |url: &str| {
            let mut p = provider(url);
            p.api_key_env = Some("SOME_KEY".into());
            p
        };
        // Bedrock and Azure: their key when one is set, else their sign-in
        assert_eq!(
            Auth::of(&keyed(
                "https://bedrock-runtime.us-east-1.amazonaws.com/openai/v1"
            )),
            Auth::Bearer
        );
        assert_eq!(
            Auth::of(&provider(
                "https://bedrock-runtime.us-east-1.amazonaws.com/openai/v1"
            )),
            Auth::Aws
        );
        assert_eq!(
            Auth::of(&keyed(
                "https://bedrock-mantle.us-east-1.api.aws/anthropic/v1"
            )),
            Auth::XApiKey
        );
        assert_eq!(
            Auth::of(&provider(
                "https://bedrock-mantle.us-east-1.api.aws/anthropic/v1"
            )),
            Auth::Aws
        );
        assert_eq!(
            Auth::of(&keyed("https://myres.openai.azure.com/openai/v1")),
            Auth::ApiKey
        );
        assert_eq!(
            Auth::of(&keyed("https://myres.services.ai.azure.com/openai/v1")),
            Auth::ApiKey
        );
        assert_eq!(
            Auth::of(&provider("https://myres.openai.azure.com/openai/v1")),
            Auth::Azure
        );
        assert_eq!(
            Auth::of(&provider("https://api.anthropic.com/v1")),
            Auth::XApiKey
        );
        assert_eq!(Auth::of(&provider("https://aiplatform.googleapis.com/v1/projects/p/locations/global/endpoints/openapi")), Auth::Google);
        assert_eq!(
            Auth::of(&provider(
                "https://aiplatform.googleapis.com/v1/projects/p/locations/global"
            )),
            Auth::Google
        );
        let mut set = provider("https://proxy.example.com/v1");
        set.auth = Some("x-api-key".into());
        assert_eq!(Auth::of(&set), Auth::XApiKey);
        set.auth = Some("aws".into());
        assert_eq!(Auth::of(&set), Auth::Aws);
        set.auth = Some("entra".into());
        assert_eq!(Auth::of(&set), Auth::Azure);
    }

    #[tokio::test]
    async fn headers_carry_the_key_the_version_and_the_files_own() {
        let mut anthropic = provider("https://api.anthropic.com/v1");
        anthropic.api_key = Some("sk-test".into());
        anthropic
            .headers
            .insert("anthropic-beta".into(), "some-beta".into());
        let (builder, key) = request(
            &anthropic,
            reqwest::Method::POST,
            "https://api.anthropic.com/v1/messages",
        )
        .await
        .unwrap();
        let built = builder.build().unwrap();
        assert_eq!(key.as_deref(), Some("sk-test"));
        assert_eq!(built.headers()["x-api-key"], "sk-test");
        assert!(built.headers().get("authorization").is_none());
        assert_eq!(built.headers()["anthropic-version"], ANTHROPIC_VERSION);
        assert_eq!(built.headers()["anthropic-beta"], "some-beta");

        let mut azure = provider("https://myres.openai.azure.com/openai/v1");
        azure.api_key = Some("az-key".into());
        let built = request(
            &azure,
            reqwest::Method::POST,
            "https://myres.openai.azure.com/openai/v1/chat/completions",
        )
        .await
        .unwrap()
        .0
        .build()
        .unwrap();
        assert_eq!(built.headers()["api-key"], "az-key");
        assert!(built.headers().get("anthropic-version").is_none());

        let mut groq = provider("https://api.groq.com/openai/v1");
        groq.api_key = Some("gsk".into());
        let built = request(
            &groq,
            reqwest::Method::POST,
            "https://api.groq.com/openai/v1/chat/completions",
        )
        .await
        .unwrap()
        .0
        .build()
        .unwrap();
        assert_eq!(built.headers()["authorization"], "Bearer gsk");
    }
}
