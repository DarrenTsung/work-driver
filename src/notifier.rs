use anyhow::{Context, Result};
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

fn generate_html(issues: &[String]) -> String {
    let issue_items: Vec<String> = issues
        .iter()
        .map(|issue| format_issue_as_html(issue))
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

pub fn send_notification(summary: &str, detailed_issues: &[String]) -> Result<()> {
    // Write detailed issues to HTML file
    let output_path = shellexpand::tilde("~/Desktop/work-driver-issues.html");
    let html_content = generate_html(detailed_issues);
    fs::write(output_path.as_ref(), html_content).context("Failed to write issues to file")?;

    // Send notification with -execute to open the file when clicked
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

    Ok(())
}
