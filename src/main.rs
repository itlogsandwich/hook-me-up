use serde_json::{Value, json};
use std::{env, error::Error, fs, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let event: Value = serde_json::from_slice(&fs::read(env::var("GITHUB_EVENT_PATH")?)?)?;
    let payload = payload(&event)?;
    let webhook = env::var("DISCORD_WEBHOOK_URL")?;
    let client = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent();

    match client
        .post(format!("{webhook}?wait=true"))
        .send_json(payload)
    {
        Ok(_) => Ok(()),
        Err(ureq::Error::StatusCode(code)) => Err(format!("Discord returned HTTP {code}").into()),
        Err(_) => Err("Discord request failed".into()),
    }
}

fn payload(event: &Value) -> Result<Value, Box<dyn Error>> {
    if event.get("issue").is_some() {
        return issue_payload(event);
    }

    let pr = &event["pull_request"];
    let (event_label, color) = match event["action"].as_str() {
        Some("opened") => ("OPENED", 0x238636),
        Some("synchronize") => ("CHANGES PUSHED", 0x58a6ff),
        Some("edited") => ("EDITED", 0xd29922),
        Some("closed") => match pr["merged"].as_bool() {
            Some(true) => ("MERGED", 0x8957e5),
            Some(false) => ("CLOSED", 0x8b949e),
            None => return Err("missing merged status for closed PR".into()),
        },
        _ => return Err("expected a supported pull request event".into()),
    };
    let number = pr["number"].as_u64().ok_or("missing PR number")?;
    let title = pr["title"].as_str().ok_or("missing PR title")?;
    let url = pr["html_url"].as_str().ok_or("missing PR URL")?;
    let author = pr["user"]["login"].as_str().ok_or("missing PR author")?;
    let title = truncate(
        &format!("{event_label} · #{number} {}", single_line(title)),
        256,
    );
    let mut embed = json!({
        "title": title,
        "url": url,
        "color": color,
        "footer": { "text": truncate(&format!("PR by {author}"), 2048) },
    });
    let mut description = Vec::new();

    match event["action"].as_str() {
        Some("opened") => {
            if let Some(summary) = summary(pr) {
                description.push(summary);
            }
            if let Some(reviewers) = requested_reviewers(pr) {
                embed["fields"] = json!([{
                    "name": "Reviewers",
                    "value": truncate(&reviewers, 1024),
                    "inline": true,
                }]);
            }
        }
        Some("edited") => {
            description.push(edit_summary(event, "PR"));
            if event["changes"].get("body").is_some()
                && let Some(summary) = summary(pr)
            {
                description.push(summary);
            }
        }
        _ => {}
    }

    if !description.is_empty() {
        embed["description"] = Value::String(description.join("\n"));
    }

    Ok(json!({
        "embeds": [embed],
        "allowed_mentions": { "parse": [] },
    }))
}

fn issue_payload(event: &Value) -> Result<Value, Box<dyn Error>> {
    let issue = &event["issue"];
    let (event_label, color) = match event["action"].as_str() {
        Some("opened") => ("ISSUE OPENED", 0x238636),
        Some("edited") => ("ISSUE EDITED", 0xd29922),
        Some("closed") => ("ISSUE CLOSED", 0x8b949e),
        Some("reopened") => ("ISSUE REOPENED", 0x58a6ff),
        _ => return Err("expected a supported issue event".into()),
    };
    let number = issue["number"].as_u64().ok_or("missing issue number")?;
    let title = issue["title"].as_str().ok_or("missing issue title")?;
    let url = issue["html_url"].as_str().ok_or("missing issue URL")?;
    let author = issue["user"]["login"]
        .as_str()
        .ok_or("missing issue author")?;
    let mut embed = json!({
        "title": truncate(&format!("{event_label} · #{number} {}", single_line(title)), 256),
        "url": url,
        "color": color,
        "footer": { "text": truncate(&format!("Issue by {author}"), 2048) },
    });

    let mut description = Vec::new();
    match event["action"].as_str() {
        Some("opened") => {
            if let Some(summary) = summary(issue) {
                description.push(summary);
            }
        }
        Some("edited") => {
            description.push(edit_summary(event, "Issue"));
            if event["changes"].get("body").is_some()
                && let Some(summary) = summary(issue)
            {
                description.push(summary);
            }
        }
        _ => {}
    }

    if !description.is_empty() {
        embed["description"] = Value::String(description.join("\n"));
    }

    Ok(json!({
        "embeds": [embed],
        "allowed_mentions": { "parse": [] },
    }))
}

fn summary(pr: &Value) -> Option<String> {
    pr["body"]
        .as_str()?
        .lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}').trim())
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| truncate(line, 240))
}

fn edit_summary(event: &Value, subject: &str) -> String {
    let changes = &event["changes"];
    let mut changed = Vec::new();

    if changes.get("title").is_some() {
        changed.push("Title changed");
    }
    if changes.get("body").is_some() {
        changed.push("Description changed");
    }
    if changes.get("base").is_some() {
        changed.push("Target branch changed");
    }

    if changed.is_empty() {
        format!("{subject} details changed")
    } else {
        changed.join(" · ")
    }
}

