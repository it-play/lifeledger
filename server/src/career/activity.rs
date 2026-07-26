use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::types::{
    ActivityCatalogEntry, ActivityDayInput, ActivityDayPlan, ActivityEffortAllocation,
    ActivityError, ActivityPlanner, ActivityStatus, MAX_ACTIVE_ACTIVITIES,
};

struct V1ActivityPlanner;

pub fn create_activity_planner() -> Arc<dyn ActivityPlanner> {
    Arc::new(V1ActivityPlanner)
}

impl ActivityPlanner for V1ActivityPlanner {
    fn plan_day(&self, input: ActivityDayInput<'_>) -> Result<ActivityDayPlan, ActivityError> {
        let catalog = validate_catalog(input.catalog)?;
        let active = validate_and_order_active_activities(&input, &catalog)?;
        let available_effort_units = input.capacities.for_status(input.life_status);
        let mut remaining_effort_units = available_effort_units;
        let mut allocations = Vec::with_capacity(active.len());
        let mut completed_activity_ids = Vec::new();

        for activity in active {
            let entry = catalog
                .get(activity.catalog_entry_key.as_str())
                .copied()
                .ok_or_else(|| {
                    ActivityError::UnknownCatalogEntry(activity.catalog_entry_key.clone())
                })?;
            let remaining_requirement = entry
                .required_effort_units
                .checked_sub(activity.accumulated_effort_units)
                .ok_or(ActivityError::EffortExceedsRequirement(
                    activity.activity_id,
                ))?;
            let status_allows_effort = entry.allowed_life_statuses.contains(&input.life_status);
            let allocated_effort_units = if status_allows_effort {
                remaining_effort_units
                    .min(entry.daily_effort_cap_units)
                    .min(remaining_requirement)
            } else {
                0
            };
            let accumulated_effort_units = activity
                .accumulated_effort_units
                .checked_add(allocated_effort_units)
                .ok_or(ActivityError::ArithmeticOverflow)?;
            remaining_effort_units = remaining_effort_units
                .checked_sub(allocated_effort_units)
                .ok_or(ActivityError::ArithmeticOverflow)?;

            let started_game_day = activity
                .started_game_day
                .ok_or(ActivityError::InvalidActiveDates(activity.activity_id))?;
            let elapsed_calendar_days = input
                .current_game_day
                .checked_sub(started_game_day)
                .and_then(|days| days.checked_add(1))
                .ok_or(ActivityError::ArithmeticOverflow)?;
            let completed = accumulated_effort_units >= entry.required_effort_units
                && elapsed_calendar_days >= entry.minimum_calendar_days;
            let status = if completed {
                ActivityStatus::Completed
            } else {
                ActivityStatus::Active
            };
            let completed_game_day = completed.then_some(input.current_game_day);
            if completed {
                completed_activity_ids.push(activity.activity_id);
            }
            allocations.push(ActivityEffortAllocation {
                activity_id: activity.activity_id,
                allocated_effort_units,
                accumulated_effort_units,
                elapsed_calendar_days,
                status,
                completed_game_day,
            });
        }

        Ok(ActivityDayPlan {
            available_effort_units,
            remaining_effort_units,
            allocations,
            completed_activity_ids,
        })
    }
}

fn validate_catalog(
    entries: &[ActivityCatalogEntry],
) -> Result<HashMap<&str, &ActivityCatalogEntry>, ActivityError> {
    let mut catalog = HashMap::with_capacity(entries.len());
    for entry in entries {
        if entry.catalog_entry_key.trim().is_empty()
            || entry.evidence_catalog_entry_key.trim().is_empty()
        {
            return Err(ActivityError::EmptyCatalogEntryKey);
        }
        if entry.minimum_calendar_days == 0
            || entry.required_effort_units == 0
            || entry.daily_effort_cap_units == 0
            || entry.cost_krw < 0
            || entry.allowed_life_statuses.is_empty()
        {
            return Err(ActivityError::InvalidCatalogEntry(
                entry.catalog_entry_key.clone(),
            ));
        }
        let mut statuses = HashSet::with_capacity(entry.allowed_life_statuses.len());
        for status in &entry.allowed_life_statuses {
            if !statuses.insert(*status) {
                return Err(ActivityError::DuplicateAllowedLifeStatus(
                    entry.catalog_entry_key.clone(),
                ));
            }
        }
        if catalog
            .insert(entry.catalog_entry_key.as_str(), entry)
            .is_some()
        {
            return Err(ActivityError::DuplicateCatalogEntryKey(
                entry.catalog_entry_key.clone(),
            ));
        }
    }

    Ok(catalog)
}

