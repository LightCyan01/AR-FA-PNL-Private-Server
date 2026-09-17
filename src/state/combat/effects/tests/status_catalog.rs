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
