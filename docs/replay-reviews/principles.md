# Cross-position user rulings

Except for the current turn-18 teammate-clue, turn-19 clue-efficiency, and
turn-26 discard-safety, turn-27 duplicate-response, and turn-33
minimum-clue-value rulings, the p4v0s1 positions below refer to the preserved
[historical fixture](../../crates/hanabi-search/src/h_group/tests/fixtures/game-p4v0s1-before-turn18-revision.json),
not the replacement continuation supplied on 2026-09-23. Their original review
anchors and regression assertions are retained.

These are human-reviewed principles, not additional engine configuration. Read
the linked position records for evidence and boundaries; do not introduce
fixture IDs or turn numbers into production decision logic.

## Play and leave a productive clue to a teammate

Current fixture `p4v0s1`, live turn 18, record
`t18-play-and-leave-the-clue-to-cathy`: the user explains that a player with a
card to play should usually play it and let a teammate give the clue. Bob should
play b5 instead of taking Cathy's clue-giving turn. His projection already has
Cathy clue 4s to Alice to bluff Donald's r1.

This is a scheduling preference, not an absolute prohibition on cluing while
holding a play. Evidence of a better outcome, urgent obligations, and safety
still take precedence. Do not require the teammate to give the identical clue,
assume unseen cards or draws, or treat an unfinished forecast as successful. The
current implementation credits a certain play followed by a funded teammate clue
and an explicit immediate scoring response before the original player acts
again. Longer or uncertain handoffs remain outside this conservative rule.

Regression: `reviewed_turn_eighteen_leaves_the_clue_to_a_teammate` verifies the
bluff projection, scheduling comparison and chosen action, with
evidence-ablation and urgent-policy controls. Historical
`reviewed_turn_thirty_compares_equal_elapsed_time` retains a reviewed case where
endpoint evidence favors a clue over b5.

Implementation audit, 2026-09-27: the same scheduling preference applies when
playability is established by a settled convention interpretation rather than
literal clues alone. In `p4v0s415` turn 12, Donald's b3 permits Alice's funded
blue Load Clue and Bob's immediate b4. A literal-only certainty guard silently
excluded that case. The forecast must record a settled singleton interpretation
and a successful root play; unknown or multivalued interpretations do not earn
this additional proof route. This is an implementation of the existing ruling,
not a new user strategy rule. Regression:
`reviewed_convention_known_play_earns_teammate_handoff`, with missing and
ambiguous interpretation controls, alongside the original turn-18
evidence-ablation test.

## A productive handoff must preserve executable continuation

In `p4v0s415` turn 16, `t16-finesse-plays-both-greens-before-handoff`, the user
explains that green to Bob plays g2 but leaves g3 waiting in hand. The
4s-to-Alice finesse instead makes both g2 and g3 play, and is preferable. Count
committed plays, not just touched cards or an immediate scoring response. The
usual preference to play and delegate a clue cannot erase this difference.
Compare equal elapsed time and include already-realized points so earlier plays
are not mistaken for lost commitments. The implementation compares the immediate
play/clue/response handoff at no greater clue cost: additional work bought by a
second clue does not prove that delegating the first clue loses work. Unknown
continuations prove no advantage.

Regressions:
`reviewed_finesse_commits_green_three_but_direct_green_leaves_it_waiting` checks
Bob's own knowledge after g2 in both branches, with hidden faces and blank
draws; `reviewed_finesse_continuation_outweighs_weaker_clue_handoff` checks the
planner and missing-commitment/evidence controls. The turn-12 and turn-18
handoff regressions retain the positive cases.

## Save Principle: declined protection constrains discard risk

