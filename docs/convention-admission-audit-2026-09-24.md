# Convention admission architecture audit — September 24, 2026

Audit baseline: `5f95611`. This document proposes implementation work; it does
not claim the architecture below has been implemented. Inspection covered clue
generation, interpretation precedence, history recognition, perspective
projection, outcome compilation, policy constraints, planner comparison,
diagnostics, recent corrective commits, and the previous architecture reviews.
It was not an exhaustive audit of every convention or a new replay scan.

## Why did bluffs bypass basic checks?

There is a legitimate reason to replace an ordinary interpretation: a bluff must
not be required to complete the truthful finesse it only pretends to be. Commit
`62a1e9e` explicitly added `Bluff` to `named_interpretation_replaces_ordinary`
for this reason. That is evidence of the intended local behavior, not evidence
of an intention to waive MCVP. The separate ordinary/advanced generators already
existed in the August refactor (`6550a71`); the problem was not created entirely
by that recent fix.

The structural failure is that ordinary interpretation, admission checks, and
scoring are interleaved. Advanced clues are constructed separately, merged with
ordinary candidates, and can replace them by move kind. Consequently, checks
inside the ordinary path are not universal invariants. Each advanced branch must
remember which checks to reproduce. A valid exception to one interpretation
becomes an opportunity to miss an unrelated principle.

