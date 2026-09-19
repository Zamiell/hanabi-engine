"""Position-key invariants, not invented convention expectations."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from replay_reviews import anchor_status, position_key


class ReviewAnchorTests(unittest.TestCase):
    def test_only_setup_and_prefix_determine_position(self) -> None:
        game = {"seed": "example", "actions": [1, 2, 3]}
        key = position_key(game, 2)
        self.assertEqual(key, position_key({"actions": [1, 9], "seed": "example"}, 2))
        self.assertNotEqual(key, position_key({**game, "actions": [9, 2, 3]}, 2))
        self.assertNotEqual(key, position_key({**game, "seed": "other"}, 2))
        self.assertNotEqual(key, position_key({**game, "options": {}}, 2))

    def test_stale_and_unanchored_are_not_matches(self) -> None:
        game = {"seed": "example", "actions": [1]}
        entry = {"turn": 1, "positionKey": position_key(game, 1)}
        self.assertEqual(anchor_status(entry, game), "MATCH")
        self.assertTrue(anchor_status({**entry, "positionKey": None}, game).startswith("UNANCHORED"))
        self.assertTrue(anchor_status({**entry, "positionKey": "old"}, game).startswith("STALE"))
        self.assertTrue(anchor_status({**entry, "turn": 5}, game).startswith("STALE"))

    def test_one_based_range(self) -> None:
        game = {"actions": [1]}
        for turn in [0, -1, 3]:
            with self.assertRaises(ValueError):
                position_key(game, turn)
        position_key(game, 2)  # End position is valid too.
