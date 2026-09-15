# Inference from a teammate's choice

Ordinary convention interpretation asks what a clue means. Inverse planning also
asks what a teammate would have done if one of our hidden cards were different.
This is an assumption about rational team strategy, not a deduction from the
game rules. It must not use our card's simulator identity.

## Quantifiers matter

To exclude an identity, the engine must check **every compatible assignment of
the observer's complete hand**. Assignments respect literal clues, remaining
card multiplicities, and the existing lower-order convention domains. Per-card
domains can overapproximate correlated interpretations; this makes the check
conservative, rather than supplying an excuse to ignore a difficult assignment.

For each assignment, the giver may have a different superior alternative: the
giver could see the entire hand. Showing one successful counterfactual, or
changing only the focal card while retaining the simulator's other hidden cards,
is insufficient. An extra card can change focus, introduce a competing Finesse,
violate Good Touch, or change another player's required action.

At present, all assignments must support the deduction from the same historical
choice. Combining different historical choices to eliminate different worlds is
not implemented.

## Historical perspective

For each observed clue, reconstruct the giver's view immediately before it:

- Install the complete hypothetical observer hand, only where those cards were
  already present at that historical moment.
- Hide the giver's own card faces, including their draw events.
- Use only the public history prefix, with its stacks, clues, tokens, and hands.
- Do not import later actions, later draws, or the observer's later convention
  notes into the giver's interpretation.

Later public reveals may constrain which past worlds remain possible to the
observer. They must not reveal those cards retroactively to the past giver.
Historical views that are exactly equal share a witness; the enumeration still
visits every current-hand assignment.

## Deliberately conservative comparison

The implemented certificate is a **resource-neutral clue substitution**, not an
unrestricted solution of the game tree. Both the observed clue and its
alternative must be admitted at the same policy tier. Both are projected using
the ordinary convention model, without inverse planning. At a common turn
frontier, the certificate requires:

- At least one subsequent action, not merely a new root promise.
- The same subsequent actions, consequences, and perspective assumptions.
- No strikes, unfunded clues, or unsupported convention dependencies.
- Strict improvement under the planner's non-speculative resource comparison,
  with no compensating deterioration in its other resource dimensions.

This captures a clue that obtains an additional secured play without disrupting
the same subsequent sequence. It does **not** declare every line with a larger
heuristic score, more projected turns, or more raw connections superior. The
resource comparison is still model-relative; it is not a mathematical proof of
optimality over every possible future game. Errors in convention interpretation
or resource evaluation can invalidate a certificate and require regression
tests.

Different action sequences, resource tradeoffs, and branching beyond an unknown
frontier can remain incomparable. In that case, retain the identity. Ordinary
clue selection may express a preference where inverse planning cannot safely
turn that preference into an exact card note.

## Bounds and provenance

The current discovery pass considers clued, settled, same-rank ambiguous cards
with a currently playable alternative. It tries excluding their non-playable
alternatives using the latest eight eligible one-touch Play Clues by teammates.
Each proof is bounded to 4,096 assignments, 128 distinct historical giver views,
and 16 projected actions per line. Hitting an enumeration bound, finding a
counterexample, or failing to project a world means **no deduction**. Empty
coverage is never a proof. A projected view with unresolved cards in other
players' hands is outside this hand-only quantification contract.

Before publishing any deductions, the engine also checks that their combined
domains and correlated convention constraints admit at least one physical hand.
If the rational-choice assumptions jointly contradict that belief, all strategic
additions from the pass are dropped; ordinary knowledge is retained. This is
important for self-play bug recordings and imperfect human decisions. Individual
nonempty card domains are not sufficient to establish joint consistency.

The canonical replay reducer first builds lower-order knowledge. The strategic
stage then records the observed turn, excluded identity, assignment count, and
per-view line witnesses. A typed `StrategicChoice` knowledge effect narrows the
owner's domain at the time the evidence becomes available. Rebuilding knowledge
uses those same effects; it does not maintain a second mutable set of notes.

Counterfactual analysis has a separate replay-memo stage and cannot recursively
assume the conclusion it is trying to establish. A bounded thread-local result
cache is keyed by the entire immutable view, profile, baseline card notes, and
clue interpretations. It is a cache of a pure bounded query, not learned or
mutable convention state. A second bounded cache shares identical historical
giver queries across later observations; complete present-hand coverage must
still be checked again. No certificate is reused merely because two positions
have the same focal card or preferred clue.

Within one historical query, identical action/horizon projections share their
immutable evidence. The requested horizon is part of the key; reaching an
unknown card after four actions is not interchangeable with requesting a
four-action limit. Already different action prefixes cannot satisfy the
same-suffix certificate, so those alternatives need not be projected again at
the shorter common frontier. Equal prefixes still undergo the complete
common-frontier comparison. Witnesses are immutable shared values when replay
states or cache entries are cloned; sharing changes neither their contents nor
their validation.

## Reviewed example and safeguards

The motivating position is `game-p4v0s2.json`, Alice's ambiguous yellow/green 4.
The reviewed green-4 counterfactual permits Donald to clue yellow to Bob, Alice
to clue yellow to Cathy, Bob to play yellow 4, and Cathy to play yellow 5. Tests
also vary Alice's newest card: another yellow 4 introduces a competing Finesse
reading, so the same clue cannot be assumed to work in every assignment.

The completed certificate in this replay uses Cathy's earlier rank-2 Play Clue
to Alice on Hanab Live turn 31. In the green-4 worlds, a rank-4 clue to Bob can
secure the additional yellow 4 while preserving the compared subsequent actions
and resources. The historical-view search checks this separately for every
compatible hand. Donald's later yellow line is tested as a valid continuation,
but its different actions and token usage are not silently treated as a
resource-neutral substitution. The inference is discovered by searching the
history, not by a turn-31 or yellow-4 special case.

Related shared fixes ensure that an Early Save is not classified as urgent while
its recipient already has a play, that a known giver-owned delayed connector
remains usable without exposing hidden faces, and that such a connector is not
counted as missing in frontier evaluation. A Gentleman's Discard's token is not
a compensating benefit when playing instead lets the very next player refund
that token by finishing the suit.

These are general scheduling and information-boundary rules, not checks for a
particular seed, card order, turn number, or suit. See the convention sources
for [efficiency](https://hanabi.github.io/level-3/#efficiency),
[tempo](https://hanabi.github.io/level-3/#tempo),
[delayed plays](https://hanabi.github.io/beginner/delayed-play-clues), and
[Gentleman's Discards](https://hanabi.github.io/level-10/#the-gentlemans-discard-gd).
