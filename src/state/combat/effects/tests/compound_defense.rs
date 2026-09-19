use super::super::*;

#[test]
fn compound_defense_rows_keep_both_stats_and_owner_context() {
    for (effect_id, operation) in [(91001396, "defense"), (91001397, "mental")] {
        let rule = rule_for(effect_id, "active", "skill", 11001889)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                &rule.expiry,
                rule.duration,
                rule.stack_cap,
            ),
            (operation, "self", &Expiry::Permanent, -1, 3_000)
        );
    }

    for (effect_id, operation) in [(91001581, "defense"), (91001582, "mental")] {
        let self_rule = rule_for(effect_id, "active", "skill", 12002279)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                self_rule.operation.as_str(),
                self_rule.target.as_str(),
                &self_rule.expiry,
                self_rule.duration,
            ),
            (operation, "self", &Expiry::Hit, 2)
        );

        let other_owner = rule_for(effect_id, "active", "skill", 12003576)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                other_owner.operation.as_str(),
                other_owner.target.as_str(),
                &other_owner.expiry,
                other_owner.duration,
            ),
            (operation, "self", &Expiry::Hit, 2)
        );
    }

    for (effect_id, operation) in [(91001577, "defense"), (91001578, "mental")] {
        let filtered = rule_for(effect_id, "active", "skill", 11002274)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                filtered.operation.as_str(),
                filtered.target.as_str(),
                &filtered.expiry,
                filtered.duration,
                filtered.stack_cap,
            ),
            (operation, "allies", &Expiry::Permanent, -1, 3_000)
        );
        assert!(!filtered.target_character_ids.is_empty());
    }
}
