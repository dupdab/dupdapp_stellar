# Monitoring & Uptime

This directory holds the status page and uptime-monitoring configuration for
DupDub. It is **not** part of the local Grafana observability stack (see
[`grafana/`](../grafana/)); it is the configuration used to publish a public
status page and to drive external uptime checks.

## Contents

| Path | Purpose |
|---|---|
| `status-page/` | Static status page assets served to users |
| `uptime/` | Uptime monitor definitions (endpoints, intervals, alerts) |

## Environment variables

All values are read from the environment at deploy time. Never commit real
secrets — provide them via your CI/CD secret store or a local `.env` file that
is git-ignored.

| Variable | Required | Description |
|---|---|---|
| `STATUS_PAGE_URL` | yes | Public URL the status page is served from (e.g. `https://status.dupdub.example`) |
| `STATUS_PAGE_TITLE` | no | Display title for the status page (defaults to `DupDub Status`) |
| `UPTIME_PROVIDER` | yes | Uptime provider to integrate with: `betteruptime` or `uptimerobot` |
| `UPTIME_API_KEY` | yes | API token for the chosen provider (Better Uptime API token or UptimeRobot main API key) |
| `UPTIME_MONITOR_GROUP` | no | Provider group/folder name to place created monitors in |
| `ALERT_WEBHOOK_URL` | no | Webhook URL that receives alert notifications (Slack, PagerDuty, etc.) |
| `MONITORED_ENDPOINTS` | yes | Comma-separated list of URLs to monitor (e.g. `https://api.dupdub.example/health`) |
| `CHECK_INTERVAL_SECONDS` | no | Poll interval for monitors (default `60`) |

## Deploying the status page

The status page is static. Build and publish it with your usual static host
(S3 + CloudFront, Netlify, Vercel, GitHub Pages, …):

1. Set the environment variables above in the deploy environment.
2. Publish the contents of `status-page/` to your static host.
3. Point `STATUS_PAGE_URL` at the resulting host and verify it loads.

## Uptime integration

Monitors are declared in `uptime/` and created against the provider selected by
`UPTIME_PROVIDER`. Each entry in `MONITORED_ENDPOINTS` becomes one HTTP monitor
that polls the URL every `CHECK_INTERVAL_SECONDS` and alerts on non-2xx
responses or timeouts.

### Better Uptime

1. Create an API token in **Better Uptime → Settings → API tokens** and export
   it as `UPTIME_API_KEY`.
2. Set `UPTIME_PROVIDER=betteruptime`.
3. Apply the monitors in `uptime/` (they are pushed to the Better Uptime API
   using the token above).
4. Optionally set `ALERT_WEBHOOK_URL` to receive alert events; Better Uptime
   also supports email/Slack integrations configured in its dashboard.

### UptimeRobot

1. Copy the **Main API key** from **UptimeRobot → My Settings → API Settings**
   and export it as `UPTIME_API_KEY`.
2. Set `UPTIME_PROVIDER=uptimerobot`.
3. Apply the monitors in `uptime/` (created via the UptimeRobot `newMonitor`
   API using the key above).
4. Set `ALERT_WEBHOOK_URL` to a UptimeRobot alert contact webhook if you want
   alerts forwarded to your incident tooling.

## Verifying

After deploying, confirm each URL in `MONITORED_ENDPOINTS` shows as **up** in
the provider dashboard and that the status page reflects the same state.
