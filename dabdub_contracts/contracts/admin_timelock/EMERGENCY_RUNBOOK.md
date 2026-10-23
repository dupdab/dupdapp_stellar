# Emergency Runbook: admin_timelock

## Overview
The `admin_timelock` contract gates all privileged parameter changes across the Stellar platform.

## Incident: Malicious/Erroneous Scheduled Change

### Symptoms
- A scheduled change has been proposed that would lock out users or drain funds
- The change is scheduled but not yet executed

### Response Steps

1. **Verify the pending change**
```bash
soroban contract invoke \
  --id ADMIN_TIMELOCK_CONTRACT_ID \
  -- get_pending_changes
```

2. **Assess urgency** - check time until `execute_after`

3. **Initiate cancellation** - if admin key available:
```bash
soroban contract invoke \
  --id ADMIN_TIMELOCK_CONTRACT_ID \
  --source ADMIN_KEYPAIR \
  -- cancel_change \
  --change_id CHANGE_ID
```

4. **Verify cancellation** - re-query to confirm

### Prevention
- Keep admin key in secure hardware wallet
- Require multisig for admin operations where possible
- Monitor all scheduled changes via event logs
- Use short `ledgers_to_lock` values (1-2 hours)

## Incident: `ESCROW.emergency_drain` Event

### Symptoms
- Grafana alert `payment_escrow: emergency_drain` fires (pages PagerDuty + Slack `#security`)
- A Loki log line contains an `ESCROW.emergency_drain` event on `payment_escrow`

### Grafana Alert Rule

The alert rule below triggers on any `ESCROW.emergency_drain` event in Loki, evaluates every 30s with a 30s `for` duration so it fires within 1 minute, and routes to PagerDuty and Slack `#security`.

```yaml
apiVersion: 1
groups:
  - orgId: 1
    name: payment_escrow-security
    folder: Security
    interval: 30s
    rules:
      - uid: escrow-emergency-drain
        title: ESCROW.emergency_drain detected
        condition: A
        data:
          - refId: A
            relativeTimeRange:
              from: 120
              to: 0
            datasourceUid: loki
            model:
              expr: 'count_over_time({job="payment_escrow"} |= "ESCROW.emergency_drain" [1m])'
              queryType: instant
        noDataState: OK
        execErrState: Alerting
        for: 30s
        annotations:
          summary: 'ESCROW.emergency_drain event detected on payment_escrow'
          description: 'A critical emergency_drain event was emitted. Investigate immediately.'
          runbook_url: 'https://github.com/dabdub/contracts/blob/main/dabdub_contracts/contracts/admin_timelock/EMERGENCY_RUNBOOK.md#incident-escrowemergency_drain-event'
        labels:
          severity: critical
          team: security
        notification_settings:
          receiver: pagerduty-security
```

### Notification Routing

Configure the contact points and notification policy so the alert pages PagerDuty and posts to Slack `#security`:

```yaml
apiVersion: 1
contactPoints:
  - orgId: 1
    name: pagerduty-security
    receivers:
      - uid: pagerduty-security
        type: pagerduty
        settings:
          integrationKey: $PAGERDUTY_INTEGRATION_KEY
          severity: critical
      - uid: slack-security
        type: slack
        settings:
          recipient: '#security'
          token: $SLACK_TOKEN
          url: $SLACK_WEBHOOK_URL

policies:
  - orgId: 1
    receiver: pagerduty-security
    group_by: ['alertname']
    routes:
      - receiver: pagerduty-security
        matchers:
          - severity = critical
        continue: true
      - receiver: slack-security
        matchers:
          - severity = critical
```

### Response Steps

1. **Acknowledge the page** in PagerDuty and open the `#security` Slack thread.

2. **Confirm the event** in Loki:
```logql
{contract="payment_escrow"} |= "ESCROW.emergency_drain"
```

3. **Identify the caller** - inspect the event payload for the invoking address and
   correlate with the escrow contract's admin/operator keys.

4. **Freeze further activity** - if the drain was not authorized, pause escrow
   operations and rotate any exposed operator keys.

5. **Escalate** - notify the security lead and, if funds moved, begin the
   incident-response fund-tracing process.

### Prevention
- Keep escrow operator keys in secure hardware wallets / multisig
- Alert on any `ESCROW.emergency_drain` event (see Grafana rule `payment_escrow_emergency_drain`)
- Review drain authorization policy regularly
