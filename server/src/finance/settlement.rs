use std::collections::HashSet;
use std::sync::Arc;

use super::types::{
    FinanceRuleError, RunId, ScheduledSettlement, SettlementRules, SettlementStatus,
};

struct DefaultSettlementRules;

pub fn create_settlement_rules() -> Arc<dyn SettlementRules> {
    Arc::new(DefaultSettlementRules)
}

impl SettlementRules for DefaultSettlementRules {
    fn due_settlements(
        &self,
        run: RunId,
        game_day: u32,
        settlements: Vec<ScheduledSettlement>,
    ) -> Result<Vec<ScheduledSettlement>, FinanceRuleError> {
        let mut ids = HashSet::new();
        let mut sources = HashSet::new();
        let mut due = Vec::new();

        for settlement in settlements
            .into_iter()
            .filter(|settlement| settlement.run == run)
        {
            if settlement.source.source_id.is_empty()
                || !ids.insert(settlement.id)
                || !sources.insert((
                    settlement.source.kind,
                    settlement.source.source_id.clone(),
                    settlement.source.occurrence,
                ))
            {
                return Err(FinanceRuleError::SettlementConflict);
            }

            if settlement.status == SettlementStatus::Pending && settlement.due_game_day <= game_day
            {
                due.push(settlement);
            }
        }

        // Stable sorting keeps this order deterministic if future keys gain equal components.
        due.sort_by_key(|settlement| (settlement.due_game_day, settlement.id));
        Ok(due)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ResourceId, SettlementKind, SettlementSource, SettlementSourceKind};
    use super::*;

    const SAVE_ID: ResourceId = ResourceId::from_u64(11);

    fn given_run(run_revision: u32) -> RunId {
        RunId {
            save_id: SAVE_ID,
            run_revision,
        }
    }

    fn given_settlement(
        id: u64,
        due_game_day: u32,
        source_id: &str,
        occurrence: u32,
    ) -> ScheduledSettlement {
        ScheduledSettlement {
            id: ResourceId::from_u64(id),
            run: given_run(4),
            due_game_day,
            kind: SettlementKind::DepositMaturity,
            source: SettlementSource {
                kind: SettlementSourceKind::DepositContract,
                source_id: source_id.to_owned(),
                occurrence,
            },
            status: SettlementStatus::Pending,
            payload: serde_json::json!({"contractId": source_id}),
        }
    }

    mod context_due_settlements_are_selected {
        use super::*;

        #[test]
        fn given_unsorted_due_rows_when_selected_then_they_are_ordered_by_day_and_id() {
            let rules = create_settlement_rules();
            let settlements = vec![
                given_settlement(9, 5, "deposit-9", 0),
                given_settlement(7, 4, "deposit-7", 0),
                given_settlement(5, 5, "deposit-5", 0),
            ];

            let due = rules
                .due_settlements(given_run(4), 5, settlements)
                .expect("정산을 선택할 수 있어야 한다");

            let ids = due
                .iter()
                .map(|settlement| settlement.id.get())
                .collect::<Vec<_>>();
            assert_eq!(ids, vec![7, 5, 9]);
        }

        #[test]
        fn given_future_settled_and_other_run_rows_when_selected_then_only_pending_due_rows_remain()
        {
            let rules = create_settlement_rules();
            let mut settled = given_settlement(2, 4, "settled", 0);
            settled.status = SettlementStatus::Settled;
            let mut other_run = given_settlement(3, 4, "other-run", 0);
            other_run.run = given_run(3);
            let settlements = vec![
                given_settlement(1, 4, "due", 0),
                settled,
                other_run,
                given_settlement(4, 6, "future", 0),
            ];

            let due = rules
                .due_settlements(given_run(4), 5, settlements)
                .expect("정산을 선택할 수 있어야 한다");

            assert_eq!(due.len(), 1);
            assert_eq!(due[0].id.get(), 1);
        }

        #[test]
        fn given_duplicate_source_identity_when_selected_then_it_is_rejected() {
            let rules = create_settlement_rules();
            let settlements = vec![
                given_settlement(1, 4, "deposit-1", 2),
                given_settlement(2, 5, "deposit-1", 2),
            ];

            let result = rules.due_settlements(given_run(4), 5, settlements);

            assert_eq!(result, Err(FinanceRuleError::SettlementConflict));
        }

        #[test]
        fn given_the_same_source_with_different_occurrences_when_selected_then_both_are_allowed() {
            let rules = create_settlement_rules();
            let settlements = vec![
                given_settlement(1, 4, "savings-1", 1),
                given_settlement(2, 5, "savings-1", 2),
            ];

            let due = rules
                .due_settlements(given_run(4), 5, settlements)
                .expect("회차가 다르면 정산할 수 있어야 한다");

            assert_eq!(due.len(), 2);
        }
    }
}
