use serde_json::{Value, json};
use std::{env, error::Error, fs, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let event: Value = serde_json::from_slice(&fs::read(env::var("GITHUB_EVENT_PATH")?)?)?;
    let content = message(&event)?;
    let webhook = env::var("DISCORD_WEBHOOK_URL")?;
    let client = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent();

    match client
        .post(format!("{webhook}?wait=true"))
        .send_json(json!({
            "content": content,
            "allowed_mentions": { "parse": [] },
        })) {
        Ok(_) => Ok(()),
        Err(ureq::Error::StatusCode(code)) => Err(format!("Discord returned HTTP {code}").into()),
        Err(_) => Err("Discord request failed".into()),
    }
}

fn message(event: &Value) -> Result<String, Box<dyn Error>> {
    let pr = &event["pull_request"];
    let event_label = match event["action"].as_str() {
        Some("opened") => "OPENED",
        Some("synchronize") => "CHANGES PUSHED",
        Some("edited") => "EDITED",
        Some("closed") => match pr["merged"].as_bool() {
            Some(true) => "MERGED",
            Some(false) => "CLOSED",
            None => return Err("missing merged status for closed PR".into()),
        },
        _ => return Err("expected a supported pull request event".into()),
    };
    let number = pr["number"].as_u64().ok_or("missing PR number")?;
    let title = pr["title"].as_str().ok_or("missing PR title")?;
    let url = pr["html_url"].as_str().ok_or("missing PR URL")?;
    let author = pr["user"]["login"].as_str().ok_or("missing PR author")?;
    let created = pr["created_at"]
        .as_str()
        .ok_or("missing PR creation date")?;
    let date = created.get(..10).ok_or("invalid PR creation date")?;
    let description = pr["body"]
        .as_str()
        .unwrap_or("")
        .lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}').trim())
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("No description provided.")
        .chars()
        .take(240)
        .collect::<String>();
    let reviewers = requested_reviewers(pr);

    let after = format!(
        " #{number}]({url})\n**DESCRIPTION**: {description}\n**AUTHOR**: {author}\n**REVIEWER**: {reviewers}\n**DATE**: {date}"
    );
    let title_space = 2000usize
        .checked_sub(format!("**EVENT**: {event_label}\n**PR**: [").len() + after.chars().count())
        .ok_or("PR metadata exceeds Discord message limit")?;
    // Discord ignores backslash escapes inside a masked-link label, so `\[` would show the
    // backslash and a bare `]` would end the link early: swap brackets for parentheses.
    let linked_title: String = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .map(|ch| match ch {
            '[' => '(',
            ']' => ')',
            other => other,
        })
        .take(title_space)
        .collect();
    Ok(format!(
        "**EVENT**: {event_label}\n**PR**: [{linked_title}{after}"
    ))
}

fn requested_reviewers(pr: &Value) -> String {
    let reviewers = pr["requested_reviewers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|reviewer| reviewer["login"].as_str())
        .chain(
            pr["requested_teams"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|team| team["slug"].as_str()),
        )
        .collect::<Vec<_>>();

    if reviewers.is_empty() {
        "N/A".into()
    } else {
        reviewers.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_template_with_a_short_description() {
        let event = json!({
            "action": "opened",
            "pull_request": {
                "number": 42,
                "title": "Fix [login]\nredirect",
                "html_url": "https://github.com/itlogsandwich/test-repo/pull/42",
                "body": "## Summary\n\u{feff}Short overview of the fix.\nMore details follow.",
                "created_at": "2026-10-09T00:27:23Z",
                "user": { "login": "author" },
                "requested_reviewers": [{ "login": "reviewer" }],
                "requested_teams": [{ "slug": "backend" }]
            }
        });

        let content = message(&event).unwrap();
        assert_eq!(
            content,
            "**EVENT**: OPENED\n**PR**: [Fix (login) redirect #42](https://github.com/itlogsandwich/test-repo/pull/42)\n**DESCRIPTION**: Short overview of the fix.\n**AUTHOR**: author\n**REVIEWER**: reviewer, backend\n**DATE**: 2026-10-09"
        );
    }

    #[test]
    fn handles_missing_body_and_caps_long_title() {
        let event = json!({
            "action": "opened",
            "pull_request": {
                "number": 42,
                "title": format!("@everyone {}", "x".repeat(2500)),
                "html_url": "https://github.com/itlogsandwich/test-repo/pull/42",
                "body": null,
                "created_at": "2026-10-09T00:27:23Z",
                "user": { "login": "author" }
            }
        });

        let content = message(&event).unwrap();
        assert!(content.starts_with("**EVENT**: OPENED\n**PR**: [@everyone "));
        assert!(content.contains(" #42](https://github.com/itlogsandwich/test-repo/pull/42)"));
        assert!(content.ends_with(
            "\n**DESCRIPTION**: No description provided.\n**AUTHOR**: author\n**REVIEWER**: N/A\n**DATE**: 2026-10-09"
        ));
        assert_eq!(content.chars().count(), 2000);
    }

    #[test]
    fn labels_updated_and_closed_prs() {
        for (action, merged, label) in [
            ("synchronize", None, "CHANGES PUSHED"),
            ("edited", None, "EDITED"),
            ("closed", Some(true), "MERGED"),
            ("closed", Some(false), "CLOSED"),
        ] {
            let event = json!({
                "action": action,
                "pull_request": {
                    "number": 42,
                    "title": "Title",
                    "html_url": "https://github.com/owner/repo/pull/42",
                    "body": "Description",
                    "created_at": "2026-10-09T00:27:23Z",
                    "user": { "login": "author" },
                    "merged": merged
                }
            });

            assert!(
                message(&event)
                    .unwrap()
                    .starts_with(&format!("**EVENT**: {label}\n"))
            );
        }
    }

    #[test]
    fn rejects_closed_pr_without_merged_status() {
        let event = json!({ "action": "closed", "pull_request": {} });

        assert_eq!(
            message(&event).unwrap_err().to_string(),
            "missing merged status for closed PR"
        );
    }
}