The reviewed example is green to Cathy in the purple-to-Alice projection at
[p4v0s1, turn 33](https://hanab.live/shared-replay-json/415ifirpxqufunsxcwgc-tbokayavlbjdgwqvekus-pdfanhhmkrpml,03tdeh-sbxckceeeisbkdeg1bep-edodeleufaerxaexwdez-fjgcfbffeswaeywcf5en-fvfqe2eo1bgce7emf3f4-ewf0tctdeHeIecocekkb-obe1fJf8f6et,0,p4v0s1#33).
It re-clues Cathy's secured g4 to bluff Bob's duplicate p3 while Donald's p3 is
already secured. The user ruled this an illegal zero-for-one. This audit uses
that existing review and its regression; it does not establish a new preferred
continuation.

There is no convention justification for bluffs as a class bypassing
[Minimum Clue Value](https://hanabi.github.io/beginner/minimum-clue-value-principle).
Special conventions can have documented exceptions, but those exceptions must
have specific conditions and scope. Being classified as `Advanced` proves
neither an exception nor that the clue gets a new card.

## Findings

### 1. Admission is distributed across constructors and labels

In `h_group/interpretation.rs`, ordinary generation starts at
`h_group_clue_candidates_from_replay_inner`; advanced generation is a separate
large function. Their results merge through `candidate_replaces`. Required Fixes
and fully clued endgame Burns also have early-return paths. These returns may be
appropriate, but their correctness is not enforced by a shared boundary.

The latest MCVP correction is a local repair, not completion of that boundary:

- The filter around line 968 applies to `CluePurpose::Play | Save`.
- The ordinary Bluff branch around line 1972 performs a separate count.
- Other advanced labels retain their individual conditions; there is no
  exhaustive contract requiring each to prove MCVP or a documented exception.
- `scheduled_clue_outcome(...).is_none_or(...)` preserves admission when the
  outcome cannot be compiled. Unavailable evidence is not proof of compliance.

This is a confirmed structural gap, not a claim that every unexamined advanced
clue currently violates MCVP. Replacing it with unconditional rejection on
missing evidence would introduce another error: uncertainty is not a proven
violation either.

### 2. Pipeline types promise more than they enforce

`candidate_pipeline.rs` names its input `SemanticallyAdmittedCandidates`, but
its constructor accepts a vector of already-created candidates. The debug
validation in `candidate.rs` checks that an action is a clue and that Fix/Tempo
purposes match their labels. It does not verify GTP, MCVP, recipient response,
or a valid exception.

`recipient_replay_assessment` in `interpretation/candidate_validation.rs`
generally checks broad interpretation categories. For most Advanced candidates,
any same-turn signal suffices to mark the candidate recipient-recognized. That
is weaker than proving the particular proposed response. It also runs after
semantic admission and tags the result rather than making that result the
admission contract.

### 3. Behavioral effects are reconstructed from move categories

Recent fixes demonstrate why this loses information:

| Recent correction                                         | Failure pattern                                                                                       | Required shared evidence                                              |
| --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| `d21fd07`: delayed Play protects the endangered chop      | Urgent protection recognized immediate plays and Save-like paths but missed another protective action | Protected card, recipient, deadline, and survival until that deadline |
| `d21fd07`: Save releases an untouched card                | Checking touched cards missed negative-information progress                                           | Changes in every affected owner's knowledge and actions               |
| `27a17f4`, `5f95611`: clue efficiency                     | Existing promises, new cards, and accelerated plays were counted inconsistently                       | Newly secured useful identities separate from tempo and information   |
| `2abf44c`: bluff reactor response                         | Giver-visible playability did not establish the reactor's intended action                             | Owner-relative response evidence                                      |
| `3391102`: provisional finesse exported into reactor view | A hypothetical interpretation helped prove its own prerequisite                                       | Dated facts and explicit hypothesis dependencies                      |
| `6a9c970`: Save Principle after clue actions              | An implementation guard accidentally restricted a general reviewed principle                          | The historical protection opportunity and its conditions              |
| `6c97149`, `ec4a0d9`: funded progress                     | Endpoint scheduling and resource comparisons lost relevant timing                                     | Comparable funded histories with explicit horizons                    |

`LineOutcome` already distinguishes actions, owner knowledge, protection, and
efficiency. However, policy and frontier logic still perform their own
touch/purpose-specific checks. `compile_named_line_metrics` also derives counts
and precedence from move labels and observer signals. These are multiple
opportunities to answer the same semantic question differently.

### 4. Provenance exists, but does not fully constrain inference

Keep the existing event effects, connection provenance, observer views, and
compiled prospective clues. They are useful foundations. However,
`BeliefProvenance::{Visible, Logical, Convention}` alone cannot distinguish an
established promise from a hypothesis conditional on another actor's response.
The recent `immediate_reactor` filter in `PerspectiveProjector` repairs one
cycle through an actor/kind/turn check. The general solution should represent
the dependency that makes exporting that fact unsafe.

### 5. Rejection explanations are reconstructed after the decision

`h_group_rejected_clues_from_replay` receives the admitted action list and
classifies missing clues by inspecting the position again. It cannot reliably
report the actual guard that rejected a clue. For example, the latest green clue
rejection was displayed as `NoConventionMeaning`, even though the identified
reason was zero minimum clue value. This prevents explanations from serving as a
reliable audit trail for principle enforcement.

### 6. Unified admission and equal search depth are different requirements

`choose_projected_follow_up` already shares candidate admission and the endpoint
comparator with the root. Preserve that reuse. Forecast leaves have a bounded
policy; do not recursively run full root search everywhere. Every path must use
the same semantic validity contract even when its planning budget differs. Label
heuristic choices and unresolved forecasts explicitly.

## Proposed architecture

Use one mandatory path for every physical clue:

```text
Legal clue
  -> proposed interpretations
  -> observer-relative interpretation resolution
  -> compiled causal effects and response evidence
  -> common principle validation
  -> admitted or explicitly conditional action
  -> urgency/scheduling constraints
  -> strategic comparison
```

Interpretation rules should propose meanings, effects, dependencies, and
documented exception claims. They should not construct an already-admitted
action, assign a final preference, or waive checks by choosing a move label.

The causal result should contain:

- Touched and untouched knowledge changes for each affected observer.
- Existing and newly created play commitments, with owner-relative responses.
- Newly secured useful identities, including indirect acquisitions, with
  evidence for deduplication against existing promises.
- Protection effects and deadlines, including resulting chop exposure.
- Accelerated existing plays, kept distinct from newly obtained cards.
- Prerequisites, possible misplays, unresolved alternatives, and assumption
  provenance, including source turn and originating hypothesis.

Compute these once from the existing compiled prospective transition. Both
principle checks and scoring consume this result; the MCVP validator should not
call into the strategic scoring module to discover semantics.

Give validation an explicit result such as `Pass(evidence)`,
`Fail(reason, evidence)`, or `Unresolved(requirements)`. A conditional action
must retain its conditions through planning; it must never silently become an
unconditionally admitted action. The exact treatment of unresolved convention
evidence needs a documented policy, distinct from normal uncertainty about
hidden card identities.

An exception must identify the principle it modifies, its documented rule, its
satisfied preconditions, and the precise effects covered. Examples include a
mandatory Fix or a forced Stall under MCVP, and a documented trash convention
under Good Touch. The registry must not be another boolean list saying “these
advanced moves skip validation.” Every other principle still applies.

Make the admitted-action constructor private to this validator. The planner
accepts only its validated or explicitly conditional outputs. Store the
validation trace with the candidate and render explanations from that trace.
Scores cannot rescue a failed principle check.

Interpretation remains convention-specific and observer-specific. A unified path
must not force Bob and Cathy to have identical beliefs, flatten a bluff into a
truthful finesse, or apply beginner direct-play requirements literally to every
advanced clue. Principles operate on the resolved effects and their documented
exceptions.

## Migration sequence and acceptance criteria

1. **Establish a focused, anchored contract corpus.** Use the reviewed cases
   above and their negative controls. Record existing failures and fixture
   fingerprints; do not update expectations merely to match a new engine.
   Preserve test assertions. Use artificial inputs only for ordinary algorithm
   and data-structure invariants.
2. **Separate outcome compilation from strategy.** Extract the semantic portion
   of `clue_line_value` into the existing compiled-clue layer. Keep existing
   behavior initially; differential-test complete outcomes, not just selected
   actions. Retain request-scoped caching and measure an identical workload.
3. **Introduce the common validator in shadow mode.** Record old and new
   admission decisions for every candidate, with a reason for each difference.
   Test MCVP, GTP, and response safety together; include documented exceptions.
4. **Make validation mandatory.** Route ordinary, advanced, Fix, and fallback
   Stall/Burn paths through it. Remove the old local principle checks in the
   same migration so two competing authorities do not persist. Prevent direct
   construction of admitted actions outside the validator.
5. **Move policy consumers onto effects.** Urgency asks whether the endangered
   card survives until its deadline. Productivity asks which new plays are
   caused. Clue efficiency asks what new useful identities are secured.
   Eliminate duplicate label-based derivations where these questions suffice.
6. **Generalize hypothesis provenance and comparison contracts.** Replace
   actor-specific assumption suppression with dependency checks. Separate
   semantic validity, conditional forecast evidence, and heuristic preference.

Acceptance tests should cover all registered convention families reaching the
validator; each exception's positive and negative conditions; repeated facts and
secured duplicates giving zero new value; legitimate indirect acquisitions;
unknown identities not proving duplication; no hypothesis proving its own
prerequisite; urgency seeing indirect protection; and diagnostics reporting the
same rejection that excluded the action. Root and projected decisions should
agree on admission for identical observer states, independent of search depth.

Use focused tests during extraction, then one full checkpoint after the shared
refactor settles. Do not call shadow-mode parity sufficient: reviewed bugs are
supposed to change. Check semantic expectations and investigate every
unexplained difference. A red historical test baseline and missing development
dependencies must be reported, not hidden behind passing local regressions.

## Recommendation

Make this a substantial refactor of interpretation-to-admission contracts,
starting with outcome compilation and mandatory principle validation. Reuse the
core game rules, event reducer, observer separation, connection manager,
compiled prospective transitions, caches, and planner. Merely moving the
existing branches into more files would leave the failure mechanism intact.

The September 18 review already identified incorrect intermediate behavior and
insufficiently enforced contracts. The intervening fixes sharpen that diagnosis:
the missing abstraction is a shared, evidence-bearing admission decision over
the actual effects of a clue.

## Implementation follow-through

The subsequent refactor introduces `ClueProposal` and a private
`CompiledClueAction` construction boundary. Ordinary, advanced, mandatory Fix,
and fallback Stall/Burn proposals pass through common MCVP, Good Touch, and
response checks before strategic scoring. Ordinary Good Touch and MCVP filters
were removed from their old admission branches. Conditions that distinguish
convention meanings remain in interpretation; those conditions cannot waive
common validation. Competing meanings survive until validation, so rejection of
an incorrect Bluff does not erase a valid ordinary interpretation.

Causal outcome compilation now lives in `clue_outcome.rs`, independently of
strategic preferences, and is cached with the prospective clue. It includes
untouched knowledge changes, newly playable cards, protection deadlines, and
trash demonstrated by a safe intervening response. Urgent protection and the
Save negative-information consumer use these effects. The discharge regression
exposed missing responder-relative value and trash knowledge being incorrectly
tied to an immediate discard instruction; both were corrected without changing
the recorded move or weakening its assertion.

Each principle records Pass, Exception, Unresolved, or Fail, with evidence and a
source for exceptions. Explanations use the recorded rejection when a proposal
fails validation; clues with no proposed interpretation retain an explicitly
labeled generic classification. Unresolved admission is a projection dependency,
not a passed check. Forecasts stop before consuming it, and exact search does
not report unconditional certainty over conditional clue admissions.

Projection assumptions now carry their originating focus, turn, and prerequisite
response decision. The projector enforces those dependencies without recognizing
Bluff/Finesse labels itself. The current dependency producer covers provisional
Finesse alternatives; this is not a claim that every possible future convention
has a complete dependency graph. Comparison records separately identify policy,
forecast evidence, and heuristic preference. Development heuristics no longer
supply edges in the forecast-dominance exclusion graph.

Contract coverage includes complete cached/uncached outcome equality at reviewed
turns 19 and 21; all registered move labels tested against zero and unavailable
value; conditional-clue dependency evaluation; the existing Bluff response,
indirect protection, new-card counting, and circular-assumption regressions; and
the reviewed discharge exception with demonstration evidence removed as a
negative control. The before/after reviewed-test comparison used baseline
`7199261`: 15 failures were reproduced before the refactor. Initial additional
discharge failures exposed the missing effects described above, rather than a
reason to exempt Discharge from validation. These focused comparisons do not
constitute replay-wide agreement or a clean full-suite result.

The rescan also exposed a response-policy inconsistency in a counterfactual
branch from turn 3: admission claimed Donald's blind play after purple to Bob,
while the action selector passed that obligation back because playing would
mislead Bob. Proposals now declare their required response, and the common
validator shares the action selector's Ambiguous Finesse Pass Back predicate.
The boundary regression checks both a wrong declared card and an inferred but
suspended response; the reviewed turn-19/21 Bluffs remain positive controls.
This is an implementation consistency fix, not a new human ruling that the
counterfactual clue or continuation is optimal.

The full checkpoint exposed six additional regressions against `7199261`. The
follow-up corrections preserve all original assertions: demonstrated Trash Chop
Moves count the unlabelled cards they protect; the Good Touch validator
recognizes accounted-for trash in the recipient's entire domain; critical Saves
record the explicit Save Principle exception when every direct clue has bad
collateral; and a layered connection no longer counts every possible slot's
physical identity as already secured. The latter preserves the reviewed p4v0s2
turn-6 Ejection's two acquisitions, p2 and b5, without double-counting
previously secured cards in the reviewed p4v0s1 Bluffs.

The duplicate-touch reproduction also verifies that established invisible
promises participate in Good Touch accounting. The common check now uses the
same promptable promise set as interpretation. The ordinary Rust checkpoint ran
449 tests: 388 passed and 61 failed. Before follow-up fixes, 55 failures were
separately reproduced on `7199261`, and six passed there. This is a measured
failing baseline, not permission to weaken or remove those tests. The checkpoint
passed build, Clippy, documentation, Python, and dead-code stages. Its GNU-only
ownership query silently failed on macOS; the script now uses numeric GNU/BSD
stat fallbacks and propagates query failures.

The subsequent broad recheck exposed a circular trash exception: a bad clue
could make the recipient incorrectly eliminate its real identity, then use that
inferred trash to waive Good Touch. The validator now also requires the giver's
existing evidence that the touched card is trash or duplicates an already
secured card. A newly created promise cannot certify its own exception. The
existing two-new-fours negative control passes, alongside the reviewed Trash
Chop Move and critical Save positive controls.

The historical p4v0s415 turn-32 discard assertion also fails on the starting
revision: baseline chooses 4s to Cathy, while the refactor chooses yellow to
Bob. Both reject the no-value 5s fill-in, which remains the test's admission
assertion. The first checkpoint temporarily passed the optimal-move assertion
because the incomplete causal outcome rejected productive Trash Chop Moves.
Restoring their protection effects exposed this pre-existing strategy failure;
the assertion is retained. Thus 56 failing tests have now been separately
reproduced on the starting revision, rather than 55.

Final verification: `scripts/check.sh --fast` passed. A workspace Rust recheck
passed all 394 included tests, excluding the 56 individually reproduced baseline
failures (plus the suite's normally skipped tests). This is not a clean
full-suite pass. The final p4v0s1 rescan agrees on turns 1–25 and stops at turn
26: Bob's recorded p4 discard versus purple to Alice. The current owner-relative
forecasts and unresolved strategic question are recorded in
`docs/replay-reviews/p4v0s1.json`. The fixture and all existing assertions are
unchanged.
