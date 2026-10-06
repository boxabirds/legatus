# Price and budget design (archived)

> **ARCHIVED DESIGN. NOT v1 SCOPE.** Owner decision 2026-10-06, recorded in the [scope reset](../decisions/2026-10-scope-reset.md).
>
> **Why it is not v1:** Price and budget were already deferred before the reset (stories 21, 22, 23, 58 paused); the owner confirmed price is out of scope.
>
> **What would trigger revisiting:** A request to cap or report spend on frontier or hosted nodes. The router already records an outcome for every request, which is where usage would be read from.
>
> This text is the design as it stood before the reset, kept so it can be picked up later. It is not maintained, and parameters and defaults named here are not in the live parameter table. Facts about pi, engines and the spikes are in the [decision record](../decisions/2026-10-spike-decisions.md).

## Frontier budget exhaustion

The same custody and budget rules apply to every hosted pay-per-token node, not only the frontier: the credential is held by the router, the node has a `budget_gbp_day`, and the rules below apply, including no silent downgrade. Each session may also spend at most `SESSION_BUDGET_FRACTION` of the daily cap, so one runaway loop cannot starve other roles; hitting it is treated like exhaustion for that session. The daily cap is checked before dispatch, so a request already in flight when the cap is reached completes; spend can overshoot by at most one request.

- A role with another candidate falls to it. The switch breaks stickiness for that session and is logged as a cost.
- A role with no other candidate (such as `architect`, whose only candidate is `frontier`) does not run. The task is posted as `blocked` with reason `budget_exhausted`, and the orchestrator is told. Nothing is silently downgraded to a weaker model.

## Architecture note

The frontier model, and any hosted pay-per-token node, sits behind the router with a daily spend cap. Behaviour when the cap is reached is defined in [dispatcher](#frontier-budget-exhaustion). Outputs passed back to the orchestrator are pointers (branch, path, link), so frontier context grows slowly.

## Registry fields

| Field | Purpose |
| --- | --- |
| `budget_gbp_day` | Spend cap for any hosted pay-per-token node, frontier or otherwise; behaviour at exhaustion is defined in [dispatcher](#frontier-budget-exhaustion) |

Hosted nodes in the example registry carried `budget_gbp_day`, and `system-one` and `frontier` showed it.

## Parameter

| Parameter | Default | Used for |
| --- | --- | --- |
| `SESSION_BUDGET_FRACTION` | 0.25 | Largest share of `budget_gbp_day` one session may spend |

## Other places budget appeared

- Filter step: drop nodes that are over budget (frontier or any hosted node).
- Live signals: router spend tracking, per call.
   - **Accept (budget):** at the cap, a role with another candidate moves to it, and `architect` returns `blocked: budget_exhausted`. Overshoot is at most one request. The same holds for a hosted non-frontier node. Real-process test: real router with a scripted hosted node that reports usage.

