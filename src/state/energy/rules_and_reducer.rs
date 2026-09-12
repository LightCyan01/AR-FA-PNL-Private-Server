use crate::state::service::prelude::*;

#[derive(Clone, Deserialize)]
pub(crate) struct EnergyRules {
    pub mana_recovery_limit: i32,
    pub mana_recovery_interval_seconds: i64,
    pub max_mana: i32,
    pub(crate) mana_per_purchase: i32,
    pub mana_purchase_cost: i32,
    pub stamina_recovery_interval_seconds: i64,
    pub max_stamina: i32,
    pub(crate) stamina_per_purchase: i32,
    pub(crate) max_spare_stamina: i32,
    pub(crate) spare_stamina_recovery_interval_seconds: i32,
    pub(crate) stamina_purchases: Vec<StaminaPurchase>,
    pub(crate) stamina_passes: Vec<StaminaPass>,
    pub(crate) recovery_items: Vec<RecoveryItem>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct StaminaPurchase {
    #[serde(rename = "gem_cost")]
    pub(crate) cost: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct StaminaPass {
    pub(crate) id: i32,
    pub(crate) daily_free_count: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct RecoveryItem {
    pub(crate) id: i32,
    pub(crate) item_type: i32,
    pub(crate) value: i32,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
}

pub(crate) fn refresh(
    proto: &ProtoRegistry,
    rules: &EnergyRules,
    status: &mut DynamicMessage,
    stamina_limit: i32,
    now: i64,
) -> Result<(), StateError> {
    let updated = message_i64_field(status, "stamina_updated_at", "seconds").unwrap_or(now);
    let stamina = i32_field(status, "stamina_when_updated").unwrap_or(0);
    let until_full =
        i64::from((stamina_limit - stamina).max(0)) * rules.stamina_recovery_interval_seconds;
    let spare = i64::from(i32_field(status, "spare_stamina_acc_seconds").unwrap_or(0))
        + (now.saturating_sub(updated).saturating_sub(until_full)).max(0);
    let spare_cap = i64::from(rules.max_spare_stamina)
        * i64::from(rules.spare_stamina_recovery_interval_seconds);
    status.set_field_by_name(
        "spare_stamina_acc_seconds",
        Value::I32(spare.min(spare_cap) as i32),
    );
    if home::day(updated) < home::day(now) {
        status.set_field_by_name("stamina_purchased_count", Value::I32(0));
    }
    recover_resource(
        proto,
        status,
        "stamina",
        stamina_limit,
        rules.stamina_recovery_interval_seconds,
        now,
    )?;
    recover_resource(
        proto,
        status,
        "mana",
        rules.mana_recovery_limit,
        rules.mana_recovery_interval_seconds,
        now,
    )
}

pub(crate) fn apply(
    proto: &ProtoRegistry,
    rules: &EnergyRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let count = i32_field(request, "count")
        .filter(|count| *count > 0)
        .ok_or(StateError::InvalidRequest)?;
    let mana = route.starts_with("/mana/");
    let kind = if mana { "mana" } else { "stamina" };
    let limit = if mana {
        rules.max_mana
    } else {
        rules.max_stamina
    };
    let mut status = status_message(resources)?;
    let mut offers = message_list(resources, "special_offer_states");
    let mut changed_offers = Vec::new();
    let (amount, cost) = if route.ends_with("/purchase") {
        let (per_purchase, cost) = if mana {
            (
                rules.mana_per_purchase,
                count
                    .checked_mul(rules.mana_purchase_cost)
                    .ok_or(StateError::InvalidRequest)?,
            )
        } else {
            let mut base_count = count;
            offers.sort_by_key(|offer| i32_field(offer, "special_offer_id"));
            for offer in &mut offers {
                if base_count == 0
                    || message_i64_field(offer, "expires_at", "seconds")
                        .is_none_or(|end| now >= end)
                {
                    continue;
                }
                let pass = rules
                    .stamina_passes
                    .iter()
                    .find(|pass| Some(pass.id) == i32_field(offer, "special_offer_id"))
                    .ok_or(StateError::InvalidRequest)?;
                let last = message_i64_field(offer, "last_free_stamina_recovered_at", "seconds");
                let used = if last.is_some_and(|last| home::day(last) < home::day(now)) {
                    0
                } else {
                    i32_field(offer, "free_stamina_recovered_count").unwrap_or(0)
                };
                let take = base_count.min((pass.daily_free_count - used).max(0));
                if take > 0 {
                    offer
                        .set_field_by_name("free_stamina_recovered_count", Value::I32(used + take));
                    offer.set_field_by_name(
                        "last_free_stamina_recovered_at",
                        Value::Message(timestamp(proto, now)?),
                    );
                    changed_offers.push(Value::Message(offer.clone()));
                    base_count -= take;
                }
            }
            let purchased = i32_field(&status, "stamina_purchased_count").unwrap_or(0);
            let next = purchased
                .checked_add(base_count)
                .ok_or(StateError::InvalidRequest)?;
            let prices = rules
                .stamina_purchases
                .get(purchased as usize..next as usize)
                .ok_or(StateError::InvalidRequest)?;
            let cost = prices
                .iter()
                .try_fold(0i32, |sum, row| sum.checked_add(row.cost))
                .ok_or(StateError::InvalidRequest)?;
            status.set_field_by_name("stamina_purchased_count", Value::I32(next));
            (rules.stamina_per_purchase, cost)
        };
        (
            count
                .checked_mul(per_purchase)
                .ok_or(StateError::InvalidRequest)?,
            Some(TutorialRecipeCost {
                resource_type: 1,
                id: 1,
                quantity: cost,
            }),
        )
    } else if route.ends_with("/use_item") {
        let id = i32_field(request, "item_id").ok_or(StateError::InvalidRequest)?;
        let item = rules
            .recovery_items
            .iter()
            .find(|item| {
                item.id == id
                    && item.item_type == if mana { 12 } else { 11 }
                    && item.start_at.is_none_or(|start| start <= now)
                    && item.end_at.is_none_or(|end| now < end)
            })
            .ok_or(StateError::InvalidRequest)?;
        (
            count
                .checked_mul(item.value)
                .filter(|amount| *amount > 0)
                .ok_or(StateError::InvalidRequest)?,
            Some(TutorialRecipeCost {
                resource_type: 5,
                id,
                quantity: count,
            }),
        )
    } else if route == "/stamina/use_spare_stamina" {
        let seconds = count
            .checked_mul(rules.spare_stamina_recovery_interval_seconds)
            .ok_or(StateError::InvalidRequest)?;
        let spare = i32_field(&status, "spare_stamina_acc_seconds").unwrap_or(0);
        status.set_field_by_name(
            "spare_stamina_acc_seconds",
            Value::I32(
                spare
                    .checked_sub(seconds)
                    .filter(|value| *value >= 0)
                    .ok_or(StateError::InvalidRequest)?,
            ),
        );
        (count, None)
    } else {
        return Err(StateError::InvalidRequest);
    };
    let field = format!("{kind}_when_updated");
    let old = i32_field(&status, &field).unwrap_or(0);
    let next = old
        .checked_add(amount)
        .filter(|next| *next <= limit)
        .ok_or(StateError::InvalidRequest)?;
    if let Some((field, value)) = pay_resource_cost(proto, resources, cost.as_ref())? {
        if field == "items" {
            changed.set_field_by_name(field, Value::List(vec![Value::Message(value)]));
        } else {
            changed.set_field_by_name(field, Value::Message(value));
        }
    }
    status.set_field_by_name(&field, Value::I32(next));
    status.set_field_by_name(
        &format!("{kind}_updated_at"),
        Value::Message(timestamp(proto, now)?),
    );
    resources.set_field_by_name("status", Value::Message(status.clone()));
    changed.set_field_by_name("status", Value::Message(status));
    if !changed_offers.is_empty() {
        resources.set_field_by_name(
            "special_offer_states",
            Value::List(offers.into_iter().map(Value::Message).collect()),
        );
        changed.set_field_by_name("special_offer_states", Value::List(changed_offers));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::service::prelude::{change_item, load_fresh_rules};

    #[test]
    fn energy_recovery_purchases_and_item_caps() {
        let proto = ProtoRegistry::from_file(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let rules = home::load_rules().unwrap().energy;
        let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
        let mut status = status_message(&resources).unwrap();
        let now = 1_800_000_000;
        status.set_field_by_name("stamina_when_updated", Value::I32(159));
        status.set_field_by_name(
            "stamina_updated_at",
            Value::Message(timestamp(&proto, now - 1_300).unwrap()),
        );
        status.set_field_by_name("mana_when_updated", Value::I32(18));
        status.set_field_by_name(
            "mana_updated_at",
            Value::Message(timestamp(&proto, now - 3_650).unwrap()),
        );
        refresh(&proto, &rules, &mut status, 160, now).unwrap();
        assert_eq!(i32_field(&status, "stamina_when_updated"), Some(160));
        assert_eq!(i32_field(&status, "spare_stamina_acc_seconds"), Some(1_000));
        assert_eq!(i32_field(&status, "mana_when_updated"), Some(19));
        refresh(&proto, &rules, &mut status, 160, now).unwrap();
        assert_eq!(i32_field(&status, "spare_stamina_acc_seconds"), Some(1_000));
        resources.set_field_by_name("status", Value::Message(status));
        let mut wallet = empty_message(&proto, "blend.model.Wallet").unwrap();
        wallet.set_field_by_name("free", Value::I32(1000));
        resources.set_field_by_name("wallet", Value::Message(wallet));
        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        let mut request = empty_message(&proto, "blend.api.StaminaPurchaseRequest").unwrap();
        request.set_field_by_name("count", Value::I32(3));
        apply(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            "/stamina/purchase",
            &request,
            now,
        )
        .unwrap();
        assert_eq!(
            i32_field(&status_message(&resources).unwrap(), "stamina_when_updated"),
            Some(400)
        );
        assert_eq!(message_i32_field(&resources, "wallet", "free"), Some(920));
        let mut spare = empty_message(&proto, "blend.api.StaminaUseSpareStaminaRequest").unwrap();
        spare.set_field_by_name("count", Value::I32(1));
        apply(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            "/stamina/use_spare_stamina",
            &spare,
            now,
        )
        .unwrap();
        assert_eq!(
            i32_field(
                &status_message(&resources).unwrap(),
                "spare_stamina_acc_seconds"
            ),
            Some(100)
        );
        change_item(&proto, &mut resources, 688, 2).unwrap();
        let mut item = empty_message(&proto, "blend.api.ManaUseItemRequest").unwrap();
        item.set_field_by_name("item_id", Value::I32(688));
        item.set_field_by_name("count", Value::I32(2));
        apply(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            "/mana/use_item",
            &item,
            now,
        )
        .unwrap();
        assert_eq!(
            i32_field(&status_message(&resources).unwrap(), "mana_when_updated"),
            Some(21)
        );
        let mut mana = empty_message(&proto, "blend.api.ManaPurchaseRequest").unwrap();
        mana.set_field_by_name("count", Value::I32(2));
        apply(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            "/mana/purchase",
            &mana,
            now,
        )
        .unwrap();
        assert_eq!(message_i32_field(&resources, "wallet", "free"), Some(900));
        let before = resources.clone();
        mana.set_field_by_name("count", Value::I32(1000));
        assert!(apply(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            "/mana/purchase",
            &mana,
            now
        )
        .is_err());
        assert_eq!(resources, before);
        request.set_field_by_name("count", Value::I32(14));
        assert!(apply(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            "/stamina/purchase",
            &request,
            now
        )
        .is_err());
        let mut status = status_message(&resources).unwrap();
        refresh(&proto, &rules, &mut status, 160, now + 86_400).unwrap();
        assert_eq!(i32_field(&status, "mana_when_updated"), Some(23));
        assert_eq!(i32_field(&status, "stamina_purchased_count"), Some(0));
        resources.set_field_by_name("status", Value::Message(status));
        let mut pass = empty_message(&proto, "blend.model.SpecialOfferState").unwrap();
        pass.set_field_by_name("special_offer_id", Value::I32(1));
        pass.set_field_by_name(
            "expires_at",
            Value::Message(timestamp(&proto, now + 172_800).unwrap()),
        );
        resources.set_field_by_name(
            "special_offer_states",
            Value::List(vec![Value::Message(pass)]),
        );
        request.set_field_by_name("count", Value::I32(3));
        apply(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            "/stamina/purchase",
            &request,
            now + 86_400,
        )
        .unwrap();
        assert_eq!(message_i32_field(&resources, "wallet", "free"), Some(860));
        assert_eq!(
            message_i32_field(&resources, "status", "stamina_purchased_count"),
            Some(2)
        );
        assert_eq!(
            i32_field(
                &message_list(&resources, "special_offer_states")[0],
                "free_stamina_recovered_count"
            ),
            Some(1)
        );
    }
}
