"""Audit combat effect occurrences against generated runtime rules."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")
sys.stderr.reconfigure(encoding="utf-8")


REPO = Path(__file__).resolve().parents[1]
WORKSPACE = REPO.parent
sys.path.insert(0, str(WORKSPACE / "tools" / "build"))

from build_fresh_state_rules import decode_master, load_probe  # noqa: E402
from build_gameplay_rules import burst_gauge_max  # noqa: E402


class AuditError(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AuditError(message)


def reachable_owner_ids(master: dict) -> dict[str, set[int]]:
    skill_ids = set()
    ability_ids = set()
    for character in master["character"]:
        for name, values in character.items():
            if name.endswith("_skill_ids") and isinstance(values, list):
                skill_ids.update(int(value) for value in values if value)
            if "ability" in name and isinstance(values, list):
                ability_ids.update(int(value) for value in values if isinstance(value, int))
        skill_ids.update(
            int(character[name])
            for name in ("active1_skill_id", "active2_skill_id", "active3_skill_id")
            if character.get(name)
        )
        ability_ids.update(
            int(row["ability_id"])
            for row in (character.get("leader_skill") or {}).get("abilities") or []
        )
    skill_ids.update(int(row["skill_id"]) for row in master["battle_tool"])
    skill_ids.update(int(row["skill_id"]) for row in master["battle_tool_mix"])
    skill_ids.update(
        int(skill_id) for row in master["ship_tool"] for skill_id in row["skill_ids"]
    )
    skill_ids.update(
        int(row["skill_id"]) for row in master["hyperlink"] if row.get("skill_id")
    )
    for table in ("equipment_tool", "equipment_tool_trait", "memoria", "ship_level"):
        ability_ids.update(
            int(ability_id)
            for row in master[table]
            for ability_id in row.get("ability_ids") or []
        )
    ability_ids.update(
        int(ability_id)
        for quest in master["quest"]
        for ability_id in quest["field_ability_ids"]
    )

    wave_ids = {
        int(wave_id) for battle in master["battle"] for wave_id in battle["wave_ids"]
    }
    enemy_ids = {
        int(enemy["id"])
        for wave in master["wave"]
        if int(wave["id"]) in wave_ids
        for enemy in wave["enemies"]
    }
    enemies = {int(enemy["id"]): enemy for enemy in master["enemy"]}
    enemy_ai_ids = {
        int(enemies[enemy_id]["enemy_ai_id"])
        for enemy_id in enemy_ids
        if enemy_id in enemies
    }
    enemy_burst_ids = {
        int(enemies[enemy_id]["enemy_ai_burst_id"])
        for enemy_id in enemy_ids
        if enemy_id in enemies and enemies[enemy_id].get("enemy_ai_burst_id")
    }
    skill_ids.update(
        int(skill["id"])
        for unit in master["enemy_ai_unit"]
        if int(unit["enemy_ai_id"]) in enemy_ai_ids
        for skill in unit["skills"]
    )
    skill_ids.update(
        int(skill["id"])
        for unit in master["enemy_ai_burst_unit"]
        if int(unit["enemy_ai_burst_id"]) in enemy_burst_ids
        for skill in unit["skills"]
    )
    for enemy_id in enemy_ids:
        enemy = enemies.get(enemy_id)
        if enemy is None:
            continue
        skill_ids.update(int(value) for value in enemy["extra_skill_ids"])
        if enemy.get("burst_skill_id"):
            skill_ids.add(int(enemy["burst_skill_id"]))
    skills = {int(skill["id"]): skill for skill in master["skill"]}
    while True:
        destinations = {
            int(skills[skill_id]["skill_destination"])
            for skill_id in skill_ids
            if skill_id in skills and skills[skill_id].get("skill_destination")
        }
        before = len(skill_ids)
        skill_ids.update(destinations)
        if len(skill_ids) == before:
            break
    return {
        "skill": skill_ids,
        "ability": ability_ids,
        "battle_tool_trait": {int(row["id"]) for row in master["battle_tool_trait"]},
        "equipment_tool_trait": {
            int(row["id"]) for row in master["equipment_tool_trait"]
        },
    }


def occurrences(master: dict) -> list[tuple[str, int, int]]:
    result = []
    reachable = reachable_owner_ids(master)
    for owner_type, table in (
        ("skill", "skill"),
        ("ability", "ability"),
        ("battle_tool_trait", "battle_tool_trait"),
        ("equipment_tool_trait", "equipment_tool_trait"),
    ):
        for owner in master[table]:
            if int(owner["id"]) not in reachable[owner_type]:
                continue
            for effect in owner.get("effects") or []:
                result.append((owner_type, int(owner["id"]), int(effect["id"])))
    return result


def audit(master: dict, rules: dict, source_hash: str) -> tuple[list[tuple[str, int, int]], Counter]:
    require(
        rules.get("source_sha256", "").upper() == source_hash,
        "combat effect rules source hash is stale",
    )
    runtime_modes: dict[int, set[str]] = {}
    owner_modes: dict[tuple[str, int, int], set[str]] = {}
    runtime_keys = set()
    for rule in rules["rules"]:
        owner_type = rule.get("owner_type", "")
        owner_id = int(rule.get("owner_id", 0))
        key = (int(rule["id"]), rule["mode"], owner_type, owner_id)
        require(key not in runtime_keys, "duplicate runtime effect rule")
        runtime_keys.add(key)
        if owner_type:
            owner_modes.setdefault((owner_type, owner_id, key[0]), set()).add(key[1])
        else:
            runtime_modes.setdefault(key[0], set()).add(key[1])
    nested = {
        (rule["owner_type"], int(rule["owner_id"]), int(rule.get("effect_id", 0)))
        for rule in rules.get("nested_actions", [])
        if int(rule.get("effect_id", 0)) > 0
    }
    lamp_mechanics = {
        ("skill", int(skill_id), int(effect_id))
        for skill_id, rule in rules.get("lamp_skills", {}).items()
        for effect_id in rule["mechanic_effect_ids"] + rule.get("consumed_effect_ids", [])
    }
    lamp_mechanics.update(
        ("ability", int(ability_id), int(effect_id))
        for ability_id, rule in rules.get("lamp_abilities", {}).items()
        for effect_id in rule["mechanic_effect_ids"]
    )
    catalog_mechanics = set()
    for ability in master["ability"]:
        maximum = burst_gauge_max(ability)
        if maximum is None:
            continue
        effect_ids = [
            int(effect["id"])
            for effect in ability["effects"]
            if int(effect["value"]) == maximum * 100
        ]
        require(len(effect_ids) == 1, f"ambiguous burst capacity ability {ability['id']}")
        catalog_mechanics.add(("ability", int(ability["id"]), effect_ids[0]))
    missing = []
    for occurrence in occurrences(master):
        owner_type, owner_id, effect_id = occurrence
        modes = runtime_modes.get(effect_id, set()) | owner_modes.get(occurrence, set())
        expected = {"passive"} if owner_type == "ability" else {"active", "instant"}
        executable = bool(modes & expected)
        if (
            not executable
            and occurrence not in nested
            and occurrence not in lamp_mechanics
            and occurrence not in catalog_mechanics
        ):
            missing.append(occurrence)
    return missing, Counter(owner_type for owner_type, _, _ in missing)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--limit", type=int, default=20)
    args = parser.parse_args()
    source = WORKSPACE / "data" / "masterdata.jp.decoded"
    rules_path = WORKSPACE / "data" / "combat_effect_rules.json"
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest().upper()
    master = decode_master(
        source,
        load_probe(WORKSPACE / "tools" / "protocol" / "asset_closure_probe.py"),
    )
    rules = json.loads(rules_path.read_text(encoding="utf-8"))
    missing, by_owner = audit(master, rules, source_hash)
    if missing:
        descriptions = {int(row["id"]): row.get("description", "") for row in master["effect"]}
        owners = {effect_id: (owner_type, owner_id) for owner_type, owner_id, effect_id in missing}
        details = " ".join(
            f"{effect_id}x{count}@{owners[effect_id][0]}:{owners[effect_id][1]}"
            f"[{descriptions.get(effect_id, '')}]"
            for effect_id, count in Counter(effect_id for _, _, effect_id in missing).most_common(
                max(args.limit, 0)
            )
        )
        message = (
            f"unknown={len(missing)} "
            + " ".join(f"{kind}={count}" for kind, count in sorted(by_owner.items()))
            + (f" top={details}" if details else "")
        )
        if args.check:
            raise AuditError(message)
        print(f"COMBAT_EFFECT_COVERAGE_INCOMPLETE {message}")
        return
    print(f"COMBAT_EFFECT_COVERAGE_OK occurrences={len(occurrences(master))}")


if __name__ == "__main__":
    try:
        main()
    except (AuditError, KeyError, OSError, TypeError, ValueError) as error:
        raise SystemExit(f"COMBAT_EFFECT_COVERAGE_FAILED {error}") from error
