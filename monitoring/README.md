# CheesePay Uptime Monitoring

External uptime monitoring for `api.cheesepay.xyz` and Stellar Horizon connectivity.
Alerts fire within **2 minutes** of downtime; on-call is notified within **5 minutes**.

---

## Architecture

```
Internet
  │
  ├── UptimeRobot (primary, 1-min polling)
  │     ├── Monitor: GET https://api.cheesepay.xyz/health          → email + Slack
  │     ├── Monitor: GET https://api.cheesepay.xyz/health/ready    → email + Slack
  │     └── Status page: status.cheesepay.xyz (public)
  │
  └── Better Uptime (secondary / on-call escalation, 30-sec polling)
        ├── Monitor: GET https://api.cheesepay.xyz/health
        ├── Monitor: GET https://api.cheesepay.xyz/health/ready    (checks Stellar Horizon)
        └── On-call rotation → PagerDuty / phone call within 5 min
```

The `/health` endpoint returns `{ "status": "ok" }` (always 200 while the process is alive).
The `/health/ready` endpoint checks **PostgreSQL + Stellar Horizon** and returns 503 if either is down — this is the critical monitor.

---

## Monitors to Configure

| Monitor | URL | Method | Expected Status | Interval | Alert |
|---------|-----|--------|----------------|----------|-------|
| API Liveness | `https://api.cheesepay.xyz/health` | GET | 200 | 1 min | Email + Slack |
| API Readiness (DB + Stellar) | `https://api.cheesepay.xyz/health/ready` | GET | 200 | 1 min | Email + Slack + PagerDuty |
| Stellar Horizon | `https://horizon.stellar.org/` | GET | 200 | 5 min | Slack |

---

## Soroban Contract Event Lag

The payment volume dashboard (`grafana/dashboards/payment-volume.json`) tracks how far
the Soroban event indexer has fallen behind the chain head via the
`soroban_event_lag_ledgers` metric.

- **Metric:** `soroban_event_lag_ledgers` — number of ledgers between the last indexed
  Soroban contract event and the current ledger.
- **Panel:** "Soroban Event Lag" shows the last indexed ledger vs. the current ledger
  (and the derived lag) so operators can see indexing progress at a glance.
- **Alert:** fires when `soroban_event_lag_ledgers > 10` for the configured window,
  indicating the indexer is falling behind and payment events may be delayed.

---

## Setup Guides

- [UptimeRobot Setup](./uptimerobot.md)
- [Better Uptime Setup](./better-uptime.md)
- [Status Page](./statuspage/README.md)

---

## Acceptance Criteria Verification

| Criterion | How it's met |
|-----------|-------------|
| Downtime detected within 2 min | UptimeRobot 1-min interval + Better Uptime 30-sec interval |
| On-call alerted within 5 min | Better Uptime phone/SMS escalation after 1 failed check |
| Status page shows real-time component status | `status.cheesepay.xyz` polls `/health/admin` every 60s |
