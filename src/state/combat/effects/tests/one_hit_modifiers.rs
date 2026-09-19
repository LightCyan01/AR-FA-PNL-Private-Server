use super::super::*;

#[test]
fn battle_tool_one_hit_modifiers_keep_damage_type_and_lifetime() {
    for (effect_id, operation, attribute, sign, state_id, positive) in [
        (3000047, "physical_taken", 1, 1, 920004, false),
        (3000049, "magic_taken", 5, 1, 920003, false),
        (3000059, "attribute_taken", 5, 1, 50006, false),
        (3000060, "attribute_taken", 6, 1, 50007, false),
        (3000061, "attribute_taken", 7, 1, 50008, false),
        (3000062, "attribute_taken", 8, 1, 50009, false),
        (3000063, "attribute_taken", 1, 1, 50010, false),
        (3000064, "attribute_taken", 2, 1, 50011, false),
        (3000065, "attribute_taken", 3, 1, 50012, false),
        (3000066, "attribute_taken", 5, -1, 910026, true),
        (3000067, "attribute_taken", 6, -1, 910025, true),
        (3000068, "attribute_taken", 7, -1, 910024, true),
        (3000069, "attribute_taken", 8, -1, 910023, true),
        (3000070, "attribute_taken", 1, -1, 910022, true),
        (3000071, "attribute_taken", 2, -1, 910021, true),
        (3000072, "attribute_taken", 3, -1, 910020, true),
    ] {
        let rule = rule_for(effect_id, "active", "", 0).unwrap().unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                rule.phase.as_str(),
                rule.sign,
                rule.state_id,
                &rule.expiry,
                rule.duration,
                rule.positive,
            ),
            (
                operation,
                "targets",
                "after",
                sign,
                state_id,
                &Expiry::Hit,
                1,
                positive,
            )
        );
        if operation == "attribute_taken" {
            assert_eq!(rule.attack_attributes, [attribute]);
        }
    }
}
