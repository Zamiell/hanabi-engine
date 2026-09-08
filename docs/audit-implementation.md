# September 8 audit implementation

Scope: complete the September 7 repository audit without rewriting reviewed
replay actions or assigning hidden truth to player decisions.

## Work ledger

- [x] Fixture manifest and accurate reviewed-prefix reporting.
- [x] Structured preferences in every planner path; remove redundant ordering
      fields.
- [x] Explicit conditional-projection assumptions and observed/assumed
      boundaries.
- [x] Compiled observer-relative line evidence, consumed without downstream
      recognition.
- [x] Shared work budgets, cancellation, lazy constraints, honest fallback
      diagnostics.
- [x] Immutable shared cached projections and position-bound cache context.
- [x] Event-specific reducer stages and shared public transition mechanics.
- [x] Replace source-spelling architecture checks; move audit-only inventory out
      of production.
- [x] Lazy CLI presentation and versioned engine handshake.
- [x] Atomic live-bot result validation/send and concurrency regression.
- [x] Minimum supported Rust CI build.
- [x] Performance measurements, complete validation, documentation and final
      re-audit.

## Measurement protocol

Use identical reviewed positions and unchanged planning limits. Compare isolated
optimized test runtimes before/after each performance change; distinguish build
time from test time. Validate action and interpretation expectations alongside
timing. Do not infer speedups from concurrent full-suite timings.

## Results

The reviewed blue-2 opportunity test took 25.99s before changes. Shared-handle
and content-key stages took 25.37s and 25.47s. Final isolated samples were
25.62s, 25.50s, and 25.56s: no meaningful speedup established. The lazy-factor
contract demonstrates bounded traversal of 40 factors without eager expansion.

The final `scripts/check.sh` passed in **4m21s**: **329 Rust tests**, **21
Python tests**, and **zero Hawk findings**. All declared replay review
boundaries agree. The separate Rust 1.85 workspace/all-targets/all-features
check also passed. The full-check baseline was 4m16s; there is no overall
speedup to claim.

## Final re-audit boundaries

- Only one action preference controls ordering; numeric score no longer grants
  connection admissibility. Within-category strategic heuristics remain.
- Strategic clue valuation consumes compiled line evidence and does not inspect
  signal history. The existing epistemic compiler remains authoritative.
- Shared caches match position/profile, not pointer identity; they are not a new
  persistent convention state.
- Constraints remain factored. Cancellation cannot masquerade as an exact world
  count or a complete candidate comparison. Convention compilation and frontier
  assessment remain atomic units: deadlines are cooperative.
- Event handlers preserve rule order and share core public clock mechanics. The
  clue-specific handler remains substantial.
- Four fixtures have full reviewed action parity. p4v0s415 has 36 reviewed
  actions and an explicitly unreviewed generated suffix.
- The expensive 200-game self-play benchmark remains outside check.sh and was
  not run or rebaselined during this structural audit.