fn validate_and_order_active_activities<'a>(
    input: &'a ActivityDayInput<'a>,
    catalog: &HashMap<&str, &ActivityCatalogEntry>,
) -> Result<Vec<&'a super::types::SpecActivity>, ActivityError> {
    let mut activity_ids = HashSet::with_capacity(input.activities.len());
    for activity in input.activities {
        if !activity_ids.insert(activity.activity_id) {
            return Err(ActivityError::DuplicateActivityId(activity.activity_id));
        }
    }

    let mut active = input
        .activities
        .iter()
        .filter(|activity| activity.status == ActivityStatus::Active)
        .collect::<Vec<_>>();
    if active.len() > MAX_ACTIVE_ACTIVITIES {
        return Err(ActivityError::TooManyActiveActivities);
    }

    let mut priorities = HashSet::with_capacity(active.len());
    for activity in &active {
        let Some(priority) = activity.priority else {
            return Err(ActivityError::InvalidActivePriority(activity.activity_id));
        };
        if !(1..=3).contains(&priority) {
            return Err(ActivityError::InvalidActivePriority(activity.activity_id));
        }
        if !priorities.insert(priority) {
            return Err(ActivityError::DuplicateActivePriority(priority));
        }
        let Some(started_game_day) = activity.started_game_day else {
            return Err(ActivityError::InvalidActiveDates(activity.activity_id));
        };
        if started_game_day > input.current_game_day || activity.completed_game_day.is_some() {
            return Err(ActivityError::InvalidActiveDates(activity.activity_id));
        }
        let entry = catalog
            .get(activity.catalog_entry_key.as_str())
            .copied()
            .ok_or_else(|| {
                ActivityError::UnknownCatalogEntry(activity.catalog_entry_key.clone())
            })?;
        if activity.accumulated_effort_units > entry.required_effort_units {
            return Err(ActivityError::EffortExceedsRequirement(
                activity.activity_id,
            ));
        }
    }
    active.sort_by_key(|activity| (activity.priority.unwrap_or(u8::MAX), activity.activity_id));

    Ok(active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::types::{
        ActivityStatus, LifeStatus, LifeStatusEffortCapacities, SpecActivity,
    };

    fn given_catalog_entry(
        key: &str,
        minimum_calendar_days: u32,
        required_effort_units: u64,
        daily_effort_cap_units: u64,
        allowed_life_statuses: Vec<LifeStatus>,
    ) -> ActivityCatalogEntry {
        ActivityCatalogEntry {
            catalog_entry_key: key.to_owned(),
            minimum_calendar_days,
            required_effort_units,
            daily_effort_cap_units,
            allowed_life_statuses,
            cost_krw: 10_000,
            evidence_catalog_entry_key: format!("{key}-evidence"),
        }
    }

    fn given_active_activity(
        activity_id: u64,
        key: &str,
        priority: u8,
        started_game_day: u32,
        accumulated_effort_units: u64,
    ) -> SpecActivity {
        SpecActivity {
            activity_id,
            catalog_entry_key: key.to_owned(),
            status: ActivityStatus::Active,
            priority: Some(priority),
            started_game_day: Some(started_game_day),
            accumulated_effort_units,
            completed_game_day: None,
        }
    }

    fn given_capacities() -> LifeStatusEffortCapacities {
        LifeStatusEffortCapacities {
            unemployed: 10,
            employed: 6,
            active_duty: 2,
            social_service: 5,
            special_service: 4,
            officer_or_nco: 3,
        }
    }

    mod context_우선순위대로_하루_effort를_나누는_경우 {
        use super::*;

        #[test]
        fn given_세_활동과_개별_daily_cap_when_계획하면_then_낮은_우선순위부터_상한만큼_배분한다() {
            let catalog = vec![
                given_catalog_entry("first", 1, 20, 4, vec![LifeStatus::Unemployed]),
                given_catalog_entry("second", 1, 20, 5, vec![LifeStatus::Unemployed]),
                given_catalog_entry("third", 1, 20, 7, vec![LifeStatus::Unemployed]),
            ];
            let activities = vec![
                given_active_activity(30, "third", 3, 0, 0),
                given_active_activity(10, "first", 1, 0, 0),
                given_active_activity(20, "second", 2, 0, 0),
            ];

            let result = create_activity_planner()
                .plan_day(ActivityDayInput {
                    current_game_day: 0,
                    life_status: LifeStatus::Unemployed,
                    capacities: given_capacities(),
                    catalog: &catalog,
                    activities: &activities,
                })
                .expect("하루 활동을 계획해야 한다");

            assert_eq!(
                result
                    .allocations
                    .iter()
                    .map(|allocation| (allocation.activity_id, allocation.allocated_effort_units))
                    .collect::<Vec<_>>(),
                vec![(10, 4), (20, 5), (30, 1)]
            );
            assert_eq!(result.remaining_effort_units, 0);
        }

        #[test]
        fn given_여섯_생활상태별_capacity_when_계획하면_then_해당_상태의_가용량을_쓴다() {
            let cases = [
                (LifeStatus::Unemployed, 10),
                (LifeStatus::Employed, 6),
                (LifeStatus::ActiveDuty, 2),
                (LifeStatus::SocialService, 5),
                (LifeStatus::SpecialService, 4),
                (LifeStatus::OfficerOrNco, 3),
            ];
            let catalog = vec![given_catalog_entry(
                "all-status",
                1,
                100,
                100,
                cases.iter().map(|(status, _)| *status).collect(),
            )];
            let activities = vec![given_active_activity(1, "all-status", 1, 0, 0)];

            for (status, expected) in cases {
                let result = create_activity_planner()
                    .plan_day(ActivityDayInput {
                        current_game_day: 0,
                        life_status: status,
                        capacities: given_capacities(),
                        catalog: &catalog,
                        activities: &activities,
                    })
                    .expect("생활상태별 활동을 계획해야 한다");

                assert_eq!(result.available_effort_units, expected);
                assert_eq!(result.allocations[0].allocated_effort_units, expected);
            }
        }

        #[test]
        fn given_허용되지_않은_생활상태_when_계획하면_then_effort를_배분하지_않는다() {
            let catalog = vec![given_catalog_entry(
                "unemployed-only",
                1,
                10,
                10,
                vec![LifeStatus::Unemployed],
            )];
            let activities = vec![given_active_activity(1, "unemployed-only", 1, 0, 0)];

            let result = create_activity_planner()
                .plan_day(ActivityDayInput {
                    current_game_day: 0,
                    life_status: LifeStatus::Employed,
                    capacities: given_capacities(),
                    catalog: &catalog,
                    activities: &activities,
                })
                .expect("허용 상태를 판정해야 한다");

            assert_eq!(result.allocations[0].allocated_effort_units, 0);
            assert_eq!(result.remaining_effort_units, 6);
        }
    }

    mod context_완료조건을_판정하는_경우 {
        use super::*;

        #[test]
        fn given_effort는_찼지만_최소일이_남았을때_when_계획하면_then_active를_유지한다() {
            let catalog = vec![given_catalog_entry(
                "course",
                3,
                5,
                5,
                vec![LifeStatus::Unemployed],
            )];
            let activities = vec![given_active_activity(1, "course", 1, 0, 0)];

            let result = create_activity_planner()
                .plan_day(ActivityDayInput {
                    current_game_day: 0,
                    life_status: LifeStatus::Unemployed,
                    capacities: given_capacities(),
                    catalog: &catalog,
                    activities: &activities,
                })
                .expect("최소 달력일을 판정해야 한다");

            assert_eq!(result.allocations[0].accumulated_effort_units, 5);
            assert_eq!(result.allocations[0].status, ActivityStatus::Active);
        }

        #[test]
        fn given_effort와_최소일이_모두_찼을때_when_계획하면_then_그날_완료한다() {
            let catalog = vec![given_catalog_entry(
                "course",
                3,
                5,
                5,
                vec![LifeStatus::Unemployed],
            )];
            let activities = vec![given_active_activity(1, "course", 1, 0, 5)];

            let result = create_activity_planner()
                .plan_day(ActivityDayInput {
                    current_game_day: 2,
                    life_status: LifeStatus::Employed,
                    capacities: given_capacities(),
                    catalog: &catalog,
                    activities: &activities,
                })
                .expect("활동 완료를 계획해야 한다");

            assert_eq!(result.allocations[0].allocated_effort_units, 0);
            assert_eq!(result.allocations[0].status, ActivityStatus::Completed);
            assert_eq!(result.allocations[0].completed_game_day, Some(2));
        }

        #[test]
        fn given_같은날_완료하는_두_활동_when_계획하면_then_우선순위_순서로_결과를_고정한다() {
            let catalog = vec![
                given_catalog_entry("later", 1, 1, 1, vec![LifeStatus::Unemployed]),
                given_catalog_entry("earlier", 1, 1, 1, vec![LifeStatus::Unemployed]),
            ];
            let activities = vec![
                given_active_activity(20, "later", 2, 0, 0),
                given_active_activity(10, "earlier", 1, 0, 0),
            ];

            let result = create_activity_planner()
                .plan_day(ActivityDayInput {
                    current_game_day: 0,
                    life_status: LifeStatus::Unemployed,
                    capacities: given_capacities(),
                    catalog: &catalog,
                    activities: &activities,
                })
                .expect("같은 날 완료 순서를 계획해야 한다");

            assert_eq!(result.completed_activity_ids, vec![10, 20]);
        }
    }

    mod context_active_불변식이_깨진_경우 {
        use super::*;

        #[test]
        fn given_중복_priority_when_계획하면_then_거절한다() {
            let catalog = vec![given_catalog_entry(
                "course",
                1,
                10,
                10,
                vec![LifeStatus::Unemployed],
            )];
            let activities = vec![
                given_active_activity(1, "course", 1, 0, 0),
                given_active_activity(2, "course", 1, 0, 0),
            ];

            let result = create_activity_planner().plan_day(ActivityDayInput {
                current_game_day: 0,
                life_status: LifeStatus::Unemployed,
                capacities: given_capacities(),
                catalog: &catalog,
                activities: &activities,
            });

            assert_eq!(result, Err(ActivityError::DuplicateActivePriority(1)));
        }

        #[test]
        fn given_active_활동이_세개를_넘을때_when_계획하면_then_상한오류로_거절한다() {
            let catalog = vec![given_catalog_entry(
                "course",
                1,
                10,
                10,
                vec![LifeStatus::Unemployed],
            )];
            let activities = vec![
                given_active_activity(1, "course", 1, 0, 0),
                given_active_activity(2, "course", 2, 0, 0),
                given_active_activity(3, "course", 3, 0, 0),
                given_active_activity(4, "course", 3, 0, 0),
            ];

            let result = create_activity_planner().plan_day(ActivityDayInput {
                current_game_day: 0,
                life_status: LifeStatus::Unemployed,
                capacities: given_capacities(),
                catalog: &catalog,
                activities: &activities,
            });

            assert_eq!(result, Err(ActivityError::TooManyActiveActivities));
        }
    }
}
