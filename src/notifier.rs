use crate::state::{load_state, save_state};
use anyhow::{Context, Result};
use chrono::Utc;
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
                        "<a href=\"https://github.com/figma/figma/pull/{}\" target=\"_blank\">PR #{}</a>",
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
                    return format!("<li><a href=\"{}\" target=\"_blank\">{}</a></li>", url, display_text);
                }
            }
        }
    }

    // Default: no link
    format!("<li>{}</li>", issue)
}

fn generate_html(unseen: &[String], seen: &[String]) -> String {
    let unseen_items: Vec<String> = unseen.iter().map(|i| format_issue_as_html(i)).collect();
    let seen_items: Vec<String> = seen.iter().map(|i| format_issue_as_html(i)).collect();

    let unseen_section = if unseen_items.is_empty() {
        r#"<p class="empty">All caught up!</p>"#.to_string()
    } else {
        format!(
            r#"<h2>Needs Attention ({})</h2>
    <ul class="unseen">
        {}
    </ul>"#,
            unseen_items.len(),
            unseen_items.join("\n        ")
        )
    };

    let seen_section = if seen_items.is_empty() {
        String::new()
    } else {
        format!(
            r#"<h2 class="seen-header">Recently Reviewed ({})</h2>
    <ul class="seen">
        {}
    </ul>"#,
            seen_items.len(),
            seen_items.join("\n        ")
        )
    };

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
        h2 {{
            color: #444;
            margin-top: 24px;
        }}
        .seen-header {{
            color: #888;
        }}
        .timer {{
            color: #666;
            font-size: 14px;
            margin-bottom: 16px;
        }}
        ul {{
            list-style-type: none;
            padding-left: 0;
        }}
        .unseen li {{
            padding: 10px;
            margin: 8px 0;
            background: #f6f8fa;
            border-radius: 6px;
            border-left: 4px solid #0969da;
            transition: opacity 0.3s, background 0.3s;
        }}
        .seen li {{
            padding: 10px;
            margin: 8px 0;
            background: #fafafa;
            border-radius: 6px;
            border-left: 4px solid #ccc;
            opacity: 0.6;
            transition: opacity 0.3s, background 0.3s;
        }}
        .empty {{
            color: #666;
            font-style: italic;
        }}
        a {{
            color: #0969da;
            text-decoration: none;
        }}
        a:hover {{
            text-decoration: underline;
        }}
        .seen a {{
            color: #666;
        }}
        li.marking-seen {{
            opacity: 0.3;
        }}
    </style>
</head>
<body>
    <h1>Work Driver Issues</h1>
    <div class="timer" id="timer"></div>
    {}
    {}
    <script>
    (function() {{
        // Countdown timer
        function updateTimer() {{
            fetch('/state')
                .then(r => r.json())
                .then(state => {{
                    if (state.last_check) {{
                        const lastCheck = new Date(state.last_check);
                        const nextCheck = new Date(lastCheck.getTime() + 5 * 60 * 1000);
                        const now = new Date();
                        const remaining = Math.max(0, nextCheck - now);
                        const minutes = Math.floor(remaining / 60000);
                        const el = document.getElementById('timer');
                        if (remaining <= 60000) {{
                            el.textContent = 'Next check in <1m';
                        }} else {{
                            el.textContent = 'Next check in ~' + minutes + 'm';
                        }}
                    }}
                }})
                .catch(() => {{}});
        }}
        updateTimer();
        setInterval(updateTimer, 5000);

        // Intercept link clicks to mark as seen
        document.addEventListener('click', function(e) {{
            const link = e.target.closest('a');
            if (!link) return;

            const li = link.closest('li');
            if (!li) return;

            e.preventDefault();

            // Extract issue text (strip HTML)
            const issueText = li.textContent.trim();

            // Visual feedback
            li.classList.add('marking-seen');

            // POST to server
            fetch('/seen', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ issue: issueText }})
            }}).catch(() => {{}});

            // Open the link
            window.open(link.href, '_blank');

            // Move to seen section after a brief delay
            setTimeout(function() {{
                const seenList = document.querySelector('ul.seen');
                if (seenList) {{
                    li.classList.remove('marking-seen');
                    seenList.appendChild(li);
                }}
            }}, 300);
        }});
    }})();
    </script>
</body>
</html>"#,
        unseen_section, seen_section
    )
}

pub fn update_html(issues: &[String]) -> Result<()> {
    let output_path = shellexpand::tilde("~/Desktop/work-driver-issues.html");

    let mut state = load_state().unwrap_or_default();
    let now = Utc::now();
    let seen_threshold = chrono::Duration::minutes(30);

    // Classify issues as seen or unseen
    let mut unseen_issues = Vec::new();
    let mut seen_issues = Vec::new();

    for issue in issues {
        let is_seen = state
            .seen
            .get(issue)
            .is_some_and(|ts| now.signed_duration_since(*ts) < seen_threshold);

        if is_seen {
            seen_issues.push(issue.clone());
        } else {
            unseen_issues.push(issue.clone());
        }
    }

    // Clean up stale entries from state
    let current_issues: std::collections::HashSet<&String> = issues.iter().collect();
    state
        .issue_timestamps
        .retain(|k, _| current_issues.contains(k));
    state.seen.retain(|k, _| current_issues.contains(k));

    // Update last_check
    state.last_check = Some(now);

    // Write HTML
    let html_content = generate_html(&unseen_issues, &seen_issues);
    fs::write(output_path.as_ref(), html_content).context("Failed to write issues to file")?;

    // Save state
    save_state(&state).context("Failed to save state")?;

    Ok(())
}

pub fn send_notification(summary: &str, detailed_issues: &[String]) -> Result<()> {
    let mut state = load_state().unwrap_or_default();
    let now = Utc::now();
    let seen_threshold = chrono::Duration::minutes(30);
    let notify_threshold = chrono::Duration::minutes(19);

    // Determine if we need to send a notification
    let mut needs_notification = false;
    for issue in detailed_issues {
        let is_seen = state
            .seen
            .get(issue)
            .is_some_and(|ts| now.signed_duration_since(*ts) < seen_threshold);
        if is_seen {
            continue;
        }

        match state.issue_timestamps.get(issue) {
            Some(last_notified) => {
                if now.signed_duration_since(*last_notified) > notify_threshold {
                    needs_notification = true;
                    state.issue_timestamps.insert(issue.clone(), now);
                }
            }
            None => {
                needs_notification = true;
                state.issue_timestamps.insert(issue.clone(), now);
            }
        }
    }

    if needs_notification {
        save_state(&state).context("Failed to save state")?;

        Command::new("terminal-notifier")
            .args([
                "-title",
                "Work Driver",
                "-message",
                summary,
                "-sound",
                "Blow",
                "-open",
                "http://localhost:9845/",
            ])
            .output()
            .context("Failed to send notification")?;
    }

    Ok(())
}
