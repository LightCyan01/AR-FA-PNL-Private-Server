"""Audit story mission mappings against the decoded client master data.

Numbered labels normally bind by the quest's scene field. Retained labels with
a removed or duplicated scene bind by current chapter order; labels past the
current chapter tail bind to its final quest. Shared condition IDs must expose
the complete set as a one-of projection rather than a cumulative counter.

Run ``py tools/story_mission_mapping_audit.py --check`` after master-data or
home-rule generation changes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import unicodedata
from collections import Counter, defaultdict
from pathlib import Path


REPO = Path(__file__).resolve().parents[1]
WORKSPACE = REPO.parent
sys.path.insert(0, str(WORKSPACE / "tools" / "build"))

from build_fresh_state_rules import decode_master, load_probe  # noqa: E402


class AuditError(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AuditError(message)


def compact(value: str) -> str:
    return re.sub(r"\s+", "", unicodedata.normalize("NFKC", value))


def story_label(name: str) -> tuple[str, int, int | None] | None:
    normalized = compact(name)
    numbered = re.fullmatch(r"メインストーリー(\d+)-(\d+)をクリアしよう", normalized)
    if numbered:
        return "numbered", int(numbered[1]), int(numbered[2])
    milestone = re.fullmatch(
        r"メインストーリー(?:(\d+)章|第一部\(11章まで\))をクリアしよう",
        normalized,
    )
    if milestone:
        return "milestone", int(milestone[1] or 11), None
    return None


def mapped_quest_ids(objective: dict) -> set[int]:
    counter = objective.get("counter")
    if isinstance(counter, str) and counter.startswith("quest_clear:"):
        return {int(counter.partition(":")[2])}
    state = objective.get("state") or {}
    if state.get("kind") == "quest_clear_any":
        return set(state.get("quest_ids", []))
    return {
        int(counter.partition(":")[2])
        for counter in objective.get("counters", [])
        if counter.startswith("quest_clear:")
    }


def expected_quest(
    quests: list[dict], kind: str, scene: int | None
) -> tuple[dict, str]:
    require(bool(quests), "story chapter has no current quests")
    if kind == "milestone":
        return quests[-1], "chapter_end"
    require(scene is not None and scene > 0, "numbered story label has an invalid scene")
    exact = [quest for quest in quests if quest.get("scene") == scene]
    if len(exact) == 1:
        return exact[0], "exact_scene"
    if scene <= len(quests):
        return quests[scene - 1], "catalog_position_alias"
    return quests[-1], "retired_tail_alias"


def audit(master: dict, rules: dict, source_hash: str) -> tuple[Counter, int, int]:
    require(rules.get("source_sha256", "").upper() == source_hash, "home rules source hash is stale")
    generated = {row["id"]: row for row in rules["missions"]}
    require(len(generated) == len(rules["missions"]), "duplicate generated mission id")
    tasks = {row["condition_id"]: row for row in rules["total_tasks"]}
    require(len(tasks) == len(rules["total_tasks"]), "duplicate generated task condition")

    quests_by_day: dict[int, list[dict]] = defaultdict(list)
    quest_by_id = {}
    for quest in master["quest"]:
        quest_by_id[quest["id"]] = quest
        if quest.get("episode_type") == 1 and isinstance(quest.get("day"), int):
            quests_by_day[quest["day"]].append(quest)
    for quests in quests_by_day.values():
        quests.sort(key=lambda quest: (quest["priority"], quest["id"]))

    classifications = Counter()
    expected_by_condition: dict[int, set[int]] = defaultdict(set)
    chapters_by_condition: dict[int, set[int]] = defaultdict(set)
    story_rows = []
    for mission in master["mission"]:
        parsed = story_label(mission["name"])
        if parsed is None:
            continue
        kind, chapter, scene = parsed
        condition_id = mission.get("total_task_condition_id")
        require(isinstance(condition_id, int) and condition_id > 0, f"story mission {mission['id']} has no condition")
        quest, classification = expected_quest(quests_by_day[chapter], kind, scene)
        require(quest.get("day") == chapter, f"story mission {mission['id']} crossed chapters")
        output = generated.get(mission["id"])
        require(output is not None, f"story mission {mission['id']} is missing from home rules")
        require(output.get("name") == mission["name"], f"story mission {mission['id']} name drifted")
        require(
            mapped_quest_ids(output) == {quest["id"]},
            f"story mission {mission['id']} maps to the wrong quest",
        )
        classifications[kind] += 1
        classifications[classification] += 1
        expected_by_condition[condition_id].add(quest["id"])
        chapters_by_condition[condition_id].add(chapter)
        story_rows.append(mission)

    require(classifications["numbered"] > 0, "no numbered story missions were audited")
    require(classifications["milestone"] > 0, "no chapter or part milestones were audited")
    for condition_id, expected in expected_by_condition.items():
        require(
            len(chapters_by_condition[condition_id]) == 1,
            f"condition {condition_id} spans story chapters",
        )
        task = tasks.get(condition_id)
        require(task is not None, f"condition {condition_id} has no generated objective")
        require(
            mapped_quest_ids(task) == expected,
            f"condition {condition_id} does not preserve its story aliases",
        )
        if len(expected) > 1:
            require(
                (task.get("state") or {}).get("kind") == "quest_clear_any",
                f"condition {condition_id} aliases are cumulative",
            )
        for quest_id in expected:
            require(quest_id in quest_by_id, f"condition {condition_id} maps to missing quest {quest_id}")

    return classifications, len(story_rows), sum(len(ids) > 1 for ids in expected_by_condition.values())


def negative_control(master: dict, rules: dict, source_hash: str) -> None:
    mission_index = next(
        index for index, mission in enumerate(master["mission"]) if story_label(mission["name"])
    )
    mission_id = master["mission"][mission_index]["id"]
    generated_index = next(index for index, row in enumerate(rules["missions"]) if row["id"] == mission_id)
    _, chapter, _ = story_label(master["mission"][mission_index]["name"])
    foreign = next(
        quest["id"]
        for quest in master["quest"]
        if quest.get("episode_type") == 1 and quest.get("day") != chapter
    )
    tampered = {**rules, "missions": list(rules["missions"])}
    tampered["missions"][generated_index] = {
        **tampered["missions"][generated_index],
        "counter": f"quest_clear:{foreign}",
    }
    try:
        audit(master, tampered, source_hash)
    except AuditError:
        return
    raise AuditError("negative control accepted a cross-chapter mapping")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.parse_args()
    source = WORKSPACE / "data" / "masterdata.jp.decoded"
    rules_path = WORKSPACE / "data" / "home_rules.json"
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest().upper()
    master = decode_master(
        source,
        load_probe(WORKSPACE / "tools" / "protocol" / "asset_closure_probe.py"),
    )
    rules = json.loads(rules_path.read_text(encoding="utf-8"))
    classifications, mission_count, shared_aliases = audit(master, rules, source_hash)
    negative_control(master, rules, source_hash)
    print(
        "STORY_MISSION_MAPPING_AUDIT_OK "
        f"missions={mission_count} numbered={classifications['numbered']} "
        f"milestones={classifications['milestone']} exact={classifications['exact_scene']} "
        f"position_aliases={classifications['catalog_position_alias']} "
        f"tail_aliases={classifications['retired_tail_alias']} "
        f"shared_alias_conditions={shared_aliases} negative_control=passed"
    )


if __name__ == "__main__":
    try:
        main()
    except (AuditError, KeyError, OSError, ValueError) as error:
        raise SystemExit(f"STORY_MISSION_MAPPING_AUDIT_FAILED {error}") from error
