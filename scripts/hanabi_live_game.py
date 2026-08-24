from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from hanabi_live_engine import PersistentEngine

@dataclass
class LiveGame:
    """Typed mutable state for one attended Hanabi Live game."""

    table_id: int
    player_names: list[str]
    our_player_index: int
    spectating: bool
    replay: bool
    options: dict[str, Any]
    generation: int
    h_group_level: str | None
    actions: list[dict[str, Any]] = field(default_factory=list)
    action_list_loaded: bool = False
    turn: int = 0
    current_player: int = 0
    terminal: bool = False
    in_flight: bool = False
    last_decided_turn: int | None = None
    synced_actions: int = 0
    engine_initialized: bool = False
    engine: PersistentEngine | None = None

    def reset_engine(self) -> PersistentEngine | None:
        old = self.engine
        self.engine = None
        self.engine_initialized = False
        self.synced_actions = 0
        return old

    def load_actions(self, actions: list[dict[str, Any]], generation: int) -> None:
        self.generation = generation
        self.reset_engine()
        self.actions = actions
        self.action_list_loaded = True
        self.turn = 0
        self.current_player = 0
        self.terminal = False
        self.in_flight = False
        self.last_decided_turn = None
        for action in actions:
            self.update_progress(action)

    def update_progress(self, action: dict[str, Any]) -> None:
        action_type = action.get("type")
        if action_type == "turn":
            self.turn = int(action["num"])
            self.current_player = int(action["currentPlayerIndex"])
        elif action_type == "gameOver":
            self.terminal = True

    def snapshot(self, action_count: int) -> dict[str, Any]:
        return {
            "tableID": self.table_id,
            "playerNames": self.player_names,
            "ourPlayerIndex": self.our_player_index,
            "spectating": self.spectating,
            "replay": self.replay,
            "options": self.options,
            "actions": self.actions[:action_count],
        }

    def close_engine(self) -> None:
        engine = self.reset_engine()
        if engine is not None:
            engine.close()

