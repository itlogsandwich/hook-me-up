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
    if event["action"].as_str() != Some("opened") {
        return Err("expected an opened pull request event".into());
    }

    let pr = &event["pull_request"];
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

    let after = format!(
        " #{number}]({url})\n**DESCRIPTION**: {description}\n**AUTHOR**: {author}\n**DATE**: {date}"
    );
    let title_space = 2000usize
        .checked_sub("**PR**: [".len() + after.chars().count())
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
    Ok(format!("**PR**: [{linked_title}{after}"))
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
                "user": { "login": "author" }
            }
        });

        let content = message(&event).unwrap();
        assert_eq!(
            content,
            "**PR**: [Fix (login) redirect #42](https://github.com/itlogsandwich/test-repo/pull/42)\n**DESCRIPTION**: Short overview of the fix.\n**AUTHOR**: author\n**DATE**: 2026-10-09"
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
        assert!(content.starts_with("**PR**: [@everyone "));
        assert!(content.contains(" #42](https://github.com/itlogsandwich/test-repo/pull/42)"));
        assert!(content.ends_with(
            "\n**DESCRIPTION**: No description provided.\n**AUTHOR**: author\n**DATE**: 2026-10-09"
        ));
        assert_eq!(content.chars().count(), 2000);
    }
}