Source: [Save Principle](https://hanabi.github.io/beginner/save-principle/).
Reviewed examples: `p4v0s1.json`, live turns 33, 38, and 40 in this directory.

The user repeatedly ruled out unseen y3/g3 on these exposed chops because the
team would have protected unique playable cards. This is not limited to one
player, the immediately preceding turn, or a preceding ordinary play. Giving a
different clue—including a Save to another player—can also establish declined
protection. An available clue for the discarder does not itself invalidate that
evidence. Do not treat old implementation restrictions as exceptions to Save
Principle.

Evaluate the actual historical opportunity: the same card was exposed, the team
could protect it, and the recipient was not occupied by a promised response. The
current turn-26 ruling also covers earlier teammates, not only the player seated
immediately before the discarder: Alice's zero-token discard does not erase
Donald's earlier opportunity to protect Bob. See
`t26-team-save-principle-removes-all-discard-bdr` for the user's per-identity
reasoning. Retain valid historical evidence across later token exhaustion. Do
not assume that all unknown cards are safe, use their actual faces, or turn
conditional discard safety into literal knowledge or permission to blind-play.

Regression contract:

- `reviewed_turn_twenty_six_discard_has_no_bottom_deck_risk` covers the current
  turn26 ruling, unchanged literal knowledge, missing-history control, and the
  safe reveal branches' zero losses and guaranteed token refund.

- `reviewed_save_principle_exclusions_apply_after_plays_and_clues` checks all
  three reviewed positions, unchanged literal knowledge, and the missing-history
  negative control. Its derived turn-42 case also covers a Save exposing a new
  chop: evaluate the recipient's post-clue response position, not just the
  pre-clue chop. Its turn-44 case covers a voluntary trash discard with a
  protection token already available. These are implementation regressions, not
  new user rulings; zero-token discards and useful-card transfers are not
  treated as equivalent evidence.
- `first_seed_donald_discard_does_not_invent_playable_three_risk` verifies that
  the turn-40 planner consumes the reduced risk domain, not the raw logical one.
- Existing turn-33/38 tests cover fresh non-chop cards, visible replacements,
  delayed evidence, and planner risk evaluation.
- `save_principle_does_not_infer_safe_chop_for_an_occupied_player` checks that
  queued responses do not turn into permissions to discard instead.

The former ordinary-play-only implementation was an incomplete implementation,
not a user-approved boundary on the convention.

## Funded protection does not erase earlier productive progress

Reviewed example: `p4v0s1.json`, live turn 28
(`t28-reviewed-purple-four-progress`).

The reviewed Donald-rooted p4 line plays p4, r2, p5, and r3 with no BDR through
its eight-action prefix. An alternative completing a Save earlier needs a real
benefit to outweigh that progress; a later snapshot must not forget when the
points were earned. Count a pending critical Save's funding obligation once,
including it in the compared clue bill. Do not turn the obligation into a
secured or playable card, ignore unfunded Saves, or trade away ready successors
and hand quality merely for earlier points. This is not authority to assume
Donald's unknown r4 in his own forecast or to freeze other observers' lines.

## Clarification does not make an earlier Save deadline free

Current `p4v0s1`, turn29, record
`t29-clarification-does-not-justify-accelerating-yellow-five-save`: the user
prefers Alice playing p2 and leaving purple to Bob. Negative5 on Cathy's red4 is
not worth accelerating Bob's hand and bringing forward the yellow5 Save. Cathy
need not identify that red card immediately for the play to be preferable.

Funding a pending Save prevents treating it as a lost card; it does not erase
its scheduling cost. At equal realized score, extra identified future work must
not automatically dominate while exposing additional critical chops. Preserve
the earlier ruling for actual scoring progress; do not assume hidden identities
or impose a blanket ban on useful clarification.

Regression: `reviewed_turn_twenty_nine_clarification_preserves_save_timing`
checks the reviewed root, all compared safe reveal branches, and controls for
clarification without extra exposure and genuinely realized progress.

## Team Distribution: who draws when a clue is interchangeable

Source:
[Team Distribution Principle](https://hanabi.github.io/level-8/#team-distribution-principle).
Reviewed example: `p4v0s1.json`, live turn 40 (`t40-donald-draws-alice-saves`).

When either player can give the same clue before its recipient acts, and the
discard is safe, prefer the player with less known useful work to draw. This
applies to Save Clues too, not only Play Clues. Also compare conditional draw
schedules: drawing a predecessor into the hand already holding its successor can
delay the stack. Unknown draws remain unknown; this is a conditional comparison,
not a prediction of their identities.

An unfunded later part of the chain must not erase an earlier funded advantage.
Compare the same funded prefix on both sides. Preserve clue availability,
recipient deadlines, interpretation, and protection when handing off the clue.

Current turn33 (`t33-relative-workload-and-common-observer-save-handoff`) has
one known useful card in Alice's hand versus two useful cards in Bob's. The
reviewed principle is relative workload, not an exact-zero requirement. Exclude
unclued duplicates already secured elsewhere from the teammate's needed work.
Check the later giver's admission using their own knowledge, then compare both
clue outcomes from the original observer. Re-rooting only one outcome can hide
what the recipient sees and manufacture a knowledge loss.

`reviewed_lighter_hand_can_delegate_a_save_with_one_known_play` verifies the
same safe Save handoff and resulting discard preference, with equal-workload and
missing-visible-work controls.

## Clue efficiency counts newly obtained cards

Current `p4v0s1`, live turn 19, record
`t19-two-bluffs-have-higher-clue-efficiency`: the user compares two 2-for-1
clues (4s to Alice, then purple to Donald) against a 2-for-1 and a 1-for-1
(purple to Bob, then green to Bob). Already-clued p3 and g5 do not become new
cards obtained by the later clue merely because it clarifies or releases them.
Keep their timing and continuation benefits separate from clue efficiency.

A Bluff through an already-clued intermediate can identify that intermediate
using literal information from the same clue. Donald's rank-clued p3 becomes
literally purple 3 when the purple clue touches it; overlooking that evidence
must not erase the newly secured p4. Do not use later clues or speculative
same-clue identity promises to establish the intermediate.

Regressions: `reviewed_turn_nineteen_counts_new_cards_not_old_connectors` checks
all four clue counts; `reviewed_two_bluffs_secure_both_fours` checks Donald's
inference and the endpoint's secured cards, with a historical cutoff control and
future draws hidden.

At equal realized points and clue resources, an additional secured future card
is a concrete efficiency gain; an unclued visible successor must not by itself
veto that gain. Readiness of already secured cards is a tempo benefit, not
additional clue efficiency. Retain guards for realized points, clue costs,
future clue demand and safety.
`reviewed_turn_nineteen_prefers_two_two_for_one_clues` checks the actual planner
choice and endpoint counts, with controls for lost points, extra clue cost, lost
committed plays, exposed critical cards and equal secured coverage.

## Minimum clue value is an admission requirement

Current `p4v0s1` turn 33 in the turn-28 purple-to-Alice projection, record
`t33-green-bluff-zero-minimum-clue-value`: the user rules that green to Cathy is
an illegal 0-for-1 under
[MCVP](https://hanabi.github.io/beginner/minimum-clue-value-principle/). Cathy's
g4 is already clued; bluffing Bob's p3 merely replaces Donald's secured p3.
Neither a new physical clue fact nor playing a different copy earns new card
value. This is not a preference to resolve with a heuristic penalty.

Count newly secured useful identities, including indirect plays and protection.
Do not count previously secured cards again or infer duplication from unknown
identities. Preserve the documented Fix, valuable Tempo, chop-move and forced
Stall exceptions; do not prohibit all clues on already-clued cards.

Regressions: `reviewed_green_bluff_requires_minimum_clue_value` checks
rejection, legitimate turn19/21 bluffs, and unknown-identity controls;
`reviewed_turn_nineteen_counts_new_cards_not_old_connectors` checks zero value
for this clue alongside the previously reviewed productive lines.

The current turn-27 review (`t27-indirect-red-two-duplicates-givers-known-play`)
also checks the giver's own known card. A hidden physical face is not an unknown
identity when the giver already knows it from prior clues. Use only that giver's
pre-clue evidence; unresolved domains and other observers' hypotheses do not
prove duplication. Donald's r2 does not obtain another useful identity when
Cathy already knows she holds the secured r2.

`reviewed_turn_twenty_seven_counts_givers_known_duplicate` checks the corrected
acquisition count and removes the giver's identity evidence as a negative
control. The existing turn33 green Bluff regression covers the parallel
visible-copy case. The turn27 ledger separately records an unresolved question
about the extra Ignition: the clue also Trash Pushes Alice's p2, so correcting
its false extra r2 credit is not a ruling that the entire clue has zero value or
that every indirect duplicate is prohibited.

On 2026-09-25 the user accepted 1s to Alice and replaced the continuation from
turn27 (`t27-user-accepts-ones-and-replaces-continuation`). This resolves the
move choice for that position, without supplying a broader duplicate-Ignition
rule. Later positions from the prior continuation retain their historical
anchors and regressions in `game-p4v0s1-before-turn27-revision.json`.

## Known trash prevents a locked-hand Anxiety obligation

Current turn30, `t30-known-trash-prevents-false-anxiety`: the user supplies
purple to Cathy followed by her r2 discard. A fully touched hand is not locked
when the recipient knows a card is trash. Use the recipient’s knowledge after
the clue, not merely the giver seeing no playable card. The clue validator and
the recipient’s ordinary action policy must agree on this condition.

`reviewed_turn_thirty_known_trash_prevents_false_anxiety` verifies clue
admission and the recipient’s discard with both actual visible draws and unknown
replacement draws. The existing reviewed p4v0s3 Anxiety regression retains the
genuine locked-hand case.

On 2026-09-26 the user accepted purple to Alice at turn26 and replaced the
continuation through turn54
(`t26-user-accepts-purple-and-replaces-continuation`). The prior 1s-to-Alice
continuation and known-trash Anxiety regression are preserved in
`game-p4v0s1-before-turn26-purple-revision.json`. Discard safety remains valid;
the revised move supplies no broader strategy rule.

The user subsequently accepted the turn27 Gentleman’s Discard and replaced its
continuation through54 (`t27-user-accepts-gentlemans-discard-continuation`).
This accepts the concrete move without establishing a general transfer premium.

## Known-trash collateral retains secured duplicate possibilities

Current `p4v0s1`, live turn 30, record
`t30-normal-purple-play-with-duplicate-collateral`: the user identifies purple
to Cathy after Alice's p2 play as an ordinary Play Clue. The reviewed
continuation discards Cathy's newly touched p3 while Donald already holds clued
p3 and p4.

The implementation must distinguish a new play promise from collateral that the
recipient knows is trash. Being safely discardable does not imply a physically
dead identity: preserve possibilities secured by other clued cards. Admission,
connection safety, and discard/Anxiety evaluation must agree about that
knowledge. This is not blanket permission to duplicate useful touches; the
owner's entire remaining domain must be accounted for, and common clue
validation still applies.

`reviewed_purple_play_with_known_duplicate_collateral` checks the actual
position and the projected post-p2 position without future draws, including
missing-evidence and useful-focus controls. Historical
`first_seed_four_save_accounts_for_collateral_trash` retains secured g4 as
possible while requiring every collateral alternative to be physically dead or
that visible clued g4.

The same reviewed normal clue must retain its meaning after a connector is
played. Record `t29-public-connector-play-must-not-rewrite-purple-as-ejection`
documents a historical-state bug: counting connectors in current hands could
retroactively classify the old clue as a 5 Color Ejection and retract Donald's
p4. Count clue-time hands and prior literal evidence; neither later draws nor
same-clue touches supply previously established connectors.
`reviewed_purple_connectors_survive_their_public_plays` checks r3-first and
p3-first branches, retained p4 knowledge and downstream commitments, plus a
control removing the old rank clue without exposing the hidden card's face.

The same regression covers all seven safe reveals of Alice's unknown discard. A
hypothetical connector created by a clue cannot make that originating clue a
zero-value duplicate for Time Travel Chop Move recognition: the duplicate must
already have been secured before the clue. Otherwise a possible g1 reveal
falsely changes an earlier green clue's meaning and the later purple focus.

## Public Good Touch identities must reach convention consumers

Current turn32 (`t32-priority-prompt-identifies-cathys-red-four`) and the
hypothetical turn43 branch (`t43-demonstrated-bluff-retracts-hidden-connector`)
expose the same implementation omission: a clued card can be publicly known
because all but one literal possibility are dead. Priority and resolved-Bluff
recognition must retain that knowledge when the card is in the observer's own
hand. Requiring a visible face or an explicit rank-and-color singleton creates
inconsistent interpretations between observers.

This is an implementation consequence of Good Touch, not a new user strategy
ruling. It does not identify arbitrary unknown cards or use later evidence to
justify an earlier decision. A demonstrated Bluff must cancel its competing
Finesse, including the inferred identity subsequently exported to teammates.

`reviewed_good_touch_identities_reach_priority_and_bluff_consumers` checks both
reviewed positions with hidden owner faces, then exercises each downstream
recognition and action/export regression. Existing prior-obligation and
unresolved-response controls remain in place.

## Fully clued does not erase a known playable transfer

In `p4v0s415` turns29–31
(`t29-known-playable-gentlemans-discard-is-not-sacrifice`), the user supplies
Alice’s g4 Gentleman’s Discard followed by Cathy’s g4 play. Alice’s hand is
fully clued, but she knows the discarded g4 is playable. The rare Sacrifice
Discard of a future card must not suppress this ordinary transfer. Use
pre-discard owner knowledge; the revealed face alone does not establish what the
actor knew. This complements the earlier known-trash Anxiety ruling: a count of
touched cards does not establish that the actor has no useful action. It also
complements the `p4v0s3` pre-event protection regression; a discard cannot
manufacture its own locked-hand precondition.

`reviewed_known_playable_discard_from_fully_clued_hand_transfers` checks the
actual and projected continuation from three perspectives, including Cathy’s
hidden hand and downstream playability. The earlier
`fourth_replay_gentlemans_discard_is_not_a_self_created_sacrifice` retains its
pre-event protection checks.

## A saved card can already have an executable continuation

In `p4v0s415` turn28
(`t28-five-clue-secures-delayed-blue-play-and-purple-clarification`), the user
explains that 5s to Alice obtains b5 after her already-known g4 plays. Alice's
remaining g5/b5 ambiguity does not require another clue. This is productive play
acquisition, not a passive Early Save. Merely seeing missing predecessors is
insufficient: their plays must already be established.

The same clue identifies her previously purple-clued p5 for later. With both
lines funded and committed progress preserved, this useful clarification is
better than obtaining an unnecessary immediate b5 refund. Keep identified future
cards separate from executable commitments: this does not invent p5's missing
connectors. Preserve the earlier negative5 ruling against accelerating a
critical Save merely for clarification. Unknown hands and future draws remain
unknown; no universal transfer preference follows from this ruling.

Regressions:
`reviewed_delayed_five_clue_commits_play_and_identifies_purple_five` and
`reviewed_delayed_five_play_prefers_useful_purple_clarification`, including
missing-commitment, missing-clarification, funding and safety controls.

## Duplicate touches require a safe continuation

In `p4v0s415` turn32 (`t32-duplicate-touch-has-safe-future-clarification`), the
user explains why purple to Bob is optimal despite touching both p2s. Purple1
has played and Alice's p5 is globally known. Bob's collateral would ordinarily
be misplayed as p3, but the team can first clue purple to Alice for her p3. Both
collateral notes then become p4. Bob's normal order selects the leftmost real p4
before the duplicate p2, making the duplicate harmless without another clue to
Bob. Good Touch protects against confusion and wrong plays; this proven
continuation is an exception to the usual duplicate prohibition.

Admission must establish that continuation from the clue giver's current
knowledge: the clue is funded, intermediate actions are safe, the prerequisite
becomes established in its owner's knowledge, and ordinary play order consumes
the real successor first. Mere visibility of a possible connector is not a
proof. Future draws remain unknown. Do not assume a recursive chain of other
unproven exceptions or bypass Minimum Clue Value and response safety.

The certificate, downstream inference/play-order test and integrated planner
test listed in the review record cover this position. Negative controls remove
funding, hide the connector, and put the duplicate first. Collateral from the
same clue must narrow together; one peer's new note is not independent evidence
for excluding that identity from the next peer during the same inference pass.

These correlated notes also cannot establish that either card is a disposable
spare before the successor plays. An independent literal identification or later
focused clue can establish a duplicate; two notes derived from the same
collateral promise cannot. The regression checks the waiting positions after 35
and 37 as well as play order after 41, and a literal-identification control.

## Same-Priority plays can prefer an earlier five refund

In `p4v0s415` turn37 (`t37-same-priority-prefers-earlier-five-refund`), Alice's
p3 and y4 both lead into clued teammates. With no chop to protect, they should
be evaluated similarly; lower rank is a fallback rather than an absolute
ordering between them. The user slightly prefers y4 because Cathy plays y5
before Alice returns. Playing p3 leads through Bob's p4, but Alice still plays
y4 next, so her own p5 supplies no equally early refund.

Use the actual scheduled continuation, not mere reachability of a five. Require
preserved progress and protection, equal elapsed turns, and an extra usable
refund. Higher-priority obligations and losses still take precedence. The
regression checks both real lines and removes refund evidence, same-category
status, protection, and progress in negative controls.

An earlier observer may stop before an unresolved teammate action even after
seeing the refund. Compare the equal established prefix before the actor
returns; do not require an entire rotation when the benefit is already proved.
The same regression checks the three-action prefix through Cathy and keeps later
unknown actions unknown.

## Equivalent winning play clues prefer color

In `p4v0s415` turn43 (`t43-equivalent-winning-play-clues-prefer-color`), green
and 5s to Bob both obtain g5 and finish perfectly with Alice's already-known p5.
The user explains that negative information has no remaining value once all
needed cards are accounted for. Treat the winning outcomes as equivalent and
prefer the color clue as a fallback over the rank clue. Both are Play Clues.
This does not turn a later repeat clue into a new play acquisition.

`reviewed_equivalent_winning_play_clues_prefer_color` checks both complete
projected lines, equivalent perfect endpoints, and the color preference. An
unfinished-forecast control prevents treating an unproved finish as equivalent.

## Burn when a perfect finish is already secured

In `p4v0s415` turn44 (`t44-secured-finish-prefers-fill-in-burn`), the user
prefers Donald's 5s Burn to a safe discard. Alice's p5 and Bob's known g5 finish
the game either way. A funded finish using already-known cards permits this Burn
even before the ordinary deck/pace threshold. The extra discard token has no
remaining use. Filling in g5's rank is preferable to repeating green; the color
fallback for equivalent Play Clues does not reverse that preference.

This is a stall exception, not a new card acquired by the clue. Require public
owner knowledge, sufficient tokens, and enough turns for the schedule; unknown
faces or future draws cannot establish completion. The focused turn44 and turn43
planner regressions verify the finish and preference; certificate controls
remove card knowledge, funding, and time.

## Ambiguous gotten cards can violate Good Touch at every rank

At p4v0s9 turn3's hypothetical turn6, the user rejected Bob's 2s to Alice as a
Good Touch violation. Bob's gotten 2 can be y2 or g2; Alice's newly touched
cards include g2. An unknown own identity does not authorize promising a
possible duplicate. Apply the overlap check to all ranks, retaining proven safe
duplicate-continuation exceptions. This risk check does not turn a possible
identity into exact knowledge or a factual MCV duplicate.

See `t3-projected-twos-ambiguous-good-touch` in [p4v0s9](p4v0s9.json).
Regression: `reviewed_projected_twos_respect_givers_ambiguous_good_touch` checks
ordinary and masked forecast views, downstream admission, and clarification to a
nonduplicating identity as a negative control.

## Forecast teammate choices from the observer's probability of coverage

The earlier `p4v0s9` turn-7 unconditional loading preference was superseded by
`t6-majority-conditional-cathy-response`. Bob cannot see his own r3. If he holds
it, Cathy sees Donald's safe duplicate chop and should play p1; otherwise Cathy
has reason to accelerate b1/b2 by cluing blue to Alice. The user says Bob should
project blue at the current roughly30% probability of holding r3, and p1 if that
probability is greater than50%. Exactly50% was not ruled on. The general
scheduling override from c1f22f9 was explicitly reverted.

This is a forecast choice under uncertainty, not permission to infer a card's
identity or declare its discard certainly safe. Weight physical-card assignments
using the observer's information; distinguish literal clues, Good Touch
constraints, and any additional model of clue selection. The projected-choice
implementation uses a strict majority only for this conditional duplicate
coverage tradeoff. It preserves existing selection at exactly50% or when a
bounded count cannot establish the probability. It does not soften critical
losses, strikes, forced actions, successor development, or 5 refunds. Unknown
identities and conservative loss annotations remain intact. See the three
`planner::majority::tests` regressions linked in the position record for the
current case, above-threshold and equality refinements, and negative controls.

## Conditional duplicate protection must use consistent information

In `p4v0s9` turn6 (`t6-delayed-red-three-can-be-protected-conditionally`), 2s to
Alice still permits Cathy to give Donald a delayed red Play Clue before his r3
would be discarded. Cathy sees whether Bob has a clued r3. When he does,
Donald's copy has a secured replacement; when he does not, Cathy can protect
Donald. A forecast must not combine the duplicate world's clue rejection with
the nonduplicate world's unique-card loss as an unconditional consequence.

A giver's visible nonduplicate takes precedence over that card owner's older
ambiguous note when checking Good Touch reservations. This does not supply an
unknown face to the original observer or authorize a possibly duplicating root
clue. The localized regression
`reviewed_delayed_red_three_uses_givers_visible_nonduplicate` checks three
conditional non-red identities, the actual duplicate, hidden-source controls,
Cathy's selected clue, Donald's waiting/protection, and playability after r2. It
does not cover aggregation by itself. The subsequent
`t6-conditional-protection-policies-keep-their-own-losses` implementation
retains bounded, exhaustive private-card branches and compares their common
elapsed horizon. No branch supplies its assignments as public knowledge or
cancels a loss in another branch. Unexpanded domains remain explicit
uncertainty. The two `private_policy::tests` regressions cover both conditional
policies, inference, owner masking, resource/budget controls, and integrated
downstream evidence.

A shorter unresolved sibling cannot erase a loss witnessed in another branch
when the competing forecast reaches that loss's turn. Conversely, an unfinished
competing forecast cannot certify avoidance. The planner regression
`unfinished_sibling_does_not_erase_a_witnessed_conditional_loss` checks both
comparison directions and the shorter-opponent negative control.

The majority response selector must inspect covered private continuations, not
only their shared prefix, which may end before any discard. Its original
probability is still computed from the observer's unchanged information.
Conditional assignments are used to validate the response, including visible
successors, without revealing faces to that observer or erasing minority-world
losses. The existing 30%, 50%, above-50% and certainty expectations remain in
force; their safety assertions now traverse the branch representation.

The certain covered case must also apply when Cathy chooses her own move, not
only when Bob forecasts her. In `p4v0s9` turn7
(`t7-visible-coverage-uses-the-reviewed-response`), Cathy sees Bob’s touched r3.
With the same committed/secured progress, no visible b3 or five refund, and a
safe Donald discard, she plays p1 and retains the extra clue. Root, forecast and
completed-candidate deadline comparisons share the same scheduling eligibility.
This is not the retracted unconditional loading preference: unknown or untouched
replacements and useful successor/refund progress do not satisfy this proof. The
root/forecast regression includes those negative controls, while the existing
majority regressions preserve Bob’s information and probability rules.

## Cancelled claims cannot borrow a later promise’s authority

In `p4v0s9` turn14 (`t14-cancelled-layer-cannot-revive-a-fix`), an old cancelled
y2 claim and membership in a new p3 layer referred to the same physical b2. That
combination must not turn a fresh blue Play Clue into a Fix. Historical claims
need matching live identity/response evidence or their own surviving effect
provenance. A conditional suffix is not the actionable connection head. The
existing reviewed b2-persistence test and the new giver/recipient table verify
inference and downstream playability; genuine Fix controls remain. This follows
promise lifecycle consistency and documented Fix meaning, not a new strategic
exception.

## Every new rank-Bluff touch needs supported intermediates

In `p4v0s9` turn20’s hypothetical turn22
(`t20-rank-bluff-collateral-needs-giver-known-intermediate`), Bob’s4s to Alice
would bluff Cathy’s y2 and touch g4/y4. Checking only the one-away g4 misses the
two-away y4. Bob’s unidentified3s cannot establish the already-clued y3 required
by that promise. Use pre-clue giver knowledge for newly promised collateral as
well as focus; earlier chop protection is not a prior play promise. A known
visible intermediate or exact own note can support the clue. Preserve the
documented Hard3 exception and established promises. The regression includes
masked forecasts, Bob’s own view, downstream selection, and a known-y3 control.
