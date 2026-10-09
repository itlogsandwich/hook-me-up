# pr-notify-dc

Posts a compact Discord embed when a pull request is opened, updated, merged, or closed. A small Rust binary run from
a GitHub Actions workflow: it reads the event at `GITHUB_EVENT_PATH` and POSTs to a
Discord webhook.

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
3. Add `.github/workflows/notify-pr.yml`:

```yaml
name: Notify Discord on PR

on:
  pull_request_target:
    types: [opened, synchronize, edited, closed]
    branches: [develop, main]

permissions:
  contents: read

jobs:
  notify:
    if: github.event.pull_request.user.login != 'dependabot[bot]'
    runs-on: ubuntu-24.04
    environment:
      name: discord-notify
      deployment: false
    steps:
      # Checks out the notifier, never the pull request's code: the job holds the
      # webhook secret. Pin `ref` to a tag or SHA if you want releases, not latest.
      - uses: actions/checkout@v7
        with:
          repository: itlogsandwich/pr-notify-dc
          ref: main
          persist-credentials: false
      - run: cargo run --locked --quiet
        env:
          DISCORD_WEBHOOK_URL: ${{ secrets.DISCORD_WEBHOOK_URL }}
```

`pull_request_target` runs the workflow from your default branch with access to the
secret; the PR's own code is never checked out or executed. `ref: main` tracks the
latest version of this notifier — pin a tag or commit SHA instead if you want to
control upgrades.

## Development

```sh
cargo test
cargo run   # needs GITHUB_EVENT_PATH and DISCORD_WEBHOOK_URL
```
