use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::process::Command;

fn format_issue_as_html(issue: &str) -> String {
    // Check if it's a PR issue
    if issue.starts_with("PR #") {
        if let Some(end_idx) = issue.find(" '") {
            let number = &issue[4..end_idx];
            return format!(
                "<li>{}</li>",
                issue.replace(
                    &format!("PR #{}", number),
                    &format!(
                        "<a href=\"https://github.com/figma/figma/pull/{}\">PR #{}</a>",
                        number, number
                    )
                )
            );
        }
    }

    // Check if it's a LaunchDarkly flag issue
    if issue.starts_with("Flag '") && issue.contains(" [") {
        // Extract the flag metadata: [project:key:env]
        if let Some(start) = issue.find(" [") {
            if let Some(end) = issue.find(']') {
                let metadata = &issue[start + 2..end];
                let parts: Vec<&str> = metadata.split(':').collect();
                if parts.len() == 3 {
                    let project_key = parts[0];
                    let flag_key = parts[1];
                    let env = parts[2];

                    let url = format!(
                        "https://app.launchdarkly.com/projects/{}/flags/{}/targeting?env=production&env=staging&selected-env={}",
                        project_key, flag_key, env
                    );

                    // Remove the metadata from the display text
                    let display_text = issue.replace(&format!(" [{}]", metadata), "");
                    return format!("<li><a href=\"{}\">{}</a></li>", url, display_text);
                }
            }
        }
    }

    // Default: no link
    format!("<li>{}</li>", issue)
}

fn generate_html(issues: &[String], issue_timestamps: &HashMap<String, DateTime<Utc>>) -> String {
    let issue_items: Vec<String> = issues
        .iter()
        .map(|issue| {
            let timestamp = issue_timestamps
                .get(issue)
                .map(|ts| ts.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            format!(
                r#"{} <span style="display:none" class="timestamp" data-issue="{}">{}</span>"#,
                format_issue_as_html(issue),
                html_escape::encode_text(issue),
                timestamp
            )
        })
        .collect();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Work Driver Issues</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
            max-width: 800px;
            margin: 40px auto;
            padding: 20px;
            line-height: 1.6;
        }}
        h1 {{
            color: #333;
            border-bottom: 2px solid #e1e4e8;
            padding-bottom: 10px;
        }}
        ul {{
            list-style-type: none;
            padding-left: 0;
        }}
        li {{
            padding: 10px;
            margin: 8px 0;
            background: #f6f8fa;
            border-radius: 6px;
            border-left: 4px solid #0969da;
        }}
        a {{
            color: #0969da;
            text-decoration: none;
        }}
        a:hover {{
            text-decoration: underline;
        }}
    </style>
</head>
<body>
    <h1>Work Driver Issues</h1>
    <ul>
        {}
    </ul>
</body>
</html>"#,
        issue_items.join("\n        ")
    )
}

fn parse_existing_timestamps(html_path: &str) -> HashMap<String, DateTime<Utc>> {
    let mut timestamps = HashMap::new();

    if let Ok(content) = fs::read_to_string(html_path) {
        // Parse timestamps from HTML
        for line in content.lines() {
            if line.contains(r#"class="timestamp""#) {
                // Extract data-issue and timestamp
                if let Some(issue_start) = line.find(r#"data-issue=""#) {
                    if let Some(issue_end) = line[issue_start + 12..].find('"') {
                        let issue = &line[issue_start + 12..issue_start + 12 + issue_end];

                        // Find the timestamp after the closing >
                        if let Some(ts_start) = line[issue_start..].find('>') {
                            if let Some(ts_end) = line[issue_start + ts_start + 1..].find('<') {
                                let timestamp_str = &line[issue_start + ts_start + 1..issue_start + ts_start + 1 + ts_end];
                                if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp_str) {
                                    timestamps.insert(issue.to_string(), dt.with_timezone(&Utc));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    timestamps
}

pub fn send_notification(summary: &str, detailed_issues: &[String]) -> Result<()> {
    let output_path = shellexpand::tilde("~/Desktop/work-driver-issues.html");

    // Parse existing timestamps
    let mut issue_timestamps = parse_existing_timestamps(output_path.as_ref());

    // Check which issues need notification (new or >19 minutes old)
    let now = Utc::now();
    let threshold = chrono::Duration::minutes(19);
    let mut needs_notification = false;
    let mut new_issues = Vec::new();

    for issue in detailed_issues {
        match issue_timestamps.get(issue) {
            Some(last_notified) => {
                if now.signed_duration_since(*last_notified) > threshold {
                    needs_notification = true;
                    new_issues.push(issue.clone());
                    // Update timestamp for re-notification
                    issue_timestamps.insert(issue.clone(), now);
                }
            }
            None => {
                // New issue
                needs_notification = true;
                new_issues.push(issue.clone());
                issue_timestamps.insert(issue.clone(), now);
            }
        }
    }

    // Always write the HTML file with updated timestamps
    let html_content = generate_html(detailed_issues, &issue_timestamps);
    fs::write(output_path.as_ref(), html_content).context("Failed to write issues to file")?;

    // Only send notification if there are new issues or issues past threshold
    if needs_notification && !new_issues.is_empty() {
        Command::new("terminal-notifier")
            .args([
                "-title",
                "Work Driver",
                "-message",
                summary,
                "-sound",
                "Blow",
                "-execute",
                &format!("open -a 'Google Chrome' {}", output_path),
            ])
            .output()
            .context("Failed to send notification")?;
    }

    Ok(())
}
