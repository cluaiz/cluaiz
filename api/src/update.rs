//! Is a newer Cluaiz release out? One request to the GitHub releases list, the same source the
//! installers read (the newest release, pre-releases included). `cluaiz self update` and the
//! `system.update_check` CEP method share it.

use serde::Serialize;
use std::time::Duration;

/// The GitHub repository the releases and the installers come from
pub const REPO: &str = "cluaiz/cluaiz";

#[derive(Debug, Clone, Serialize)]
pub struct Release {
    /// "0.1.2" (the tag without its `v`)
    pub version: String,
    pub tag: String,
    /// The release page
    pub url: String,
    pub prerelease: bool,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheck {
    /// This Engine's version
    pub current: String,
    pub latest: Release,
    pub update_available: bool,
}

/// The newest published release
pub async fn latest_release() -> Result<Release, String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("cluaiz/{}", cluaiz_core::VERSION))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=1");
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("GitHub could not be reached ({error})"))?;
    let status = response.status();
    let body = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("GitHub returned HTTP {status} for {url}"));
    }
    parse_releases(&body)
}

/// The first release of a GitHub releases list
pub fn parse_releases(body: &str) -> Result<Release, String> {
    let list: serde_json::Value = serde_json::from_str(body).map_err(|error| format!("unreadable release list: {error}"))?;
    let release = list.get(0).ok_or_else(|| format!("no release is published on github.com/{REPO}"))?;
    let tag = release["tag_name"].as_str().ok_or("the release has no tag")?.to_string();
    Ok(Release {
        version: tag.trim_start_matches('v').to_string(),
        url: release["html_url"].as_str().unwrap_or_default().to_string(),
        prerelease: release["prerelease"].as_bool().unwrap_or(false),
        published_at: release["published_at"].as_str().map(str::to_string),
        tag,
    })
}

/// This Engine against the newest release
pub async fn check() -> Result<UpdateCheck, String> {
    let latest = latest_release().await?;
    Ok(UpdateCheck {
        current: cluaiz_core::VERSION.to_string(),
        update_available: is_newer(&latest.version, cluaiz_core::VERSION),
        latest,
    })
}

/// True when `latest` is a newer release than `current`: x.y.z compared as numbers, and at the same
/// x.y.z a final release is newer than a pre-release ("0.2.0" > "0.2.0-rc1"). Anything that is not
/// a version ("dev-release") is never newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    fn parse(version: &str) -> Option<(Vec<u64>, bool)> {
        let version = version.trim().trim_start_matches('v');
        let (numbers, pre) = match version.split_once(['-', '+']) {
            Some((numbers, rest)) => (numbers, version.as_bytes()[numbers.len()] == b'-' && !rest.is_empty()),
            None => (version, false),
        };
        let numbers = numbers.split('.').map(|part| part.parse().ok()).collect::<Option<Vec<u64>>>()?;
        Some((numbers, pre))
    }
    match (parse(latest), parse(current)) {
        (Some((latest, latest_pre)), Some((current, current_pre))) => {
            latest > current || (latest == current && current_pre && !latest_pre)
        }
        _ => false,
    }
}
