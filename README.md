# Hook Me Up

Send GitHub activity to Discord. Hook Me Up posts a compact embed when a pull request
is opened, updated, merged, or closed, or when an issue is opened, edited, closed, or
reopened. The small Rust binary runs from a GitHub Actions workflow, reads the event at
`GITHUB_EVENT_PATH`, and POSTs to a Discord webhook.

Message format:

```
OPENED · #42 Fix login redirect
Short overview of the fix.
Reviewers: reviewer, backend
PR by author
```

The card title links to the PR. Opened PRs show the first non-empty, non-heading line
of the PR body, capped at 240 characters, and outstanding requested users or teams.
Edited PRs identify the title, description, or target-branch changes; a new summary is
shown only when the description changed. Pushes, merges, and unmerged closures keep to
the title and author. Discord timestamps each card, so the PR creation date is omitted.

Each event has a distinct color and an explicit text label: opened (green), changes
pushed (blue), edited (amber), merged (purple), and closed (gray). Mentions are inert
(`allowed_mentions.parse` is empty). Embed titles, reviewer fields, and footer text are
capped to Discord's 256, 1,024, and 2,048-character limits.

## Use it in your repo

1. Create a Discord webhook for the target channel.
2. In your repo, add an environment named `discord-notify` with a secret
   `DISCORD_WEBHOOK_URL`.
3. Copy [`example.yml`](example.yml) to `.github/workflows/notify-discord.yml` in
   your repository. Remove either trigger if you only want pull-request or issue
   notifications.

```yaml
on:
  pull_request_target:
    types: [opened, synchronize, edited, closed]
    branches: [develop, main]
  issues:
    types: [opened, edited, closed, reopened]
```

`pull_request_target` runs the workflow from your default branch with access to the
secret; the PR's own code is never checked out or executed. Public repositories may
need to explicitly allow this event in their Actions policy; see GitHub's
[`pull_request_target` security guidance](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target).

Normal repository issues still emit issue events when they are shown on a GitHub
Project board. Project-only draft items and board status changes are not repository
issue events.

`ref: main` tracks the latest version of this notifier — pin a tag or commit SHA to
control upgrades.

## Development

```sh
cargo test
cargo run   # needs GITHUB_EVENT_PATH and DISCORD_WEBHOOK_URL
```
