use anyhow::Result;
use work_driver::check::Check;
use work_driver::github::GitHubChecker;
use work_driver::launchdarkly::LaunchDarklyChecker;
use work_driver::notifier::{send_notification, update_html};
use work_driver::server::run_server;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "serve") {
        return run_server().await;
    }

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

    update_html(&all_issues)?;

    if !all_issues.is_empty() {
        send_notification(&all_issues)?;
        println!("{} issues found", all_issues.len());
    } else {
        println!("No issues found");
    }

    Ok(())
}
