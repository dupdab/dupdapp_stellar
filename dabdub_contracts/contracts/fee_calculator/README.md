# Fee Calculator Contract

## Purpose

`fee_calculator` computes protocol fees for transfers and other operations. It centralises fee logic so that dependent contracts (e.g. the token and escrow contracts) can query a single, consistent source of truth for fee amounts and fee recipients instead of duplicating the calculation.

## Storage

| Key | Type | Description |
| --- | --- | --- |
| `Admin` | `Address` | Address authorised to update fee configuration. |
| `FeeConfig` | `FeeConfig` | Current fee basis points, flat fee, and recipient configuration. |
| `FeeRecipient` | `Address` | Address that receives collected fees. |

Storage is instance-scoped; each deployed instance maintains its own configuration.

## Events

| Event | Emitted when |
| --- | --- |
| `fee_config_updated` | The admin updates the fee configuration. |
| `fee_recipient_updated` | The admin changes the fee recipient. |
| `fee_calculated` | A fee is computed for a caller (where emitted). |

## Admin operations

- `set_fee_config(admin, config)` — update fee basis points / flat fee. Admin-only.
- `set_fee_recipient(admin, recipient)` — change the address receiving fees. Admin-only.
- `get_fee_config()` — read the current configuration (public).
- `calculate_fee(amount)` — compute the fee for a given amount (public, read-only).

All admin operations require the stored `Admin` to authorise the call; unauthorised callers are rejected.

## Known limitations

- Fee configuration changes apply to subsequent calculations only; in-flight operations keep the fee computed at their start.
- No built-in bounds validation beyond what the setter enforces — misconfigured basis points can produce unexpectedly high fees.
- Rounding follows integer arithmetic; very small amounts may round fees to zero.

## Cross-references

- Consumed by the token contract for transfer fee calculation.
- Consumed by the escrow contract when settling fees on release.
- Admin authorisation follows the same pattern as `admin_auth`.
