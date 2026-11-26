# Payment Escrow Contract

The Payment Escrow contract holds funds in escrow between a payer and a merchant until the payment is released, refunded, or a dispute is resolved.

## Dispute Resolution Flow

The following sequence diagram illustrates how a dispute is raised and resolved, including resolution to the merchant, resolution to the customer, and automatic resolution when the admin review window times out.

```mermaid
sequenceDiagram
    autonumber
    actor Customer
    actor Merchant
    actor Admin
    participant Escrow as Payment Escrow Contract

    Customer->>Escrow: raise_dispute(payment_id, reason)
    activate Escrow
    Escrow->>Escrow: mark payment as Disputed
    Escrow->>Escrow: start admin review window (timeout)
    Escrow-->>Customer: dispute recorded
    Escrow-->>Merchant: dispute recorded
    deactivate Escrow

    Note over Admin,Escrow: Admin reviews the dispute
    Admin->>Escrow: review_dispute(payment_id)
    activate Escrow
    Escrow-->>Admin: dispute details
    deactivate Escrow

    alt Resolved to merchant
        Admin->>Escrow: resolve_dispute(payment_id, Resolution::Merchant)
        activate Escrow
        Escrow->>Escrow: release funds to merchant
        Escrow-->>Merchant: funds released
        Escrow-->>Customer: dispute resolved (merchant)
        deactivate Escrow
    else Resolved to customer
        Admin->>Escrow: resolve_dispute(payment_id, Resolution::Customer)
        activate Escrow
        Escrow->>Escrow: refund funds to customer
        Escrow-->>Customer: funds refunded
        Escrow-->>Merchant: dispute resolved (customer)
        deactivate Escrow
    else Auto-resolved on timeout
        Note over Escrow: Admin review window elapses
        Escrow->>Escrow: auto_resolve_dispute(payment_id)
        activate Escrow
        Escrow->>Escrow: apply default resolution
        Escrow-->>Customer: dispute auto-resolved
        Escrow-->>Merchant: dispute auto-resolved
        deactivate Escrow
    end
```

## Deadline Passed Event

When a dispute's resolution deadline (`dispute_resolution_deadline` ledger) has been reached or passed, anyone may call the permissionless `auto_resolve_dispute(payment_id)` entrypoint. This entrypoint panics if the deadline has not yet passed, so it only succeeds for genuinely expired disputes.

On success it emits a `DeadlinePassed` event so the backend Soroban event indexer can detect expired disputes and trigger auto-resolution without polling every escrow. The event carries enough context to identify the expired dispute:

| Field | Description |
| --- | --- |
| `escrow_id` / `payment_id` | Identifier of the escrow whose dispute expired |
| `dispute_id` | Identifier of the expired dispute |
| `deadline` | The dispute resolution deadline ledger that was passed |

Indexers should subscribe to the `DeadlinePassed` topic and action the referenced dispute accordingly.
