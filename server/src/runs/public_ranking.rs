use super::{
    PublicSaveDetail, PublicSaveProgressStatus, PublicSaveRankingItem, PublicSaveRankingMetric,
    PublicSaveRankingPage, PublicSaveRankingQuery, PublicSaveRankingRules,
};

pub struct DefaultPublicSaveRankingRules;

impl PublicSaveRankingRules for DefaultPublicSaveRankingRules {
    fn page(
        &self,
        mut saves: Vec<PublicSaveDetail>,
        query: &PublicSaveRankingQuery,
    ) -> PublicSaveRankingPage {
        saves.retain(|save| matches_query(save, query));
        let ranking_metric = if query.status == Some(PublicSaveProgressStatus::Completed) {
            PublicSaveRankingMetric::AfterTaxNetWorth
        } else {
            PublicSaveRankingMetric::CurrentNetWorth
        };
        saves.sort_by(|left, right| {
            ranking_value(right, ranking_metric)
                .cmp(&ranking_value(left, ranking_metric))
                .then_with(|| left.save_uid.cmp(&right.save_uid))
        });

        let total = u64::try_from(saves.len()).unwrap_or(u64::MAX);
        let start = usize::try_from(query.page)
            .ok()
            .and_then(|page| {
                usize::try_from(query.limit)
                    .ok()
                    .and_then(|limit| page.checked_mul(limit))
            })
            .unwrap_or(usize::MAX);
        let limit = usize::try_from(query.limit).unwrap_or(usize::MAX);
        let items = saves
            .into_iter()
            .enumerate()
            .skip(start)
            .take(limit)
            .map(|(index, save)| PublicSaveRankingItem {
                rank: u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1),
                save_uid: save.save_uid,
                character_name: save.character_name,
                progress_status: save.progress_status,
                game_day: save.game_day,
                age_years: save.age_years,
                net_worth_krw: save.net_worth_krw,
                after_tax_net_worth_krw: save.after_tax_net_worth_krw,
            })
            .collect();

        PublicSaveRankingPage {
            page: query.page,
            limit: query.limit,
            total,
            ranking_metric,
            items,
        }
    }
}

fn matches_query(save: &PublicSaveDetail, query: &PublicSaveRankingQuery) -> bool {
    query
        .status
        .is_none_or(|status| save.progress_status == status)
        && query
            .game_day_from
            .is_none_or(|minimum| save.game_day >= minimum)
        && query
            .game_day_to
            .is_none_or(|maximum| save.game_day <= maximum)
        && query
            .age_from
            .is_none_or(|minimum| save.age_years >= minimum)
        && query.age_to.is_none_or(|maximum| save.age_years <= maximum)
}

fn ranking_value(save: &PublicSaveDetail, metric: PublicSaveRankingMetric) -> i64 {
    match metric {
        PublicSaveRankingMetric::CurrentNetWorth => save.net_worth_krw,
        PublicSaveRankingMetric::AfterTaxNetWorth => {
            save.after_tax_net_worth_krw.unwrap_or(i64::MIN)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_진행중과_완주_세이브가_함께_있는_경우 {
        use super::*;

        #[test]
        fn given_전체조회_when_순위를계산하면_then_현재순자산으로모두정렬한다() {
            let rules = DefaultPublicSaveRankingRules;
            let saves = vec![
                given_save("b", PublicSaveProgressStatus::Completed, 20, Some(100)),
                given_save("a", PublicSaveProgressStatus::InProgress, 30, None),
            ];

            let page = rules.page(saves, &given_query(None));

            assert_eq!(
                page.ranking_metric,
                PublicSaveRankingMetric::CurrentNetWorth
            );
            assert_eq!(page.items[0].character_name, "a");
            assert_eq!(page.items[1].character_name, "b");
        }

        #[test]
        fn given_완주조회_when_순위를계산하면_then_세후순자산으로완주만정렬한다() {
            let rules = DefaultPublicSaveRankingRules;
            let saves = vec![
                given_save("a", PublicSaveProgressStatus::Completed, 30, Some(10)),
                given_save("b", PublicSaveProgressStatus::Completed, 20, Some(40)),
                given_save("c", PublicSaveProgressStatus::InProgress, 50, None),
            ];

            let page = rules.page(
                saves,
                &given_query(Some(PublicSaveProgressStatus::Completed)),
            );

            assert_eq!(
                page.ranking_metric,
                PublicSaveRankingMetric::AfterTaxNetWorth
            );
            assert_eq!(page.total, 2);
            assert_eq!(page.items[0].character_name, "b");
            assert_eq!(page.items[1].character_name, "a");
        }
    }

    mod context_게임일과_연령_구간이_주어진_경우 {
        use super::*;

        #[test]
        fn given_구간밖의세이브_when_순위를계산하면_then_목록에서제외한다() {
            let rules = DefaultPublicSaveRankingRules;
            let mut inside = given_save("inside", PublicSaveProgressStatus::InProgress, 20, None);
            inside.game_day = 400;
            inside.age_years = 31;
            let mut outside = given_save("outside", PublicSaveProgressStatus::InProgress, 30, None);
            outside.game_day = 1_900;
            outside.age_years = 42;
            let query = PublicSaveRankingQuery {
                game_day_from: Some(365),
                game_day_to: Some(1_824),
                age_from: Some(30),
                age_to: Some(39),
                ..given_query(None)
            };

            let page = rules.page(vec![inside, outside], &query);

            assert_eq!(page.total, 1);
            assert_eq!(page.items[0].character_name, "inside");
        }
    }

    fn given_query(status: Option<PublicSaveProgressStatus>) -> PublicSaveRankingQuery {
        PublicSaveRankingQuery {
            page: 0,
            limit: 20,
            status,
            game_day_from: None,
            game_day_to: None,
            age_from: None,
            age_to: None,
        }
    }

    fn given_save(
        name: &str,
        progress_status: PublicSaveProgressStatus,
        net_worth_krw: i64,
        after_tax_net_worth_krw: Option<i64>,
    ) -> PublicSaveDetail {
        PublicSaveDetail {
            save_uid: format!("{name:0<64}"),
            character_name: name.to_owned(),
            progress_status,
            game_day: 0,
            age_years: 20,
            region: crate::character::Region::CapitalArea,
            education: crate::character::Education::HighSchool,
            net_worth_krw,
            wallet_cash_krw: 0,
            liquid_cash_krw: 0,
            cash_product_principal_krw: 0,
            lease_deposit_krw: 0,
            investment_value_krw: 0,
            property_value_krw: 0,
            debt_krw: 0,
            after_tax_net_worth_krw,
            employer_name: None,
            job_family_key: None,
            annual_salary_krw: None,
            household_member_count: None,
            residence_tenure: None,
            active_property_count: 0,
            corporation_name: None,
        }
    }
}
