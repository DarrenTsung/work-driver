use crate::check::Check;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LaunchDarklyFlag {
    key: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct LaunchDarklyFlagDetail {
    #[allow(dead_code)]
    key: String,
    name: String,
    kind: String,
    variations: Vec<Variation>,
    environments: std::collections::HashMap<String, Environment>,
}

#[derive(Debug, Deserialize)]
struct Variation {
    #[allow(dead_code)]
    #[serde(rename = "_id")]
    id: String,
    name: Option<String>,
    value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct Environment {
    #[serde(rename = "lastModified")]
    last_modified: Option<i64>,
    on: bool,
    fallthrough: Option<Fallthrough>,
}

#[derive(Debug, Deserialize)]
struct Fallthrough {
    rollout: Option<Rollout>,
}

#[derive(Debug, Deserialize)]
struct Rollout {
    variations: Vec<WeightedVariation>,
}

#[derive(Debug, Deserialize)]
struct WeightedVariation {
    variation: i32,
    weight: i32,
}

#[derive(Debug, Deserialize)]
struct LaunchDarklyResponse {
    items: Vec<LaunchDarklyFlag>,
}

pub struct LaunchDarklyChecker {
    api_token: String,
    maintainer_id: String,
    project_key: String,
}

impl LaunchDarklyChecker {
    pub fn new() -> Result<Self> {
        let api_token = std::env::var("LAUNCHDARKLY_API_TOKEN")
            .context("LAUNCHDARKLY_API_TOKEN environment variable not set")?;
        let maintainer_id = std::env::var("LAUNCHDARKLY_MAINTAINER_ID")
            .context("LAUNCHDARKLY_MAINTAINER_ID environment variable not set")?;
        let project_key =
            std::env::var("LAUNCHDARKLY_PROJECT_KEY").unwrap_or_else(|_| "default".to_string());

        Ok(Self {
            api_token,
            maintainer_id,
            project_key,
        })
    }
}

#[async_trait]
impl Check for LaunchDarklyChecker {
    async fn check(&self) -> Result<Vec<String>> {
        let client = reqwest::Client::new();

        // First, list all flags for this maintainer
        let list_url = format!(
            "https://app.launchdarkly.com/api/v2/flags/{}?filter=maintainerId:{}",
            self.project_key, self.maintainer_id
        );

        let response = client
            .get(&list_url)
            .header("Authorization", &self.api_token)
            .send()
            .await
            .context("Failed to fetch LaunchDarkly flags list")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "LaunchDarkly API returned error: {}",
                response.status()
            ));
        }

        let data: LaunchDarklyResponse = response
            .json()
            .await
            .context("Failed to parse LaunchDarkly response")?;

        let mut issues = Vec::new();
        let now = Utc::now().timestamp_millis();
        let two_hours_ago = now - (2 * 60 * 60 * 1000);
        let eighteen_hours_ago = now - (18 * 60 * 60 * 1000);

        // For each flag, fetch detailed info with staging and production environments
        for flag in data.items {
            let detail_url = format!(
                "https://app.launchdarkly.com/api/v2/flags/{}/{}",
                self.project_key, flag.key
            );

            let detail_response = client
                .get(&detail_url)
                .header("Authorization", &self.api_token)
                .send()
                .await
                .context("Failed to fetch flag details")?;

            if !detail_response.status().is_success() {
                eprintln!(
                    "Failed to fetch details for flag '{}': {}",
                    flag.name,
                    detail_response.status()
                );
                continue;
            }

            let mut flag_detail: LaunchDarklyFlagDetail = detail_response
                .json()
                .await
                .context("Failed to parse flag details")?;
            flag_detail
                .environments
                .retain(|env_name, _env| env_name == "staging" || env_name == "production");

            // Get rollout percentages for both environments
            let staging_rollout = flag_detail
                .environments
                .get("staging")
                .and_then(|env| get_rollout_percentage(&flag_detail, env));
            let production_rollout = flag_detail
                .environments
                .get("production")
                .and_then(|env| get_rollout_percentage(&flag_detail, env));

            // Check if staging is finished rolling out, but production isn't started
            if let (Some(staging), Some(production)) = (staging_rollout, production_rollout) {
                if staging >= 50.0 && production == 0.0 {
                    issues.push(format!(
                        "Flag '{}' [{}:{}:production] rolled out to {:.0}% in staging, but not started in production",
                        flag_detail.name, self.project_key, flag.key, staging
                    ));
                }
            }

            // Check each environment (staging and production) for stale partial rollouts
            for (env_name, env) in &flag_detail.environments {
                let Some(last_modified) = env.last_modified else {
                    continue;
                };

                let (time_threshold, time_str) = if env_name == "staging" {
                    (two_hours_ago, "2h")
                } else {
                    (eighteen_hours_ago, "18h")
                };

                let updated_recently = last_modified > time_threshold;
                if updated_recently {
                    continue;
                }

                let Some(rollout) = get_rollout_percentage(&flag_detail, env) else {
                    continue;
                };
                let threshold = if env_name == "staging" { 50.0 } else { 100.0 };
                if rollout > 0.0 && rollout < threshold {
                    issues.push(format!(
                        "Flag '{}' [{}:{}:{}] in {} at partial {:.0}% rollout, not updated in {}",
                        flag_detail.name,
                        self.project_key,
                        flag.key,
                        env_name,
                        env_name,
                        rollout,
                        time_str
                    ));
                }
            }
        }

        Ok(issues)
    }
}

fn get_rollout_percentage(flag: &LaunchDarklyFlagDetail, env: &Environment) -> Option<f64> {
    if !env.on {
        return Some(0.0);
    }

    // Only handle boolean flags
    if flag.kind != "boolean" {
        return None;
    }

    // Find the "enabled" variation index
    // First, try to find a variation with name "enabled"
    let enabled_index = flag
        .variations
        .iter()
        .position(|v| {
            if let Some(name) = &v.name {
                return name.to_lowercase() == "enabled";
            }
            false
        })
        // Fallback: find a variation with value true
        .or_else(|| {
            flag.variations
                .iter()
                .position(|v| v.value.as_bool() == Some(true))
        })?;

    env.fallthrough
        .as_ref()
        .and_then(|ft| ft.rollout.as_ref())
        .map(|rollout| {
            // Calculate percentage from weights
            let total_weight: i32 = rollout.variations.iter().map(|v| v.weight).sum();
            if total_weight == 0 {
                return 0.0;
            }

            // Get the weight for the "enabled" variation
            let on_weight = rollout
                .variations
                .iter()
                .find(|v| v.variation == enabled_index as i32)
                .map(|v| v.weight)
                .unwrap_or(0);

            (on_weight as f64 / total_weight as f64) * 100.0
        })
}
