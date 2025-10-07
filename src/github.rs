use crate::check::Check;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::process::Command;

pub struct GitHubChecker;

impl GitHubChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn check_output(&self, github_pr_status_output: &str) -> Result<Vec<String>> {
        let data: serde_json::Value = serde_json::from_str(github_pr_status_output)?;

        let mut issues = Vec::new();

        // Check created PRs (only if checks are failing)
        if let Some(created) = data.get("createdBy").and_then(|v| v.as_array()) {
            for pr in created {
                if let Some(checks) = pr.get("statusCheckRollup").and_then(|v| v.as_array()) {
                    let has_failures = checks.iter().any(|check| {
                        check.get("state").and_then(|s| s.as_str()) == Some("FAILURE")
                            || check.get("conclusion").and_then(|s| s.as_str()) == Some("FAILURE")
                    });

                    if has_failures {
                        let title = pr
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown PR");
                        issues.push(format!(
                            "PR #{} '{}' has failing checks",
                            pr.get("number").and_then(|v| v.as_u64()).unwrap_or(0),
                            title
                        ));
                    }
                }
            }
        }

        // Check PRs requesting review from us (all should create an issue)
        if let Some(needs_review) = data.get("needsReview").and_then(|v| v.as_array()) {
            for pr in needs_review {
                let title = pr
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown PR");
                let number = pr.get("number").and_then(|v| v.as_u64()).unwrap_or(0);

                issues.push(format!(
                    "PR #{} '{}' awaiting your review",
                    number,
                    title
                ));
            }
        }

        Ok(issues)
    }
}

#[async_trait]
impl Check for GitHubChecker {
    async fn check(&self) -> Result<Vec<String>> {
        let output = Command::new("gh")
            .args([
                "pr",
                "status",
                "--json",
                "number,title,state,statusCheckRollup,reviewDecision",
            ])
            .output()
            .context("Failed to execute gh pr status")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("gh pr status failed"));
        }

        let stdout = String::from_utf8(output.stdout)?;
        self.check_output(&stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_checker() {
        const EXPECTED_OUTPUT: &str = include_str!("github/check_output_1.txt");
        const TEST_JSON: &str = include_str!("github/check_output_1.json");

        let checker = GitHubChecker::new();
        let issues = checker.check_output(TEST_JSON).unwrap();

        // Based on check_output_1.txt:
        // - PR #591209 (Created by you) - checks passing, so should NOT appear
        // - PR #591746 (Requesting review) - should appear
        // - PR #591547 (Requesting review) - should appear
        // - PR #590962 (Requesting review) - should appear
        assert_eq!(issues.len(), 3, "Expected 3 issues, got: {:#?}", issues);

        // All issues should be from needsReview
        assert!(
            issues.iter().any(|i| i.contains("#591746") && i.contains("awaiting your review")),
            "Expected PR #591746 awaiting review. Got: {:#?}\n\nExpected output:\n{}",
            issues,
            EXPECTED_OUTPUT
        );

        assert!(
            issues.iter().any(|i| i.contains("#591547") && i.contains("awaiting your review")),
            "Expected PR #591547 awaiting review. Got: {:#?}\n\nExpected output:\n{}",
            issues,
            EXPECTED_OUTPUT
        );

        assert!(
            issues.iter().any(|i| i.contains("#590962") && i.contains("awaiting your review")),
            "Expected PR #590962 awaiting review. Got: {:#?}\n\nExpected output:\n{}",
            issues,
            EXPECTED_OUTPUT
        );

        // PR #591209 should NOT appear (checks passing)
        assert!(
            !issues.iter().any(|i| i.contains("#591209")),
            "PR #591209 should not appear (checks passing). Got: {:#?}\n\nExpected output:\n{}",
            issues,
            EXPECTED_OUTPUT
        );
    }
}
