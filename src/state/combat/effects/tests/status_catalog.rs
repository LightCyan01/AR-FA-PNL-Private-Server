use super::super::*;

#[test]
fn shared_status_rules_cover_textless_and_shorthand_skills() {
    for skill_id in [20000019, 20010790, 32000281, 32000285] {
        let rule = rule_for(780035009, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                rule.state_id,
                rule.duration,
            ),
            ("status", "targets", 940005, 3)
        );
    }
    let all_enemies = rule_for(780035009, "active", "skill", 20008975)
        .unwrap()
        .unwrap();
    assert_eq!(all_enemies.target, "enemies");

    for (effect_id, skill_id, state_id, duration) in [
        (780039007, 20000072, 940005, 3),
        (780080004, 20001295, 940007, 5),
    ] {
        let rule = rule_for(effect_id, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (rule.target.as_str(), rule.state_id, rule.duration),
            ("targets", state_id, duration)
        );
    }
}

#[test]
fn canonical_named_ability_states_use_status_rules() {
    for (effect_id, ability_id, state_id, positive) in [
        (120000553, 600000580, 1210318, true),
        (120000606, 600000617, 1210330, true),
        (120000622, 600000601, 1220035, false),
        (120000643, 24019001, 1210352, true),
        (120000643, 600000599, 1210352, true),
        (120000653, 600000631, 1210362, true),
        (120000661, 600000624, 1220059, false),
    ] {
        let rule = rule_for(effect_id, "passive", "ability", ability_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                rule.state_id,
                &rule.expiry,
                rule.duration,
                rule.positive,
            ),
            ("status", "self", state_id, &Expiry::Permanent, -1, positive,)
        );
    }
}

#[test]
fn compound_resistance_effects_keep_each_attribute() {
    for (effect_id, skill_id, target, attribute, state_id, duration) in [
        (91000971, 11000436, "targets", 5, 50006, 5),
        (91000936, 11000436, "targets", 6, 50007, 5),
        (4000003, 31553, "enemies", 5, 50006, 2),
        (4000004, 31553, "enemies", 6, 50007, 2),
        (4000005, 31553, "enemies", 7, 50008, 2),
        (4000006, 31553, "enemies", 8, 50009, 2),
    ] {
        let rule = rule_for(effect_id, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                rule.attack_attributes.as_slice(),
                rule.state_id,
                &rule.expiry,
                rule.duration,
            ),
            (
                "attribute_taken",
                target,
                [attribute].as_slice(),
                state_id,
                &Expiry::Attacked,
                duration,
            )
        );
    }
}

#[test]
fn all_attribute_resistance_up_covers_every_owner() {
    for skill_id in [
        22001593, 22002062, 22002369, 22002376, 32003960, 32004029, 32004843, 32005762,
    ] {
        let rule = rule_for(71225003, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                rule.sign,
                rule.attack_attributes.as_slice(),
                rule.state_id,
                &rule.expiry,
                rule.duration,
            ),
            (
                "attribute_taken",
                "self",
                -1,
                [1, 2, 3, 5, 6, 7, 8].as_slice(),
                910004,
                &Expiry::Attacked,
                2,
            )
        );
        assert!(rule.positive);
    }
}

#[test]
fn received_attribute_damage_uses_the_target_multiplier() {
    for (effect_id, attribute, state_id) in [(91001658, 5, 50006), (91001645, 2, 50011)] {
        let rule = rule_for(effect_id, "active", "skill", 12002406)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.attack_attributes.as_slice(),
                rule.state_id,
                rule.positive,
            ),
            ("attribute_taken", [attribute].as_slice(), state_id, false)
        );
    }
}
