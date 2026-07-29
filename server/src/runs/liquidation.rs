use std::collections::HashSet;

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::types::{
    LiquidationComponentInput, LiquidationLine, LiquidationPlan, LiquidationPlanner,
};

pub(super) struct DefaultLiquidationPlanner;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalLine<'a> {
    component_key: &'a str,
    cost_krw: i64,
    detail: &'a serde_json::Value,
    gross_krw: i64,
    line_no: u32,
    net_krw: i64,
    policy_reference: &'a str,
    tax_krw: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalPlan<'a> {
    after_tax_net_worth_krw: i64,
    lines: Vec<CanonicalPlanLine<'a>>,
    policy_key: &'a str,
    target_game_day: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalPlanLine<'a> {
    canonical_sha256: &'a str,
    component_key: &'a str,
    line_no: u32,
    net_krw: i64,
}

impl LiquidationPlanner for DefaultLiquidationPlanner {
    fn plan(
        &self,
        policy_key: &str,
        target_game_day: u32,
        components: Vec<LiquidationComponentInput>,
    ) -> Result<LiquidationPlan> {
        ensure!(
            policy_key == "m5c-after-tax-liquidation-v1" && target_game_day > 0,
            "unsupported liquidation authority"
        );
        ensure!(
            (1..=256).contains(&components.len()),
            "liquidation component count is out of range"
        );

        let mut seen = HashSet::new();
        let mut total = 0_i128;
        let mut lines = Vec::with_capacity(components.len());
        for (index, component) in components.into_iter().enumerate() {
            ensure!(
                valid_component_key(&component.component_key),
                "invalid component key"
            );
            ensure!(
                seen.insert(component.component_key.clone()),
                "duplicate liquidation component"
            );
            ensure!(
                component.cost_krw >= 0
                    && component.tax_krw >= 0
                    && !component.policy_reference.is_empty(),
                "invalid liquidation component amount"
            );
            let net = i128::from(component.gross_krw)
                .checked_sub(i128::from(component.cost_krw))
                .and_then(|value| value.checked_sub(i128::from(component.tax_krw)))
                .context("liquidation line overflowed")?;
            let net_krw = i64::try_from(net).context("liquidation line is out of range")?;
            total = total
                .checked_add(net)
                .context("liquidation total overflowed")?;
            let line_no = u32::try_from(index + 1).context("liquidation line number overflowed")?;
            let canonical_json = serde_json::to_string(&CanonicalLine {
                component_key: &component.component_key,
                cost_krw: component.cost_krw,
                detail: &component.detail,
                gross_krw: component.gross_krw,
                line_no,
                net_krw,
                policy_reference: &component.policy_reference,
                tax_krw: component.tax_krw,
            })
            .context("liquidation line cannot be serialized")?;
            let canonical_sha256 = sha256(&canonical_json);
            lines.push(LiquidationLine {
                line_no,
                component_key: component.component_key,
                gross_krw: component.gross_krw,
                cost_krw: component.cost_krw,
                tax_krw: component.tax_krw,
                net_krw,
                policy_reference: component.policy_reference,
                canonical_json,
                canonical_sha256,
            });
        }

        let after_tax_net_worth_krw =
            i64::try_from(total).context("liquidation total is out of range")?;
        let canonical_json = serde_json::to_string(&CanonicalPlan {
            after_tax_net_worth_krw,
            lines: lines
                .iter()
                .map(|line| CanonicalPlanLine {
                    canonical_sha256: &line.canonical_sha256,
                    component_key: &line.component_key,
                    line_no: line.line_no,
                    net_krw: line.net_krw,
                })
                .collect(),
            policy_key,
            target_game_day,
        })
        .context("liquidation plan cannot be serialized")?;
        let canonical_sha256 = sha256(&canonical_json);

        Ok(LiquidationPlan {
            after_tax_net_worth_krw,
            canonical_json,
            canonical_sha256,
            lines,
        })
    }
}

fn valid_component_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('a'..='z'))
        && value.len() <= 64
        && chars.all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn given_component(
        component_key: &str,
        gross_krw: i64,
        cost_krw: i64,
        tax_krw: i64,
    ) -> LiquidationComponentInput {
        LiquidationComponentInput {
            component_key: component_key.to_owned(),
            gross_krw,
            cost_krw,
            tax_krw,
            policy_reference: "authority:1".to_owned(),
            detail: json!({"sourceIds": ["1"]}),
        }
    }

    fn when_planned(components: Vec<LiquidationComponentInput>) -> Result<LiquidationPlan> {
        DefaultLiquidationPlanner.plan("m5c-after-tax-liquidation-v1", 10_950, components)
    }

    mod context_청산_구성요소가_완전한_경우 {
        use super::*;

        #[test]
        fn given_자산과_채무_when_계획하면_then_검사된_순액을_합산한다() {
            let given = vec![
                given_component("cash.walletAccounts", 1_000, 0, 0),
                given_component("asset.marketSecurities", 2_000, 100, 200),
                given_component("liability.personal", -500, 0, 0),
            ];

            let when = when_planned(given).expect("plan should succeed");

            assert_eq!(when.after_tax_net_worth_krw, 2_200);
            assert_eq!(when.lines[1].net_krw, 1_700);
            assert_eq!(when.lines.len(), 3);
        }
    }

    mod context_청산_합계가_bigint를_넘는_경우 {
        use super::*;

        #[test]
        fn given_최대_자산_두개_when_계획하면_then_overflow를_거절한다() {
            let given = vec![
                given_component("cash.walletAccounts", i64::MAX, 0, 0),
                given_component("asset.marketSecurities", i64::MAX, 0, 0),
            ];

            let when = when_planned(given);

            assert!(when.is_err());
        }
    }
}