fn requested_reviewers(pr: &Value) -> Option<String> {
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

    (!reviewers.is_empty()).then(|| reviewers.join(", "))
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(action: &str) -> Value {
        json!({
            "action": action,
            "pull_request": {
                "number": 42,
                "title": "Fix [login]\nredirect",
                "html_url": "https://github.com/itlogsandwich/test-repo/pull/42",
                "body": "## Summary\n\u{feff}Short overview of the fix.\nMore details follow.",
                "user": { "login": "author" }
            }
        })
    }

    #[test]
    fn opened_pr_is_a_compact_embed() {
        let mut event = pr("opened");
        event["pull_request"]["requested_reviewers"] = json!([{ "login": "reviewer" }]);
        event["pull_request"]["requested_teams"] = json!([{ "slug": "backend" }]);

        assert_eq!(
            payload(&event).unwrap(),
            json!({
                "embeds": [{
                    "title": "OPENED · #42 Fix [login] redirect",
                    "url": "https://github.com/itlogsandwich/test-repo/pull/42",
                    "color": 0x238636,
                    "footer": { "text": "PR by author" },
                    "description": "Short overview of the fix.",
                    "fields": [{ "name": "Reviewers", "value": "reviewer, backend", "inline": true }]
                }],
                "allowed_mentions": { "parse": [] }
            })
        );
    }

    #[test]
    fn edited_pr_describes_only_the_changed_details() {
        let mut event = pr("edited");
        event["changes"] = json!({ "title": { "from": "Old" }, "body": { "from": "Old body" } });

        let embed = &payload(&event).unwrap()["embeds"][0];
        assert_eq!(
            embed["description"],
            "Title changed · Description changed\nShort overview of the fix."
        );
        assert!(embed.get("fields").is_none());

        event["changes"] = json!({ "base": { "ref": { "from": "develop" } } });
        let embed = &payload(&event).unwrap()["embeds"][0];
        assert_eq!(embed["description"], "Target branch changed");
    }

    #[test]
    fn labels_and_colors_lifecycle_cards() {
        for (action, merged, label, color) in [
            ("synchronize", None, "CHANGES PUSHED", 0x58a6ff),
            ("edited", None, "EDITED", 0xd29922),
            ("closed", Some(true), "MERGED", 0x8957e5),
            ("closed", Some(false), "CLOSED", 0x8b949e),
        ] {
            let mut event = pr(action);
            event["pull_request"]["merged"] = json!(merged);
            let embed = &payload(&event).unwrap()["embeds"][0];

            assert!(embed["title"].as_str().unwrap().starts_with(label));
            assert_eq!(embed["color"], color);
        }
    }

    #[test]
    fn caps_embed_fields_and_omits_empty_optional_content() {
        let mut event = pr("opened");
        event["pull_request"]["title"] = json!(format!("@everyone {}", "x".repeat(500)));
        event["pull_request"]["body"] = Value::Null;
        event["pull_request"]["requested_reviewers"] = json!([{ "login": "r".repeat(2000) }]);

        let output = payload(&event).unwrap();
        let embed = &output["embeds"][0];
        assert_eq!(embed["title"].as_str().unwrap().chars().count(), 256);
        assert!(embed.get("description").is_none());
        assert_eq!(
            embed["fields"][0]["value"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            1024
        );
        assert_eq!(output["allowed_mentions"], json!({ "parse": [] }));
    }

    #[test]
    fn rejects_closed_pr_without_merged_status() {
        let event = json!({ "action": "closed", "pull_request": {} });

        assert_eq!(
            payload(&event).unwrap_err().to_string(),
            "missing merged status for closed PR"
        );
    }

    fn issue(action: &str) -> Value {
        json!({
            "action": action,
            "issue": {
                "number": 7,
                "title": "Fix issue\nnotifications",
                "html_url": "https://github.com/itlogsandwich/hook-me-up/issues/7",
                "body": "## Context\nNotify Discord when this changes.",
                "user": { "login": "reporter" }
            }
        })
    }

    #[test]
    fn issue_lifecycle_uses_compact_embeds() {
        for (action, label, color, description) in [
            (
                "opened",
                "ISSUE OPENED",
                0x238636,
                Some("Notify Discord when this changes."),
            ),
            (
                "edited",
                "ISSUE EDITED",
                0xd29922,
                Some("Issue details changed"),
            ),
            ("closed", "ISSUE CLOSED", 0x8b949e, None),
            ("reopened", "ISSUE REOPENED", 0x58a6ff, None),
        ] {
            let output = payload(&issue(action)).unwrap();
            let embed = &output["embeds"][0];

            assert_eq!(
                embed["title"],
                format!("{label} · #7 Fix issue notifications")
            );
            assert_eq!(embed["color"], color);
            assert_eq!(embed["footer"]["text"], "Issue by reporter");
            assert_eq!(embed["description"].as_str(), description);
            assert_eq!(output["allowed_mentions"], json!({ "parse": [] }));
        }
    }

    #[test]
    fn edited_issue_describes_changed_details() {
        let mut event = issue("edited");
        event["changes"] = json!({ "title": { "from": "Old" }, "body": { "from": "Old body" } });

        assert_eq!(
            payload(&event).unwrap()["embeds"][0]["description"],
            "Title changed · Description changed\nNotify Discord when this changes."
        );
    }

    #[test]
    fn rejects_unsupported_issue_events() {
        assert_eq!(
            payload(&issue("labeled")).unwrap_err().to_string(),
            "expected a supported issue event"
        );
    }
}
