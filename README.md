# pr-notify-dc

Posts a Discord message when a pull request is opened, updated, merged, or closed. A small Rust binary run from
a GitHub Actions workflow: it reads the event at `GITHUB_EVENT_PATH` and POSTs to a
Discord webhook.

Message format:

```
**EVENT**: OPENED
**PR**: [Fix login redirect #42](https://github.com/you/repo/pull/42)
**DESCRIPTION**: Short overview of the fix.
**AUTHOR**: author
**REVIEWER**: reviewer, backend
**DATE**: 2026-10-09
```

The description is the first non-empty, non-heading line of the PR body, capped at
240 characters. `REVIEWER` lists outstanding requested users and teams, or `N/A`.
Mentions are inert (`allowed_mentions.parse` is empty), and the whole message is
trimmed to Discord's 2000-character limit.

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
