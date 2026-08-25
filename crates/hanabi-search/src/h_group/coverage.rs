//! Pinned subsection inventory for the standard H-Group ruleset.
//!
//! This is deliberately more granular than the executable level registry.
//! A level-wide handler is not evidence that every named convention in that
//! chapter exists. Audit tests use this catalog as the source-link side of
//! the coverage contract; executable conventions additionally require a
//! concrete move kind and behavioral regression.

use crate::HGroupProfile;

use super::HGroupLevel;

/// One third-level subsection in the pinned numbered or Max documentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HGroupDocumentationSection {
    pub profile: HGroupProfile,
    pub title: &'static str,
    pub source_url: &'static str,
}

/// Every third-level subsection in levels 1-25 and the Max extras index pages.
pub const H_GROUP_DOCUMENTATION_SECTIONS: [HGroupDocumentationSection; 357] = [
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "The Chop",
        source_url: "https://hanabi.github.io/level-1/#the-chop",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "The Definition of Playable",
        source_url: "https://hanabi.github.io/level-1/#the-definition-of-playable",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "The Definition of Trash",
        source_url: "https://hanabi.github.io/level-1/#the-definition-of-trash",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Save Clues",
        source_url: "https://hanabi.github.io/level-1/#save-clues",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Clue Focus",
        source_url: "https://hanabi.github.io/level-1/#clue-focus",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Good Touch Principle",
        source_url: "https://hanabi.github.io/level-1/#good-touch-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Save Principle",
        source_url: "https://hanabi.github.io/level-1/#save-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Minimum Clue Value Principle",
        source_url: "https://hanabi.github.io/level-1/#minimum-clue-value-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "The Early Game",
        source_url: "https://hanabi.github.io/level-1/#the-early-game",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "The 5 Save",
        source_url: "https://hanabi.github.io/level-1/#the-5-save",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "The 2 Save",
        source_url: "https://hanabi.github.io/level-1/#the-2-save",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "The Prompt",
        source_url: "https://hanabi.github.io/level-1/#the-prompt",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "The Finesse",
        source_url: "https://hanabi.github.io/level-1/#the-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Finesse Position",
        source_url: "https://hanabi.github.io/level-1/#finesse-position",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Finessed Cards",
        source_url: "https://hanabi.github.io/level-1/#finessed-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Prompts > Finesses",
        source_url: "https://hanabi.github.io/level-1/#prompts--finesses",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Card Notes",
        source_url: "https://hanabi.github.io/level-1/#card-notes",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Rewind",
        source_url: "https://hanabi.github.io/level-1/#rewind",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Empathy",
        source_url: "https://hanabi.github.io/level-1/#empathy",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "The 5 Stall (Cluing Off Chop 5's)",
        source_url: "https://hanabi.github.io/level-2/#the-5-stall-cluing-off-chop-5s",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "The Double Prompt / Triple Prompt / Quadruple Prompt",
        source_url: "https://hanabi.github.io/level-2/#the-double-prompt--triple-prompt--quadruple-prompt",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "The Double Finesse / Triple Finesse / Quadruple Finesse",
        source_url: "https://hanabi.github.io/level-2/#the-double-finesse--triple-finesse--quadruple-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "The Prompt + Finesse",
        source_url: "https://hanabi.github.io/level-2/#the-prompt--finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "The Reverse Finesse",
        source_url: "https://hanabi.github.io/level-2/#the-reverse-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "The Self-Finesse",
        source_url: "https://hanabi.github.io/level-2/#the-self-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "Trash",
        source_url: "https://hanabi.github.io/level-2/#trash",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "One-Away-From-Playable Cards",
        source_url: "https://hanabi.github.io/level-2/#one-away-from-playable-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "Stomping on a Finesse",
        source_url: "https://hanabi.github.io/level-2/#stomping-on-a-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "What to Do After a Strike",
        source_url: "https://hanabi.github.io/level-2/#what-to-do-after-a-strike",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "The Wrong Prompt",
        source_url: "https://hanabi.github.io/level-2/#the-wrong-prompt",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "Playing Multiple 1's",
        source_url: "https://hanabi.github.io/level-3/#playing-multiple-1s",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "The Fix Clue",
        source_url: "https://hanabi.github.io/level-3/#the-fix-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "The Fix Clue (That Touches Multiple Cards)",
        source_url: "https://hanabi.github.io/level-3/#the-fix-clue-that-touches-multiple-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "The Fix Clue (That Gives No Additional Information)",
        source_url: "https://hanabi.github.io/level-3/#the-fix-clue-that-gives-no-additional-information",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "The Sarcastic Discard (SD)",
        source_url: "https://hanabi.github.io/level-3/#the-sarcastic-discard-sd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "Misplay Cost Principle",
        source_url: "https://hanabi.github.io/level-3/#misplay-cost-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "Efficiency",
        source_url: "https://hanabi.github.io/level-3/#efficiency",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "Tempo",
        source_url: "https://hanabi.github.io/level-3/#tempo",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "Information Lock Principle",
        source_url: "https://hanabi.github.io/level-3/#information-lock-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "Chop Moves",
        source_url: "https://hanabi.github.io/level-4/#chop-moves",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "The Trash Chop Move (TCM)",
        source_url: "https://hanabi.github.io/level-4/#the-trash-chop-move-tcm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "The 5's Chop Move (5CM)",
        source_url: "https://hanabi.github.io/level-4/#the-5s-chop-move-5cm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "The Order Chop Move (OCM)",
        source_url: "https://hanabi.github.io/level-4/#the-order-chop-move-ocm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "Chop Moves as a Category",
        source_url: "https://hanabi.github.io/level-4/#chop-moves-as-a-category",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "Chop Moves & New Clues",
        source_url: "https://hanabi.github.io/level-4/#chop-moves--new-clues",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "Chop Moves & Prompts",
        source_url: "https://hanabi.github.io/level-4/#chop-moves--prompts",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "Critical Discards after a Chop Move (Mistake)",
        source_url: "https://hanabi.github.io/level-4/#critical-discards-after-a-chop-move-mistake",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "Accidental Chop Moves (Mistake)",
        source_url: "https://hanabi.github.io/level-4/#accidental-chop-moves-mistake",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "Prompts in Multi-Color Variants",
        source_url: "https://hanabi.github.io/level-5/#prompts-in-multi-color-variants",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "The Hidden Finesse",
        source_url: "https://hanabi.github.io/level-5/#the-hidden-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "The Layered Finesse",
        source_url: "https://hanabi.github.io/level-5/#the-layered-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "The Clandestine Finesse",
        source_url: "https://hanabi.github.io/level-5/#the-clandestine-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "The Queued Finesse",
        source_url: "https://hanabi.github.io/level-5/#the-queued-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "The Ambiguous Finesse",
        source_url: "https://hanabi.github.io/level-5/#the-ambiguous-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "The Layered Finesse Dupe Clue",
        source_url: "https://hanabi.github.io/level-5/#the-layered-finesse-dupe-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "Urgency Principle (Playing Into Finesses as Soon as Possible)",
        source_url: "https://hanabi.github.io/level-5/#urgency-principle-playing-into-finesses-as-soon-as-possible",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "Schrödinger's Cat Principle",
        source_url: "https://hanabi.github.io/level-5/#schrodingers-cat-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "The Tempo Clue",
        source_url: "https://hanabi.github.io/level-6/#the-tempo-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "The Valuable Tempo Clue",
        source_url: "https://hanabi.github.io/level-6/#the-valuable-tempo-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "The Tempo Clue Stall (A Non-Valuable Tempo Clue)",
        source_url: "https://hanabi.github.io/level-6/#the-tempo-clue-stall-a-non-valuable-tempo-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "The Tempo Clue Chop Move (TCCM)",
        source_url: "https://hanabi.github.io/level-6/#the-tempo-clue-chop-move-tccm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "Chop Moves & Tempo Clues",
        source_url: "https://hanabi.github.io/level-6/#chop-moves--tempo-clues",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "Focus Shifting",
        source_url: "https://hanabi.github.io/level-6/#focus-shifting",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "Discard Modulation",
        source_url: "https://hanabi.github.io/level-6/#discard-modulation",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "The Value of One-Away-From-Playable Cards",
        source_url: "https://hanabi.github.io/level-6/#the-value-of-one-away-from-playable-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "Clarity Principle",
        source_url: "https://hanabi.github.io/level-6/#clarity-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "The Scream Discard Chop Move (SDCM)",
        source_url: "https://hanabi.github.io/level-7/#the-scream-discard-chop-move-sdcm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "The Scream Discard Chop Move (With Known-Trash)",
        source_url: "https://hanabi.github.io/level-7/#the-scream-discard-chop-move-with-known-trash",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "The Shout Discard Chop Move",
        source_url: "https://hanabi.github.io/level-7/#the-shout-discard-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "The Generation Discard",
        source_url: "https://hanabi.github.io/level-7/#the-generation-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "Lines",
        source_url: "https://hanabi.github.io/level-7/#lines",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "The All 4's Test",
        source_url: "https://hanabi.github.io/level-7/#the-all-4s-test",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "The Definition of Riding",
        source_url: "https://hanabi.github.io/level-7/#the-definition-of-riding",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "Blind-Playing Chop Moved Cards",
        source_url: "https://hanabi.github.io/level-7/#blind-playing-chop-moved-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "The End-Game",
        source_url: "https://hanabi.github.io/level-8/#the-end-game",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "No Chop Moves in the End-Game",
        source_url: "https://hanabi.github.io/level-8/#no-chop-moves-in-the-end-game",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "The Positional Discard (Indicating a Play with a Discard)",
        source_url: "https://hanabi.github.io/level-8/#the-positional-discard-indicating-a-play-with-a-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "The Positional Misplay (Indicating a Play with a Misplay)",
        source_url: "https://hanabi.github.io/level-8/#the-positional-misplay-indicating-a-play-with-a-misplay",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "The Double Positional Misplay (Indicating Two Plays with a Misplay)",
        source_url: "https://hanabi.github.io/level-8/#the-double-positional-misplay-indicating-two-plays-with-a-misplay",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "The Distribution Clue",
        source_url: "https://hanabi.github.io/level-8/#the-distribution-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "Team Distribution Principle",
        source_url: "https://hanabi.github.io/level-8/#team-distribution-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "The Pace +1 Rule",
        source_url: "https://hanabi.github.io/level-8/#the-pace-1-rule",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "Burning (End-Game Stalling)",
        source_url: "https://hanabi.github.io/level-8/#burning-end-game-stalling",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "End-Game Strategy",
        source_url: "https://hanabi.github.io/level-8/#end-game-strategy",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "Stalling Situations",
        source_url: "https://hanabi.github.io/level-9/#stalling-situations",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "Allowable Stall Clues (Stall Table)",
        source_url: "https://hanabi.github.io/level-9/#allowable-stall-clues-stall-table",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "The Early Game (Severity 1 Stalling)",
        source_url: "https://hanabi.github.io/level-9/#the-early-game-severity-1-stalling",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "Double Discard Situations / Double Discard Avoidance (DDA) (Severity 2 Stalling)",
        source_url: "https://hanabi.github.io/level-9/#double-discard-situations--double-discard-avoidance-dda-severity-2-stalling",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "Locked Hands (Severity 3 Stalling)",
        source_url: "https://hanabi.github.io/level-9/#locked-hands-severity-3-stalling",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "Clues Given While at 8 Clues (Severity 4 Stalling)",
        source_url: "https://hanabi.github.io/level-9/#clues-given-while-at-8-clues-severity-4-stalling",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "The 5 Stall (Intermediate Section)",
        source_url: "https://hanabi.github.io/level-9/#the-5-stall-intermediate-section",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "The Fill-In Clue",
        source_url: "https://hanabi.github.io/level-9/#the-fill-in-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "The Locked Hand Save (LHS)",
        source_url: "https://hanabi.github.io/level-9/#the-locked-hand-save-lhs",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "The Anxiety Play (Forcing a Locked Player to Play)",
        source_url: "https://hanabi.github.io/level-9/#the-anxiety-play-forcing-a-locked-player-to-play",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "The 8 Clue Save (8CS)",
        source_url: "https://hanabi.github.io/level-9/#the-8-clue-save-8cs",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "The Gentleman's Discard (GD)",
        source_url: "https://hanabi.github.io/level-10/#the-gentlemans-discard-gd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "The Layered Gentleman's Discard",
        source_url: "https://hanabi.github.io/level-10/#the-layered-gentlemans-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "The Baton Discard (BD)",
        source_url: "https://hanabi.github.io/level-10/#the-baton-discard-bd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "The Sarcastic Finesse",
        source_url: "https://hanabi.github.io/level-10/#the-sarcastic-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "The Certain Finesse & The Certain Discard",
        source_url: "https://hanabi.github.io/level-10/#the-certain-finesse--the-certain-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "The Composition Finesse",
        source_url: "https://hanabi.github.io/level-10/#the-composition-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "Directness Principle",
        source_url: "https://hanabi.github.io/level-10/#directness-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "The Double Gentleman's Discard (Illegal)",
        source_url: "https://hanabi.github.io/level-10/#the-double-gentlemans-discard-illegal",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "The Bluff",
        source_url: "https://hanabi.github.io/level-11/#the-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "The Self-Bluff",
        source_url: "https://hanabi.github.io/level-11/#the-self-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Bluffs Through Already-Clued Cards",
        source_url: "https://hanabi.github.io/level-11/#bluffs-through-already-clued-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Bob's Truth Principle (Part 1)",
        source_url: "https://hanabi.github.io/level-11/#bobs-truth-principle-part-1",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Cathy's Connecting Principle (Part 2)",
        source_url: "https://hanabi.github.io/level-11/#cathys-connecting-principle-part-2",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Guide Principle",
        source_url: "https://hanabi.github.io/level-11/#guide-principle",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Clue Interpretation & Occam's Razor",
        source_url: "https://hanabi.github.io/level-11/#clue-interpretation--occams-razor",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "The Pang of Guilt",
        source_url: "https://hanabi.github.io/level-11/#the-pang-of-guilt",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Mistaking a Layered Finesse for a Bluff",
        source_url: "https://hanabi.github.io/level-11/#mistaking-a-layered-finesse-for-a-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Bluff Prompts / Prompt Bluffs (Illegal)",
        source_url: "https://hanabi.github.io/level-11/#bluff-prompts--prompt-bluffs-illegal",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Queued Bluffs (Illegal)",
        source_url: "https://hanabi.github.io/level-11/#queued-bluffs-illegal",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Assuming Asymmetric Information",
        source_url: "https://hanabi.github.io/level-12/#assuming-asymmetric-information",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Duplication Responsibility",
        source_url: "https://hanabi.github.io/level-12/#duplication-responsibility",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "The Selfish Clue",
        source_url: "https://hanabi.github.io/level-12/#the-selfish-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "The Selfish Finesse (A Finesse Through Your Own Hand)",
        source_url: "https://hanabi.github.io/level-12/#the-selfish-finesse-a-finesse-through-your-own-hand",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "The Ego Clue (Mistake)",
        source_url: "https://hanabi.github.io/level-12/#the-ego-clue-mistake",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "The Stale 1's Clue",
        source_url: "https://hanabi.github.io/level-12/#the-stale-1s-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Focus Inversion",
        source_url: "https://hanabi.github.io/level-12/#focus-inversion",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Context",
        source_url: "https://hanabi.github.io/level-12/#context",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Choosing Between Playable Cards",
        source_url: "https://hanabi.github.io/level-12/#choosing-between-playable-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Cluing 1's in the Early Game",
        source_url: "https://hanabi.github.io/level-12/#cluing-1s-in-the-early-game",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "The 3 Bluff",
        source_url: "https://hanabi.github.io/level-13/#the-3-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "The Critical Color Bluff (CCB)",
        source_url: "https://hanabi.github.io/level-13/#the-critical-color-bluff-ccb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "The Hard Bluff",
        source_url: "https://hanabi.github.io/level-13/#the-hard-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "The Hard 3 Bluff",
        source_url: "https://hanabi.github.io/level-13/#the-hard-3-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "The Known Bluff",
        source_url: "https://hanabi.github.io/level-13/#the-known-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "The Good Touch Bluff",
        source_url: "https://hanabi.github.io/level-13/#the-good-touch-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "Legal Bluff-Targets",
        source_url: "https://hanabi.github.io/level-13/#legal-bluff-targets",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "Known-Trash Discard Order",
        source_url: "https://hanabi.github.io/level-14/#known-trash-discard-order",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "The Trash Order Chop Move (TOCM)",
        source_url: "https://hanabi.github.io/level-14/#the-trash-order-chop-move-tocm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "The Shout Discard Order Chop Move",
        source_url: "https://hanabi.github.io/level-14/#the-shout-discard-order-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "The Trash Push",
        source_url: "https://hanabi.github.io/level-14/#the-trash-push",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "The Trash Push Prompt & The Trash Push Finesse",
        source_url: "https://hanabi.github.io/level-14/#the-trash-push-prompt--the-trash-push-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "The Trash Finesse",
        source_url: "https://hanabi.github.io/level-14/#the-trash-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "The Reverse Trash Finesse",
        source_url: "https://hanabi.github.io/level-14/#the-reverse-trash-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "The Forced Gentleman's Discard Chop Move",
        source_url: "https://hanabi.github.io/level-14/#the-forced-gentlemans-discard-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "The Trash Bluff",
        source_url: "https://hanabi.github.io/level-14/#the-trash-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "The Double Bluff",
        source_url: "https://hanabi.github.io/level-15/#the-double-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "The Triple Bluff (Illegal)",
        source_url: "https://hanabi.github.io/level-15/#the-triple-bluff-illegal",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "The Hard Double Bluff",
        source_url: "https://hanabi.github.io/level-15/#the-hard-double-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "The Pestilent Double Bluff (PDB)",
        source_url: "https://hanabi.github.io/level-15/#the-pestilent-double-bluff-pdb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "Deferring a Bluff",
        source_url: "https://hanabi.github.io/level-15/#deferring-a-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "Deferring a Double Bluff",
        source_url: "https://hanabi.github.io/level-15/#deferring-a-double-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "A Table for Deferring Bluffs",
        source_url: "https://hanabi.github.io/level-15/#a-table-for-deferring-bluffs",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "Interaction between Bob's Truth Principle and Occam's Razor",
        source_url: "https://hanabi.github.io/level-15/#interaction-between-bobs-truth-principle-and-occams-razor",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "Ejections",
        source_url: "https://hanabi.github.io/level-16/#ejections",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "Discharges",
        source_url: "https://hanabi.github.io/level-16/#discharges",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "The 5 Color Ejection (5CE)",
        source_url: "https://hanabi.github.io/level-16/#the-5-color-ejection-5ce",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "The Unknown Trash Discharge (1-for-1 Form) (UTD)",
        source_url: "https://hanabi.github.io/level-16/#the-unknown-trash-discharge-1-for-1-form-utd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "The Unknown Trash Discharge (2-for-1 Form) (UTD)",
        source_url: "https://hanabi.github.io/level-16/#the-unknown-trash-discharge-2-for-1-form-utd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "The Unknown Dupe Discharge (UDD)",
        source_url: "https://hanabi.github.io/level-16/#the-unknown-dupe-discharge-udd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "The Duplicitous Value Clue",
        source_url: "https://hanabi.github.io/level-17/#the-duplicitous-value-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "The Duplicitous Blind-Play",
        source_url: "https://hanabi.github.io/level-17/#the-duplicitous-blind-play",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "The Duplicitous Tempo Clue",
        source_url: "https://hanabi.github.io/level-17/#the-duplicitous-tempo-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "The Assisted Trash Chop Move",
        source_url: "https://hanabi.github.io/level-17/#the-assisted-trash-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "The Time Travel Chop Move (Direct Form)",
        source_url: "https://hanabi.github.io/level-17/#the-time-travel-chop-move-direct-form",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "The Time Travel Chop Move (Blind-Play Form)",
        source_url: "https://hanabi.github.io/level-17/#the-time-travel-chop-move-blind-play-form",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "Elimination & Elimination Notes",
        source_url: "https://hanabi.github.io/level-18/#elimination--elimination-notes",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "Double Discard Elimination",
        source_url: "https://hanabi.github.io/level-18/#double-discard-elimination",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "2 Elimination",
        source_url: "https://hanabi.github.io/level-18/#2-elimination",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "The Elimination Blind-Play",
        source_url: "https://hanabi.github.io/level-18/#the-elimination-blind-play",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "The Elimination Play Clue",
        source_url: "https://hanabi.github.io/level-18/#the-elimination-play-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "Interaction Between Elimination & Chop-Focus",
        source_url: "https://hanabi.github.io/level-18/#interaction-between-elimination--chop-focus",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "The Elimination Riding Deduction",
        source_url: "https://hanabi.github.io/level-18/#the-elimination-riding-deduction",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "The Riding Deduction Bluff",
        source_url: "https://hanabi.github.io/level-18/#the-riding-deduction-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "The Elimination Self-Chop Move",
        source_url: "https://hanabi.github.io/level-18/#the-elimination-self-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "The Elimination Finesse",
        source_url: "https://hanabi.github.io/level-18/#the-elimination-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "Trash Touch Elimination (TTE)",
        source_url: "https://hanabi.github.io/level-18/#trash-touch-elimination-tte",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The Low Score Phase and the Normal Score Phase",
        source_url: "https://hanabi.github.io/level-19/#the-low-score-phase-and-the-normal-score-phase",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "No Play Clues with a Number 5 Clue in the Low Score Phase",
        source_url: "https://hanabi.github.io/level-19/#no-play-clues-with-a-number-5-clue-in-the-low-score-phase",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The Early 5's Chop Move",
        source_url: "https://hanabi.github.io/level-19/#the-early-5s-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Pull",
        source_url: "https://hanabi.github.io/level-19/#the-5-pull",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Pull Prompt & The 5 Pull Finesse",
        source_url: "https://hanabi.github.io/level-19/#the-5-pull-prompt--the-5-pull-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Pull Double Finesse",
        source_url: "https://hanabi.github.io/level-19/#the-5-pull-double-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Pull Clandestine Finesse",
        source_url: "https://hanabi.github.io/level-19/#the-5-pull-clandestine-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Pull Promise (A Play Clue After a 5 Pull)",
        source_url: "https://hanabi.github.io/level-19/#the-5-pull-promise-a-play-clue-after-a-5-pull",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "Finesses While 5 Pulled are Certain Finesses",
        source_url: "https://hanabi.github.io/level-19/#finesses-while-5-pulled-are-certain-finesses",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Pull Skip",
        source_url: "https://hanabi.github.io/level-19/#the-5-pull-skip",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Number Ejection (5NE)",
        source_url: "https://hanabi.github.io/level-19/#the-5-number-ejection-5ne",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Number Discharge (5ND)",
        source_url: "https://hanabi.github.io/level-19/#the-5-number-discharge-5nd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "The 5 Number Ejection Finesse Position Skips",
        source_url: "https://hanabi.github.io/level-19/#the-5-number-ejection-finesse-position-skips",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "A Flowchart for Cluing 5's",
        source_url: "https://hanabi.github.io/level-19/#a-flowchart-for-cluing-5s",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "Interaction Between 2 Saves & 5 Stalls",
        source_url: "https://hanabi.github.io/level-19/#interaction-between-2-saves--5-stalls",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "The Occupied Play Clue & The Occupied Finesse (OPC)",
        source_url: "https://hanabi.github.io/level-20/#the-occupied-play-clue--the-occupied-finesse-opc",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "The Out-of-Order Play Clue (Triple O / OOO)",
        source_url: "https://hanabi.github.io/level-20/#the-out-of-order-play-clue-triple-o--ooo",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "The Out-of-Order Finesse",
        source_url: "https://hanabi.github.io/level-20/#the-out-of-order-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "The Out-of-Order Corollary",
        source_url: "https://hanabi.github.io/level-20/#the-out-of-order-corollary",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "The Suboptimal Prompt & The Suboptimal Finesse & The Suboptimal Bluff",
        source_url: "https://hanabi.github.io/level-20/#the-suboptimal-prompt--the-suboptimal-finesse--the-suboptimal-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "The No-Information Finesse",
        source_url: "https://hanabi.github.io/level-20/#the-no-information-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "The No-Information Double Bluff (NIDB)",
        source_url: "https://hanabi.github.io/level-20/#the-no-information-double-bluff-nidb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "Ignition",
        source_url: "https://hanabi.github.io/level-21/#ignition",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "Double Ignition (DI)",
        source_url: "https://hanabi.github.io/level-21/#double-ignition-di",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "The Replay Double Ignition (RDI)",
        source_url: "https://hanabi.github.io/level-21/#the-replay-double-ignition-rdi",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "The Trash Double Ignition (TDI)",
        source_url: "https://hanabi.github.io/level-21/#the-trash-double-ignition-tdi",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "The Poke Double Ignition (PDI)",
        source_url: "https://hanabi.github.io/level-21/#the-poke-double-ignition-pdi",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "The Chop Move Ignition (CMI) (with 1 card Chop Moved)",
        source_url: "https://hanabi.github.io/level-21/#the-chop-move-ignition-cmi-with-1-card-chop-moved",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "The Chop Move Ignition (CMI) (with 2+ cards Chop Moved)",
        source_url: "https://hanabi.github.io/level-21/#the-chop-move-ignition-cmi-with-2-cards-chop-moved",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "Bomb Double Ignition",
        source_url: "https://hanabi.github.io/level-21/#bomb-double-ignition",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "Bomb Triple Ignition",
        source_url: "https://hanabi.github.io/level-21/#bomb-triple-ignition",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "Phantom Playable Cards",
        source_url: "https://hanabi.github.io/level-22/#phantom-playable-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "The Scream Discard for a Phantom Playable Card",
        source_url: "https://hanabi.github.io/level-22/#the-scream-discard-for-a-phantom-playable-card",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "The Sacrifice Discard",
        source_url: "https://hanabi.github.io/level-22/#the-sacrifice-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "The Echo Scream Discard Chop Move (ESDCM)",
        source_url: "https://hanabi.github.io/level-22/#the-echo-scream-discard-chop-move-esdcm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "The Echo Shout Discard (Illegal)",
        source_url: "https://hanabi.github.io/level-22/#the-echo-shout-discard-illegal",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "The Composition Discard",
        source_url: "https://hanabi.github.io/level-22/#the-composition-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "The Rebellious Discard",
        source_url: "https://hanabi.github.io/level-22/#the-rebellious-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "A Scream Discard Flowchart",
        source_url: "https://hanabi.github.io/level-22/#a-scream-discard-flowchart",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level23),
        title: "Charms",
        source_url: "https://hanabi.github.io/level-23/#charms",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level23),
        title: "The 4 Charm",
        source_url: "https://hanabi.github.io/level-23/#the-4-charm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level23),
        title: "The Blaze Discard",
        source_url: "https://hanabi.github.io/level-23/#the-blaze-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level23),
        title: "The Hesitation Blind-Play",
        source_url: "https://hanabi.github.io/level-23/#the-hesitation-blind-play",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level24),
        title: "Trash Finesses and Trash Bluffs Are Always Unnecessary",
        source_url: "https://hanabi.github.io/level-24/#trash-finesses-and-trash-bluffs-are-always-unnecessary",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level24),
        title: "Unnecessary Moves with Known Trash --> Ignition",
        source_url: "https://hanabi.github.io/level-24/#unnecessary-moves-with-known-trash----ignition",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level24),
        title: "Unnecessary Moves with Unknown Trash Off Chop --> Chop Move",
        source_url: "https://hanabi.github.io/level-24/#unnecessary-moves-with-unknown-trash-off-chop----chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level24),
        title: "Unnecessary Moves with Unknown Trash On Chop --> Trash Push",
        source_url: "https://hanabi.github.io/level-24/#unnecessary-moves-with-unknown-trash-on-chop----trash-push",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level24),
        title: "Other Examples",
        source_url: "https://hanabi.github.io/level-24/#other-examples",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "The Priority Prompt & The Priority Finesse",
        source_url: "https://hanabi.github.io/level-25/#the-priority-prompt--the-priority-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "The Priority Bluff",
        source_url: "https://hanabi.github.io/level-25/#the-priority-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "The Layered Priority Finesse",
        source_url: "https://hanabi.github.io/level-25/#the-layered-priority-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "The Load Clue",
        source_url: "https://hanabi.github.io/level-25/#the-load-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "The Paused Priority Finesse",
        source_url: "https://hanabi.github.io/level-25/#the-paused-priority-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "The Trust Finesse (A Priority Finesse From Playing an Unknown Card)",
        source_url: "https://hanabi.github.io/level-25/#the-trust-finesse-a-priority-finesse-from-playing-an-unknown-card",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "A Priority Flowchart (for Choosing Between 2+ Playable Cards)",
        source_url: "https://hanabi.github.io/level-25/#a-priority-flowchart-for-choosing-between-2-playable-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "Priority with Blind-Plays",
        source_url: "https://hanabi.github.io/level-25/#priority-with-blind-plays",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "Priority with Both Known and Unknown Cards",
        source_url: "https://hanabi.github.io/level-25/#priority-with-both-known-and-unknown-cards",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "Situations Where Priority Does Not Apply",
        source_url: "https://hanabi.github.io/level-25/#situations-where-priority-does-not-apply",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "Playing Into Someone Else's Hand",
        source_url: "https://hanabi.github.io/level-25/#playing-into-someone-elses-hand",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 4 Charm",
        source_url: "https://hanabi.github.io/extras/charms/#the-4-charm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Unknown Trash Charm (UTC)",
        source_url: "https://hanabi.github.io/extras/charms/#the-unknown-trash-charm-utc",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Junk Charm (for 1's)",
        source_url: "https://hanabi.github.io/extras/charms/#the-junk-charm-for-1s",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Chop Move (TCM)",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-trash-chop-move-tcm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 5's Chop Move (5CM)",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-5s-chop-move-5cm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Order Chop Move (OCM)",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-order-chop-move-ocm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Tempo Clue Chop Move (TCCM)",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-tempo-clue-chop-move-tccm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Scream Discard Chop Move (SDCM)",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-scream-discard-chop-move-sdcm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Shout Discard Chop Move",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-shout-discard-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Order Chop Move (TOCM)",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-trash-order-chop-move-tocm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Shout Discard Order Chop Move",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-shout-discard-order-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Assisted Trash Chop Move",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-assisted-trash-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Time Travel Chop Move",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-time-travel-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Echo Scream Discard Chop Move (ESDCM)",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-echo-scream-discard-chop-move-esdcm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Transfer Chop Move",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-transfer-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Misplay Chop Move",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-misplay-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Double Order Chop Move (for 3-Player Games)",
        source_url: "https://hanabi.github.io/extras/chop-moves/#double-order-chop-move-for-3-player-games",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Spillover Chop Move",
        source_url: "https://hanabi.github.io/extras/chop-moves/#spillover-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Negative Self-Chop Move",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-negative-self-chop-move",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Asymmetric Chop Move Dilemma",
        source_url: "https://hanabi.github.io/extras/chop-moves/#the-asymmetric-chop-move-dilemma",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Sarcastic Discard (SD)",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-sarcastic-discard-sd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Scream Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-scream-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Shout Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-shout-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Generation Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-generation-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Positional Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-positional-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Positional Misplay",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-positional-misplay",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Double Positional Misplay",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-double-positional-misplay",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Gentleman's Discard (GD)",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-gentlemans-discard-gd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Layered Gentleman's Discard (GD)",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-layered-gentlemans-discard-gd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Baton Discard (BD)",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-baton-discard-bd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Certain Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-certain-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Sacrifice Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-sacrifice-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Echo Scream Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-echo-scream-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Composition Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-composition-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Rebellious Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-rebellious-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Blaze Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-blaze-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Cautious Generation Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-cautious-generation-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Promise Clue & The Promise Discard",
        source_url: "https://hanabi.github.io/extras/discards-misplays/#the-promise-clue--the-promise-discard",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Unknown Trash Discharge (UTD)",
        source_url: "https://hanabi.github.io/extras/discharges/#the-unknown-trash-discharge-utd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Unknown Dupe Discharge (UDD)",
        source_url: "https://hanabi.github.io/extras/discharges/#the-unknown-dupe-discharge-udd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 5 Number Discharge (5ND)",
        source_url: "https://hanabi.github.io/extras/discharges/#the-5-number-discharge-5nd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Push Discharge (TPD)",
        source_url: "https://hanabi.github.io/extras/discharges/#the-trash-push-discharge-tpd",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Out-of-Position Ejection",
        source_url: "https://hanabi.github.io/extras/ejection-extensions/#the-out-of-position-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Out-of-Position Discharge/Charm",
        source_url: "https://hanabi.github.io/extras/ejection-extensions/#the-out-of-position-dischargecharm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Stacked Ejection",
        source_url: "https://hanabi.github.io/extras/ejection-extensions/#the-stacked-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Stacked Discharge/Charm",
        source_url: "https://hanabi.github.io/extras/ejection-extensions/#the-stacked-dischargecharm",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Double Ejection",
        source_url: "https://hanabi.github.io/extras/ejection-extensions/#the-double-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 5 Color Ejection (5CE)",
        source_url: "https://hanabi.github.io/extras/ejections/#the-5-color-ejection-5ce",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 5 Number Ejection (5NE)",
        source_url: "https://hanabi.github.io/extras/ejections/#the-5-number-ejection-5ne",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Trash Push Ejection",
        source_url: "https://hanabi.github.io/extras/ejections/#trash-push-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Bad Chop Move Ejection (BCME)",
        source_url: "https://hanabi.github.io/extras/ejections/#the-bad-chop-move-ejection-bcme",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Bad Trash Finesse Ejection / The Bad Trash Bluff Ejection",
        source_url: "https://hanabi.github.io/extras/ejections/#the-bad-trash-finesse-ejection--the-bad-trash-bluff-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Finesse Push Ejection / The Trash Bluff Push Ejection",
        source_url: "https://hanabi.github.io/extras/ejections/#the-trash-finesse-push-ejection--the-trash-bluff-push-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Rank Choice Ejection (with a number 2 or a number 5) (RCE)",
        source_url: "https://hanabi.github.io/extras/ejections/#the-rank-choice-ejection-with-a-number-2-or-a-number-5-rce",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Ejection",
        source_url: "https://hanabi.github.io/extras/ejections/#the-trash-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Replay Ejection",
        source_url: "https://hanabi.github.io/extras/ejections/#the-replay-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Poke Ejection",
        source_url: "https://hanabi.github.io/extras/ejections/#the-poke-ejection",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Just-In-Time Fix Clue (JIT)",
        source_url: "https://hanabi.github.io/extras/fix-clues/#the-just-in-time-fix-clue-jit",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "No Valid First Turn Clues",
        source_url: "https://hanabi.github.io/extras/miscellaneous/#no-valid-first-turn-clues",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Double Prompts in Multi-Color Variants",
        source_url: "https://hanabi.github.io/extras/miscellaneous/#double-prompts-in-multi-color-variants",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Elimination Rewrite (for 1's)",
        source_url: "https://hanabi.github.io/extras/miscellaneous/#the-elimination-rewrite-for-1s",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Negative Blind-Play",
        source_url: "https://hanabi.github.io/extras/miscellaneous/#the-negative-blind-play",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Prompt",
        source_url: "https://hanabi.github.io/extras/play-clues/#the-prompt",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Finesse",
        source_url: "https://hanabi.github.io/extras/play-clues/#the-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Tempo Clue",
        source_url: "https://hanabi.github.io/extras/play-clues/#the-tempo-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Distribution Clue",
        source_url: "https://hanabi.github.io/extras/play-clues/#the-distribution-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Bluff",
        source_url: "https://hanabi.github.io/extras/play-clues/#the-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Selfish Clue",
        source_url: "https://hanabi.github.io/extras/play-clues/#the-selfish-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Load Clue",
        source_url: "https://hanabi.github.io/extras/play-clues/#the-load-clue",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Continuation Clue (Touching Both Inside and Outside a Layer)",
        source_url: "https://hanabi.github.io/extras/play-clues/#the-continuation-clue-touching-both-inside-and-outside-a-layer",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Push",
        source_url: "https://hanabi.github.io/extras/pushes-pulls/#the-trash-push",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 5 Pull",
        source_url: "https://hanabi.github.io/extras/pushes-pulls/#the-5-pull",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Pull",
        source_url: "https://hanabi.github.io/extras/pushes-pulls/#the-trash-pull",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 5 Save",
        source_url: "https://hanabi.github.io/extras/save-clues/#the-5-save",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 2 Save",
        source_url: "https://hanabi.github.io/extras/save-clues/#the-2-save",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Locked Hand Save (LHS)",
        source_url: "https://hanabi.github.io/extras/save-clues/#the-locked-hand-save-lhs",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 8 Clue Save (8CS)",
        source_url: "https://hanabi.github.io/extras/save-clues/#the-8-clue-save-8cs",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Fake Save",
        source_url: "https://hanabi.github.io/extras/save-clues/#the-fake-save",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Saving Playable Cards when the Preceding Cards Are Not Promptable",
        source_url: "https://hanabi.github.io/extras/save-clues/#saving-playable-cards-when-the-preceding-cards-are-not-promptable",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 3 Bluff",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-3-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Critical Color Bluff (CCB)",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-critical-color-bluff-ccb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Good Touch Bluff",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-good-touch-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Bluff",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-trash-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Pestilent Double Bluff (PDB)",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-pestilent-double-bluff-pdb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The No-Information Double Bluff (NIDB)",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-no-information-double-bluff-nidb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Priority Bluff",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-priority-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Self Color Bluffs (1-for-1 Form) (SCB)",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#self-color-bluffs-1-for-1-form-scb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Self Color Bluff (2-for-1 Form) (SCB)",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#self-color-bluff-2-for-1-form-scb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Self Color Double Bluff (SCDB)",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#self-color-double-bluff-scdb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Queued Bluffs (Exception)",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#queued-bluffs-exception",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Elimination Bluff & The Elimination Layered Finesse",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-elimination-bluff--the-elimination-layered-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Known Priority Bluff",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-known-priority-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Pestilent Triple Bluff",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-pestilent-triple-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Pass Bluff",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-pass-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Double/Triple Pass Bluff",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-doubletriple-pass-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Purge Bluff (Layered Bluff)",
        source_url: "https://hanabi.github.io/extras/special-bluffs/#the-purge-bluff-layered-bluff",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Hidden Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-hidden-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Layered Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-layered-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Clandestine Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-clandestine-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Queued Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-queued-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Ambiguous Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-ambiguous-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Sarcastic Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-sarcastic-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Certain Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-certain-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Composition Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-composition-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Selfish Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-selfish-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-trash-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trash Push Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-trash-push-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Elimination Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-elimination-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The 5 Pull Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-5-pull-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Occupied Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-occupied-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Out-of-Order Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-out-of-order-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Suboptimal Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-suboptimal-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The No-Information Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-no-information-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Priority Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-priority-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Trust Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-trust-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Ambiguous Finesse Pass-Back (AFPB)",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-ambiguous-finesse-pass-back-afpb",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Potential Priority Duplication & The Certain Priority Finesse (or Priority Certain Finesse)",
        source_url: "https://hanabi.github.io/extras/special-finesses/#potential-priority-duplication--the-certain-priority-finesse-or-priority-certain-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Certain Finesse Clandestine Exception",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-certain-finesse-clandestine-exception",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Patch Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-patch-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Known Patch Finesse (Illegal)",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-known-patch-finesse-illegal",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Double Patch Finesse (Illegal)",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-double-patch-finesse-illegal",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Patch Gentleman's Discard (Illegal)",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-patch-gentlemans-discard-illegal",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Surreptitious Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-surreptitious-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Inverted Priority Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#inverted-priority-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "Finesses with a Lie Component",
        source_url: "https://hanabi.github.io/extras/special-finesses/#finesses-with-a-lie-component",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Declined 5's Finesse",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-declined-5s-finesse",
    },
    HGroupDocumentationSection {
        profile: HGroupProfile::Max,
        title: "The Rank Choice Save Finesse / The Rank Choice Save Bluff",
        source_url: "https://hanabi.github.io/extras/special-finesses/#the-rank-choice-save-finesse--the-rank-choice-save-bluff",
    },
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn pinned_subsection_sources_are_complete_and_unique() {
        assert_eq!(H_GROUP_DOCUMENTATION_SECTIONS.len(), 357);
        let mut urls = HashSet::new();
        for section in H_GROUP_DOCUMENTATION_SECTIONS {
            assert!(section.source_url.starts_with("https://hanabi.github.io/"));
            assert!(
                urls.insert(section.source_url),
                "duplicate source URL: {}",
                section.source_url
            );
        }
    }

    #[test]
    fn pinned_subsection_counts_cover_every_numbered_level_and_max() {
        let expected = [
            19, 11, 9, 9, 9, 9, 8, 10, 11, 8, 11, 10, 7, 9, 8, 6, 6, 11, 15, 7, 9, 8, 4, 5, 11,
        ];
        for (index, expected_count) in expected.into_iter().enumerate() {
            let level = HGroupLevel::try_from(
                u8::try_from(index + 1).expect("the numbered level fits in u8"),
            )
            .expect("levels 1 through 25 exist");
            let count = H_GROUP_DOCUMENTATION_SECTIONS
                .iter()
                .filter(|section| section.profile == HGroupProfile::Level(level))
                .count();
            assert_eq!(
                count,
                expected_count,
                "incomplete Level {} inventory",
                index + 1
            );
        }
        assert_eq!(
            H_GROUP_DOCUMENTATION_SECTIONS
                .iter()
                .filter(|section| section.profile == HGroupProfile::Max)
                .count(),
            127
        );
    }
}
