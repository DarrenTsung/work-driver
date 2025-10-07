use anyhow::Result;
use work_driver::check::Check;
use work_driver::github::GitHubChecker;
use work_driver::launchdarkly::LaunchDarklyChecker;
use work_driver::notifier::send_notification;

#[tokio::main]
async fn main() -> Result<()> {
    let checkers: Vec<Box<dyn Check>> = vec![
        Box::new(GitHubChecker::new()),
        Box::new(LaunchDarklyChecker::new()?),
    ];

    let mut all_issues = Vec::new();

    for checker in checkers {
        match checker.check().await {
            Ok(issues) => all_issues.extend(issues),
            Err(e) => eprintln!("Error running check: {}", e),
        }
    }

    if !all_issues.is_empty() {
        // Count PR and flag issues
        let pr_count = all_issues.iter().filter(|s| s.starts_with("PR #")).count();
        let flag_count = all_issues.iter().filter(|s| s.starts_with("Flag ")).count();

        // Generate concise summary
        let summary = match (pr_count, flag_count) {
            (0, f) => format!("{} flag{} waiting", f, if f == 1 { "" } else { "s" }),
            (p, 0) => format!("{} PR{} need attention", p, if p == 1 { "" } else { "s" }),
            (p, f) => format!(
                "{} PR{} and {} flag{} need attention",
                p,
                if p == 1 { "" } else { "s" },
                f,
                if f == 1 { "" } else { "s" }
            ),
        };

        send_notification(&summary, &all_issues)?;
        println!("Sent notification: {}", summary);
    } else {
        println!("No issues found");
    }

    Ok(())
}
