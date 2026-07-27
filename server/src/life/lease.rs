use std::sync::Arc;

use time::{Date, Month};

use super::types::{
    CashJeonseMoveInput, CashJeonseMovePlan, LeaseArrearPaymentInput, LeaseArrearPaymentPlan,
    LeaseError, LeaseMoveFundingInput, LeaseMoveFundingLedgerPosting, LeaseMoveFundingPlan,
    LeaseMoveLedgerPosting, LeaseMoveLivingCostAction, LeaseMovePostingLease, LeaseMovePostingLoan,
    LeaseRentLedgerPosting, LeaseRentPostingOwner, LeaseRules, LeaseTermPlan, LeaseTermPlanInput,
    LeaseTerminationReviewDecision, LeaseTerminationReviewInput, MonthlyRentChargeDue,
    MonthlyRentSettlementInput, MonthlyRentSettlementPlan, YearMonth,
};
use crate::finance::LedgerAccountCode;

struct V1LeaseRules;

pub fn create_lease_rules() -> Arc<dyn LeaseRules> {
    Arc::new(V1LeaseRules)
}

impl LeaseRules for V1LeaseRules {
    fn plan_cash_jeonse_move(
        &self,
        input: CashJeonseMoveInput,
    ) -> Result<CashJeonseMovePlan, LeaseError> {
        validate_input(input)?;

        let available_cash_krw = input
            .wallet_cash_krw
            .checked_add(input.existing_deposit_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let required_cash_krw = input
            .new_deposit_krw
            .checked_add(input.moving_cost_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        if available_cash_krw < required_cash_krw {
            return Err(LeaseError::InsufficientWalletCash);
        }

        let wallet_after_krw = available_cash_krw
            .checked_sub(required_cash_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let wallet_delta_krw = wallet_after_krw
            .checked_sub(input.wallet_cash_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let lease_deposit_asset_delta_krw = input
            .new_deposit_krw
            .checked_sub(input.existing_deposit_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let net_worth_delta_krw = input
            .moving_cost_krw
            .checked_neg()
            .ok_or(LeaseError::ArithmeticOverflow)?;

        let mut postings = Vec::with_capacity(if input.existing_deposit_krw == 0 {
            4
        } else {
            6
        });
        if input.existing_deposit_krw != 0 {
            postings.push(LeaseMoveLedgerPosting {
                account_code: LedgerAccountCode::LeaseDepositAsset,
                lease_contract: Some(LeaseMovePostingLease::Ended),
                amount_krw: input
                    .existing_deposit_krw
                    .checked_neg()
                    .ok_or(LeaseError::ArithmeticOverflow)?,
            });
            postings.push(LeaseMoveLedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                lease_contract: None,
                amount_krw: input.existing_deposit_krw,
            });
        }
        postings.extend([
            LeaseMoveLedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                lease_contract: None,
                amount_krw: input
                    .new_deposit_krw
                    .checked_neg()
                    .ok_or(LeaseError::ArithmeticOverflow)?,
            },
            LeaseMoveLedgerPosting {
                account_code: LedgerAccountCode::LeaseDepositAsset,
                lease_contract: Some(LeaseMovePostingLease::Started),
                amount_krw: input.new_deposit_krw,
            },
            LeaseMoveLedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                lease_contract: None,
                amount_krw: input
                    .moving_cost_krw
                    .checked_neg()
                    .ok_or(LeaseError::ArithmeticOverflow)?,
            },
            LeaseMoveLedgerPosting {
                account_code: LedgerAccountCode::MovingExpense,
                lease_contract: None,
                amount_krw: input.moving_cost_krw,
            },
        ]);

        Ok(CashJeonseMovePlan {
            returned_deposit_krw: input.existing_deposit_krw,
            deposit_krw: input.new_deposit_krw,
            moving_cost_krw: input.moving_cost_krw,
            wallet_delta_krw,
            wallet_after_krw,
            tenant_lease_deposit_krw: input.new_deposit_krw,
            lease_deposit_asset_delta_krw,
            net_worth_delta_krw,
            living_cost_action: LeaseMoveLivingCostAction::PreserveCurrentMonth,
            postings,
        })
    }

    fn plan_lease_move_funding(
        &self,
        input: LeaseMoveFundingInput,
    ) -> Result<LeaseMoveFundingPlan, LeaseError> {
        validate_funding_input(input)?;

        let returned_cash_krw = input
            .existing_deposit_krw
            .checked_sub(input.repaid_loan_principal_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let tenant_cash_for_deposit_krw = input
            .new_deposit_krw
            .checked_sub(input.new_loan_principal_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let wallet_after_krw = input
            .wallet_cash_krw
            .checked_add(returned_cash_krw)
            .and_then(|value| value.checked_sub(tenant_cash_for_deposit_krw))
            .and_then(|value| value.checked_sub(input.moving_cost_krw))
            .ok_or(LeaseError::ArithmeticOverflow)?;
        if wallet_after_krw < 0 {
            return Err(LeaseError::InsufficientWalletCash);
        }

        let wallet_delta_krw = wallet_after_krw
            .checked_sub(input.wallet_cash_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let debt_delta_krw = input
            .new_loan_principal_krw
            .checked_sub(input.repaid_loan_principal_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let lease_deposit_asset_delta_krw = input
            .new_deposit_krw
            .checked_sub(input.existing_deposit_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let net_worth_delta_krw = input
            .moving_cost_krw
            .checked_neg()
            .ok_or(LeaseError::ArithmeticOverflow)?;

        let mut postings = Vec::with_capacity(8);
        push_funding_posting(
            &mut postings,
            LedgerAccountCode::LeaseDepositAsset,
            Some(LeaseMovePostingLease::Ended),
            None,
            input
                .existing_deposit_krw
                .checked_neg()
                .ok_or(LeaseError::ArithmeticOverflow)?,
        );
        push_funding_posting(
            &mut postings,
            LedgerAccountCode::LoanPrincipalLiability,
            None,
            Some(LeaseMovePostingLoan::Repaid),
            input.repaid_loan_principal_krw,
        );
        push_funding_posting(
            &mut postings,
            LedgerAccountCode::Wallet,
            None,
            None,
            returned_cash_krw,
        );
        push_funding_posting(
            &mut postings,
            LedgerAccountCode::LeaseDepositAsset,
            Some(LeaseMovePostingLease::Started),
            None,
            input.new_deposit_krw,
        );
        push_funding_posting(
            &mut postings,
            LedgerAccountCode::LoanPrincipalLiability,
            None,
            Some(LeaseMovePostingLoan::Originated),
            input
                .new_loan_principal_krw
                .checked_neg()
                .ok_or(LeaseError::ArithmeticOverflow)?,
        );
        push_funding_posting(
            &mut postings,
            LedgerAccountCode::Wallet,
            None,
            None,
            tenant_cash_for_deposit_krw
                .checked_neg()
                .ok_or(LeaseError::ArithmeticOverflow)?,
        );
        push_funding_posting(
            &mut postings,
            LedgerAccountCode::MovingExpense,
            None,
            None,
            input.moving_cost_krw,
        );
        push_funding_posting(
            &mut postings,
            LedgerAccountCode::Wallet,
            None,
            None,
            input
                .moving_cost_krw
                .checked_neg()
                .ok_or(LeaseError::ArithmeticOverflow)?,
        );

        Ok(LeaseMoveFundingPlan {
            returned_deposit_krw: input.existing_deposit_krw,
            repaid_loan_principal_krw: input.repaid_loan_principal_krw,
            deposit_krw: input.new_deposit_krw,
            new_loan_principal_krw: input.new_loan_principal_krw,
            moving_cost_krw: input.moving_cost_krw,
            wallet_delta_krw,
            wallet_after_krw,
            debt_delta_krw,
            tenant_lease_deposit_krw: input.new_deposit_krw,
            lease_deposit_asset_delta_krw,
            net_worth_delta_krw,
            living_cost_action: LeaseMoveLivingCostAction::PreserveCurrentMonth,
            postings,
        })
    }

    fn plan_monthly_rent_settlement(
        &self,
        input: MonthlyRentSettlementInput,
    ) -> Result<MonthlyRentSettlementPlan, LeaseError> {
        if input.wallet_cash_krw < 0 {
            return Err(LeaseError::InvalidWalletCash);
        }
        if input.monthly_rent_krw <= 0 {
            return Err(LeaseError::InvalidMonthlyRent);
        }

        let paid_krw = input.wallet_cash_krw.min(input.monthly_rent_krw);
        let arrear_krw = input
            .monthly_rent_krw
            .checked_sub(paid_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let wallet_after_krw = input
            .wallet_cash_krw
            .checked_sub(paid_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let mut postings = vec![LeaseRentLedgerPosting {
            account_code: LedgerAccountCode::LeaseRentExpense,
            owner: LeaseRentPostingOwner::RentCharge,
            amount_krw: input.monthly_rent_krw,
        }];
        if paid_krw > 0 {
            postings.push(LeaseRentLedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                owner: LeaseRentPostingOwner::None,
                amount_krw: paid_krw
                    .checked_neg()
                    .ok_or(LeaseError::ArithmeticOverflow)?,
            });
        }
        if arrear_krw > 0 {
            postings.push(LeaseRentLedgerPosting {
                account_code: LedgerAccountCode::LeaseArrearLiability,
                owner: LeaseRentPostingOwner::Arrear,
                amount_krw: arrear_krw
                    .checked_neg()
                    .ok_or(LeaseError::ArithmeticOverflow)?,
            });
        }

        Ok(MonthlyRentSettlementPlan {
            paid_krw,
            arrear_krw,
            wallet_after_krw,
            postings,
        })
    }

    fn plan_lease_arrear_payment(
        &self,
        input: LeaseArrearPaymentInput,
    ) -> Result<LeaseArrearPaymentPlan, LeaseError> {
        if input.wallet_cash_krw < 0 {
            return Err(LeaseError::InvalidWalletCash);
        }
        if input.outstanding_krw <= 0 {
            return Err(LeaseError::InvalidArrearBalance);
        }
        if input.amount_krw <= 0 {
            return Err(LeaseError::InvalidArrearPayment);
        }
        if input.amount_krw > input.outstanding_krw {
            return Err(LeaseError::ArrearPaymentExceedsOutstanding);
        }
        if input.amount_krw > input.wallet_cash_krw {
            return Err(LeaseError::InsufficientWalletCash);
        }

        let wallet_after_krw = input
            .wallet_cash_krw
            .checked_sub(input.amount_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let remaining_krw = input
            .outstanding_krw
            .checked_sub(input.amount_krw)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        Ok(LeaseArrearPaymentPlan {
            paid_krw: input.amount_krw,
            remaining_krw,
            wallet_after_krw,
            postings: vec![
                LeaseRentLedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    owner: LeaseRentPostingOwner::None,
                    amount_krw: input
                        .amount_krw
                        .checked_neg()
                        .ok_or(LeaseError::ArithmeticOverflow)?,
                },
                LeaseRentLedgerPosting {
                    account_code: LedgerAccountCode::LeaseArrearLiability,
                    owner: LeaseRentPostingOwner::Arrear,
                    amount_krw: input.amount_krw,
                },
            ],
        })
    }

    fn next_monthly_rent_charge(
        &self,
        current_game_day: u32,
        market_date: Date,
    ) -> Result<MonthlyRentChargeDue, LeaseError> {
        let (year, month) = if market_date.month() == Month::December {
            (
                market_date
                    .year()
                    .checked_add(1)
                    .ok_or(LeaseError::ArithmeticOverflow)?,
                Month::January,
            )
        } else {
            (
                market_date.year(),
                Month::try_from(u8::from(market_date.month()) + 1)
                    .map_err(|_| LeaseError::ArithmeticOverflow)?,
            )
        };
        let due_date =
            Date::from_calendar_date(year, month, 1).map_err(|_| LeaseError::ArithmeticOverflow)?;
        let days_until_due = u32::try_from((due_date - market_date).whole_days())
            .map_err(|_| LeaseError::ArithmeticOverflow)?;
        let due_game_day = current_game_day
            .checked_add(days_until_due)
            .ok_or(LeaseError::ArithmeticOverflow)?;

        Ok(MonthlyRentChargeDue {
            due_game_day,
            due_year_month: YearMonth {
                year,
                month: u8::from(month),
            },
        })
    }

    fn plan_lease_term(&self, input: LeaseTermPlanInput) -> Result<LeaseTermPlan, LeaseError> {
        if input.term_no == 0 {
            return Err(LeaseError::InvalidTermNumber);
        }
        if input.term_months == 0 {
            return Err(LeaseError::InvalidTermMonths);
        }
        if input.renewal_notice_lead_days == 0 {
            return Err(LeaseError::InvalidRenewalNoticeLeadDays);
        }

        let prior_term_no = input
            .term_no
            .checked_sub(1)
            .ok_or(LeaseError::InvalidTermNumber)?;
        let from_month_offset = prior_term_no
            .checked_mul(u32::from(input.term_months))
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let to_month_offset = input
            .term_no
            .checked_mul(u32::from(input.term_months))
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let from_date = add_anchor_months(input.anchor_date, from_month_offset)?;
        let to_date = add_anchor_months(input.anchor_date, to_month_offset)?;
        let from_offset = game_day_offset(input.anchor_date, from_date)?;
        let to_offset = game_day_offset(input.anchor_date, to_date)?;
        let effective_from_game_day = input
            .anchor_game_day
            .checked_add(from_offset)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let effective_to_game_day = input
            .anchor_game_day
            .checked_add(to_offset)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let term_days = effective_to_game_day
            .checked_sub(effective_from_game_day)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let notice_lead_days = u32::from(input.renewal_notice_lead_days);
        if notice_lead_days > term_days {
            return Err(LeaseError::InvalidRenewalNoticeLeadDays);
        }
        let renewal_notice_game_day = effective_to_game_day
            .checked_sub(notice_lead_days)
            .ok_or(LeaseError::ArithmeticOverflow)?;

        Ok(LeaseTermPlan {
            term_no: input.term_no,
            effective_from_game_day,
            effective_to_game_day,
            renewal_notice_game_day,
            renewal_game_day: effective_to_game_day,
        })
    }

    fn decide_lease_termination_review(
        &self,
        input: LeaseTerminationReviewInput,
    ) -> Result<LeaseTerminationReviewDecision, LeaseError> {
        if input.review_after_days == 0 {
            return Err(LeaseError::InvalidTerminationReviewAfterDays);
        }
        if input
            .oldest_active_arrear_created_game_day
            .is_some_and(|created_game_day| created_game_day > input.current_game_day)
        {
            return Err(LeaseError::InvalidArrearGameDay);
        }

        if input.review_is_open {
            return Ok(if input.oldest_active_arrear_created_game_day.is_some() {
                LeaseTerminationReviewDecision::KeepOpen
            } else {
                LeaseTerminationReviewDecision::Resolve
            });
        }

        let Some(created_game_day) = input.oldest_active_arrear_created_game_day else {
            return Ok(LeaseTerminationReviewDecision::NoAction);
        };
        let due_game_day = created_game_day
            .checked_add(u32::from(input.review_after_days))
            .ok_or(LeaseError::ArithmeticOverflow)?;
        if input.current_game_day >= due_game_day {
            Ok(LeaseTerminationReviewDecision::Open)
        } else {
            Ok(LeaseTerminationReviewDecision::Schedule { due_game_day })
        }
    }
}

fn add_anchor_months(anchor: Date, months: u32) -> Result<Date, LeaseError> {
    let anchor_month_index = i64::from(anchor.year())
        .checked_mul(12)
        .and_then(|value| value.checked_add(i64::from(u8::from(anchor.month())) - 1))
        .ok_or(LeaseError::ArithmeticOverflow)?;
    let target_month_index = anchor_month_index
        .checked_add(i64::from(months))
        .ok_or(LeaseError::ArithmeticOverflow)?;
    let target_year = i32::try_from(target_month_index.div_euclid(12))
        .map_err(|_| LeaseError::ArithmeticOverflow)?;
    let target_month = Month::try_from(
        u8::try_from(target_month_index.rem_euclid(12) + 1)
            .map_err(|_| LeaseError::ArithmeticOverflow)?,
    )
    .map_err(|_| LeaseError::ArithmeticOverflow)?;

    (1..=anchor.day())
        .rev()
        .find_map(|day| Date::from_calendar_date(target_year, target_month, day).ok())
        .ok_or(LeaseError::ArithmeticOverflow)
}

fn game_day_offset(anchor: Date, boundary: Date) -> Result<u32, LeaseError> {
    u32::try_from((boundary - anchor).whole_days()).map_err(|_| LeaseError::ArithmeticOverflow)
}

fn validate_input(input: CashJeonseMoveInput) -> Result<(), LeaseError> {
    if input.wallet_cash_krw < 0 {
        return Err(LeaseError::InvalidWalletCash);
    }
    if input.existing_deposit_krw < 0 {
        return Err(LeaseError::InvalidExistingDeposit);
    }
    if input.new_deposit_krw <= 0 {
        return Err(LeaseError::InvalidNewDeposit);
    }
    if input.moving_cost_krw <= 0 {
        return Err(LeaseError::InvalidMovingCost);
    }
    Ok(())
}

fn validate_funding_input(input: LeaseMoveFundingInput) -> Result<(), LeaseError> {
    if input.wallet_cash_krw < 0 {
        return Err(LeaseError::InvalidWalletCash);
    }
    if input.existing_deposit_krw < 0 {
        return Err(LeaseError::InvalidExistingDeposit);
    }
    if input.repaid_loan_principal_krw < 0
        || input.repaid_loan_principal_krw > input.existing_deposit_krw
    {
        return Err(LeaseError::InvalidRepaidLoanPrincipal);
    }
    if input.new_deposit_krw <= 0 {
        return Err(LeaseError::InvalidNewDeposit);
    }
    if input.new_loan_principal_krw < 0 || input.new_loan_principal_krw > input.new_deposit_krw {
        return Err(LeaseError::InvalidNewLoanPrincipal);
    }
    if input.moving_cost_krw <= 0 {
        return Err(LeaseError::InvalidMovingCost);
    }
    Ok(())
}

fn push_funding_posting(
    postings: &mut Vec<LeaseMoveFundingLedgerPosting>,
    account_code: LedgerAccountCode,
    lease_contract: Option<LeaseMovePostingLease>,
    loan_contract: Option<LeaseMovePostingLoan>,
    amount_krw: i64,
) {
    if amount_krw != 0 {
        postings.push(LeaseMoveFundingLedgerPosting {
            account_code,
            lease_contract,
            loan_contract,
            amount_krw,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given_rules() -> Arc<dyn LeaseRules> {
        create_lease_rules()
    }

    fn given_input(
        wallet_cash_krw: i64,
        existing_deposit_krw: i64,
        new_deposit_krw: i64,
        moving_cost_krw: i64,
    ) -> CashJeonseMoveInput {
        CashJeonseMoveInput {
            wallet_cash_krw,
            existing_deposit_krw,
            new_deposit_krw,
            moving_cost_krw,
        }
    }

    fn when_plan(input: CashJeonseMoveInput) -> Result<CashJeonseMovePlan, LeaseError> {
        given_rules().plan_cash_jeonse_move(input)
    }

    fn given_funding_input(
        wallet_cash_krw: i64,
        existing_deposit_krw: i64,
        repaid_loan_principal_krw: i64,
        new_deposit_krw: i64,
        new_loan_principal_krw: i64,
        moving_cost_krw: i64,
    ) -> LeaseMoveFundingInput {
        LeaseMoveFundingInput {
            wallet_cash_krw,
            existing_deposit_krw,
            repaid_loan_principal_krw,
            new_deposit_krw,
            new_loan_principal_krw,
            moving_cost_krw,
        }
    }

    fn when_plan_funding(input: LeaseMoveFundingInput) -> Result<LeaseMoveFundingPlan, LeaseError> {
        given_rules().plan_lease_move_funding(input)
    }

    fn given_term_input(
        anchor_game_day: u32,
        anchor_date: Date,
        term_no: u32,
        term_months: u16,
        renewal_notice_lead_days: u16,
    ) -> LeaseTermPlanInput {
        LeaseTermPlanInput {
            anchor_game_day,
            anchor_date,
            term_no,
            term_months,
            renewal_notice_lead_days,
        }
    }

    fn when_plan_term(input: LeaseTermPlanInput) -> Result<LeaseTermPlan, LeaseError> {
        given_rules().plan_lease_term(input)
    }

    fn when_decide_review(
        input: LeaseTerminationReviewInput,
    ) -> Result<LeaseTerminationReviewDecision, LeaseError> {
        given_rules().decide_lease_termination_review(input)
    }

    mod context_첫_현금전세로_이동하는_경우 {
        use super::*;

        #[test]
        fn given_지갑과_새보증금과_이사비_when_계획하면_then_네개의_0원이아닌_posting을_만든다() {
            let input = given_input(12_000_000, 0, 10_000_000, 800_000);

            let plan = when_plan(input).expect("현금 전세 이동을 계획해야 한다");

            assert_eq!(
                plan.postings,
                vec![
                    LeaseMoveLedgerPosting {
                        account_code: LedgerAccountCode::Wallet,
                        lease_contract: None,
                        amount_krw: -10_000_000,
                    },
                    LeaseMoveLedgerPosting {
                        account_code: LedgerAccountCode::LeaseDepositAsset,
                        lease_contract: Some(LeaseMovePostingLease::Started),
                        amount_krw: 10_000_000,
                    },
                    LeaseMoveLedgerPosting {
                        account_code: LedgerAccountCode::Wallet,
                        lease_contract: None,
                        amount_krw: -800_000,
                    },
                    LeaseMoveLedgerPosting {
                        account_code: LedgerAccountCode::MovingExpense,
                        lease_contract: None,
                        amount_krw: 800_000,
                    },
                ]
            );
        }
    }

    mod context_기존_전세에서_새_전세로_이동하는_경우 {
        use super::*;

        #[test]
        fn given_기존보증금_when_계획하면_then_먼저반환하고_새보증금을_자산화한다() {
            let input = given_input(1_000_000, 10_000_000, 9_000_000, 600_000);

            let plan = when_plan(input).expect("전세 교체를 계획해야 한다");

            assert_eq!(
                (
                    plan.returned_deposit_krw,
                    plan.tenant_lease_deposit_krw,
                    plan.wallet_delta_krw,
                    plan.wallet_after_krw,
                    plan.lease_deposit_asset_delta_krw,
                    plan.postings.len(),
                    plan.postings
                        .iter()
                        .map(|posting| posting.amount_krw)
                        .sum::<i64>(),
                ),
                (10_000_000, 9_000_000, 400_000, 1_400_000, -1_000_000, 6, 0)
            );
        }

        #[test]
        fn given_보증금교체와_이사비_when_계획하면_then_순자산은_이사비만큼만_감소한다() {
            let input = given_input(5_000_000, 20_000_000, 22_000_000, 600_000);

            let plan = when_plan(input).expect("전세 교체를 계획해야 한다");

            assert_eq!(
                (
                    plan.net_worth_delta_krw,
                    plan.wallet_after_krw + plan.tenant_lease_deposit_krw,
                ),
                (
                    -600_000,
                    input.wallet_cash_krw + input.existing_deposit_krw - input.moving_cost_krw,
                )
            );
        }

        #[test]
        fn given_확정된_당월생활비_when_계획하면_then_재계산하지않고_보존한다() {
            let input = given_input(5_000_000, 20_000_000, 22_000_000, 600_000);

            let plan = when_plan(input).expect("전세 교체를 계획해야 한다");

            assert_eq!(
                plan.living_cost_action,
                LeaseMoveLivingCostAction::PreserveCurrentMonth
            );
        }
    }

    mod context_전세대출로_첫_전세에_입주하는_경우 {
        use super::*;

        #[test]
        fn given_보증금80퍼센트대출_when_계획하면_then_원금을지갑에노출하지않고직접충당한다() {
            let input = given_funding_input(25_000_000, 0, 0, 100_000_000, 80_000_000, 800_000);

            let plan = when_plan_funding(input).expect("대출 전세 이동을 계획해야 한다");

            assert_eq!(
                (
                    plan.wallet_delta_krw,
                    plan.wallet_after_krw,
                    plan.debt_delta_krw,
                    plan.lease_deposit_asset_delta_krw,
                ),
                (-20_800_000, 4_200_000, 80_000_000, 100_000_000)
            );
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| posting.amount_krw)
                    .sum::<i64>(),
                0
            );
        }
    }

    mod context_기존전세대출을_상환하고_새전세대출로_갈아타는_경우 {
        use super::*;

        #[test]
        fn given_기존원금과신규원금_when_계획하면_then_wallet과debt_delta를대수식대로계산한다() {
            let input = given_funding_input(
                5_000_000,
                100_000_000,
                70_000_000,
                120_000_000,
                90_000_000,
                600_000,
            );

            let plan = when_plan_funding(input).expect("전세대출 대환 이동을 계획해야 한다");

            assert_eq!(
                (
                    plan.wallet_delta_krw,
                    plan.wallet_after_krw,
                    plan.debt_delta_krw,
                    plan.net_worth_delta_krw,
                ),
                (-600_000, 4_400_000, 20_000_000, -600_000)
            );
            assert!(plan.postings.iter().any(|posting| {
                posting.account_code == LedgerAccountCode::LoanPrincipalLiability
                    && posting.loan_contract == Some(LeaseMovePostingLoan::Repaid)
                    && posting.amount_krw == 70_000_000
            }));
            assert!(plan.postings.iter().any(|posting| {
                posting.account_code == LedgerAccountCode::LoanPrincipalLiability
                    && posting.loan_contract == Some(LeaseMovePostingLoan::Originated)
                    && posting.amount_krw == -90_000_000
            }));
        }

        #[test]
        fn given_보증금과대출원금이각각같을때_when_계획하면_then_0원posting을제거한다() {
            let input = given_funding_input(
                1_000_000,
                80_000_000,
                80_000_000,
                100_000_000,
                100_000_000,
                600_000,
            );

            let plan = when_plan_funding(input).expect("전액 대출 대환을 계획해야 한다");

            assert!(plan.postings.iter().all(|posting| posting.amount_krw != 0));
            assert_eq!(plan.postings.len(), 6);
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| posting.amount_krw)
                    .sum::<i64>(),
                0
            );
        }
    }

    mod context_가용현금이_부족한_경우 {
        use super::*;

        #[test]
        fn given_반환보증금과지갑보다_필요액이클때_when_계획하면_then_잔액부족으로_거절한다() {
            let input = given_input(100_000, 10_000_000, 10_000_000, 600_000);

            let result = when_plan(input);

            assert_eq!(result, Err(LeaseError::InsufficientWalletCash));
        }
    }

    mod context_금액계산이_범위를_넘는_경우 {
        use super::*;

        #[test]
        fn given_i64최대지갑과_반환보증금_when_계획하면_then_overflow로_거절한다() {
            let input = given_input(i64::MAX, 1, 1, 1);

            let result = when_plan(input);

            assert_eq!(result, Err(LeaseError::ArithmeticOverflow));
        }
    }

    mod context_월세를_전액_낼_수_있는_경우 {
        use super::*;

        #[test]
        fn given_충분한지갑_when_정산하면_then_월세전액을내고_연체를만들지않는다() {
            let input = MonthlyRentSettlementInput {
                wallet_cash_krw: 1_000_000,
                monthly_rent_krw: 600_000,
            };

            let plan = given_rules()
                .plan_monthly_rent_settlement(input)
                .expect("월세를 정산해야 한다");

            assert_eq!(
                (plan.paid_krw, plan.arrear_krw, plan.wallet_after_krw),
                (600_000, 0, 400_000)
            );
            assert_eq!(plan.postings.len(), 2);
            assert!(plan.postings.iter().all(|posting| posting.amount_krw != 0));
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| posting.amount_krw)
                    .sum::<i64>(),
                0
            );
        }
    }

    mod context_월세를_일부만_낼_수_있는_경우 {
        use super::*;

        #[test]
        fn given_부족한지갑_when_정산하면_then_지갑을0으로만들고_나머지를연체한다() {
            let input = MonthlyRentSettlementInput {
                wallet_cash_krw: 200_000,
                monthly_rent_krw: 600_000,
            };

            let plan = given_rules()
                .plan_monthly_rent_settlement(input)
                .expect("월세를 부분 정산해야 한다");

            assert_eq!(
                (plan.paid_krw, plan.arrear_krw, plan.wallet_after_krw),
                (200_000, 400_000, 0)
            );
            assert_eq!(plan.postings.len(), 3);
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| posting.amount_krw)
                    .sum::<i64>(),
                0
            );
        }
    }

    mod context_월세를_전혀_낼_수_없는_경우 {
        use super::*;

        #[test]
        fn given_빈지갑_when_정산하면_then_0원_wallet_posting없이_전액연체한다() {
            let input = MonthlyRentSettlementInput {
                wallet_cash_krw: 0,
                monthly_rent_krw: 600_000,
            };

            let plan = given_rules()
                .plan_monthly_rent_settlement(input)
                .expect("월세 전액을 연체해야 한다");

            assert_eq!(
                (plan.paid_krw, plan.arrear_krw, plan.wallet_after_krw),
                (0, 600_000, 0)
            );
            assert_eq!(plan.postings.len(), 2);
            assert!(plan.postings.iter().all(|posting| posting.amount_krw != 0));
        }
    }

    mod context_월세연체를_일부_상환하는_경우 {
        use super::*;

        #[test]
        fn given_연체잔액과충분한지갑_when_상환하면_then_잔액과지갑을같이줄인다() {
            let input = LeaseArrearPaymentInput {
                wallet_cash_krw: 500_000,
                outstanding_krw: 400_000,
                amount_krw: 150_000,
            };

            let plan = given_rules()
                .plan_lease_arrear_payment(input)
                .expect("월세 연체를 상환해야 한다");

            assert_eq!(
                (plan.paid_krw, plan.remaining_krw, plan.wallet_after_krw),
                (150_000, 250_000, 350_000)
            );
            assert_eq!(
                plan.postings
                    .iter()
                    .map(|posting| posting.amount_krw)
                    .sum::<i64>(),
                0
            );
        }
    }

    mod context_다음_월세_청구일을_계산하는_경우 {
        use super::*;

        #[test]
        fn given_월중입주_when_계산하면_then_다음달1일을_청구일로고정한다() {
            let market_date = Date::from_calendar_date(2026, Month::January, 20)
                .expect("유효한 시장 날짜여야 한다");

            let due = given_rules()
                .next_monthly_rent_charge(40, market_date)
                .expect("다음 월세 청구일을 계산해야 한다");

            assert_eq!(due.due_game_day, 52);
            assert_eq!(
                due.due_year_month,
                YearMonth {
                    year: 2026,
                    month: 2
                }
            );
        }

        #[test]
        fn given_월초입주_when_계산하면_then_같은날이아닌_다음달1일을_청구한다() {
            let market_date = Date::from_calendar_date(2026, Month::December, 1)
                .expect("유효한 시장 날짜여야 한다");

            let due = given_rules()
                .next_monthly_rent_charge(335, market_date)
                .expect("다음 월세 청구일을 계산해야 한다");

            assert_eq!(due.due_game_day, 366);
            assert_eq!(
                due.due_year_month,
                YearMonth {
                    year: 2027,
                    month: 1
                }
            );
        }
    }

    mod context_월말에_시작한_고정기간_계약인_경우 {
        use super::*;

        #[test]
        fn given_1월31일_anchor_when_두번째term을계획하면_then_직전clamp가아닌anchor에서3월31일을계산한다()
         {
            let anchor_date = Date::from_calendar_date(2026, Month::January, 31)
                .expect("유효한 계약 시작일이어야 한다");
            let first = when_plan_term(given_term_input(100, anchor_date, 1, 1, 1))
                .expect("첫 term을 계획해야 한다");

            let second = when_plan_term(given_term_input(100, anchor_date, 2, 1, 1))
                .expect("두 번째 term을 계획해야 한다");

            assert_eq!(
                (
                    first.effective_to_game_day,
                    second.effective_from_game_day,
                    second.effective_to_game_day,
                ),
                (128, 128, 159)
            );
        }
    }

    mod context_윤년_말일에_시작한_고정기간_계약인_경우 {
        use super::*;

        #[test]
        fn given_2월29일_anchor_when_네번째12개월term을계획하면_then_윤년29일경계를회복한다() {
            let anchor_date = Date::from_calendar_date(2028, Month::February, 29)
                .expect("유효한 윤년 시작일이어야 한다");
            let input = given_term_input(0, anchor_date, 4, 12, 30);

            let plan = when_plan_term(input).expect("네 번째 term을 계획해야 한다");

            assert_eq!(
                (plan.effective_from_game_day, plan.effective_to_game_day),
                (1_095, 1_461)
            );
        }
    }

    mod context_12월에_시작한_고정기간_계약인_경우 {
        use super::*;

        #[test]
        fn given_12월31일_anchor_when_한달term을계획하면_then_다음해1월경계를계산한다() {
            let anchor_date = Date::from_calendar_date(2026, Month::December, 31)
                .expect("유효한 연말 시작일이어야 한다");
            let input = given_term_input(70, anchor_date, 1, 1, 1);

            let plan = when_plan_term(input).expect("연도를 넘는 term을 계획해야 한다");

            assert_eq!(
                (plan.effective_from_game_day, plan.effective_to_game_day),
                (70, 101)
            );
        }
    }

    mod context_12개월_자동갱신_계약인_경우 {
        use super::*;

        #[test]
        fn given_30일_안내선행기간_when_term을계획하면_then_만료30일전에안내한다() {
            let anchor_date = Date::from_calendar_date(2026, Month::January, 15)
                .expect("유효한 계약 시작일이어야 한다");
            let input = given_term_input(10, anchor_date, 1, 12, 30);

            let plan = when_plan_term(input).expect("12개월 term을 계획해야 한다");

            assert_eq!(
                (
                    plan.renewal_notice_game_day,
                    plan.renewal_game_day,
                    plan.effective_to_game_day,
                ),
                (345, 375, 375)
            );
        }
    }

    mod context_term_game_day가_범위를_넘는_경우 {
        use super::*;

        #[test]
        fn given_u32최대_anchor_when_term을계획하면_then_overflow로거절한다() {
            let anchor_date = Date::from_calendar_date(2026, Month::January, 1)
                .expect("유효한 계약 시작일이어야 한다");
            let input = given_term_input(u32::MAX, anchor_date, 1, 12, 30);

            let result = when_plan_term(input);

            assert_eq!(result, Err(LeaseError::ArithmeticOverflow));
        }
    }

    mod context_활성_월세연체가_60일에_도달하기_전인_경우 {
        use super::*;

        #[test]
        fn given_59일된_가장오래된연체_when_검토하면_then_60일째_action을예약한다() {
            let input = LeaseTerminationReviewInput {
                current_game_day: 159,
                review_after_days: 60,
                oldest_active_arrear_created_game_day: Some(100),
                review_is_open: false,
            };

            let decision = when_decide_review(input).expect("검토 일정을 판단해야 한다");

            assert_eq!(
                decision,
                LeaseTerminationReviewDecision::Schedule { due_game_day: 160 }
            );
        }
    }

    mod context_활성_월세연체가_60일에_도달한_경우 {
        use super::*;

        #[test]
        fn given_60일된_가장오래된연체_when_검토하면_then_종료검토를연다() {
            let input = LeaseTerminationReviewInput {
                current_game_day: 160,
                review_after_days: 60,
                oldest_active_arrear_created_game_day: Some(100),
                review_is_open: false,
            };

            let decision = when_decide_review(input).expect("종료 검토를 판단해야 한다");

            assert_eq!(decision, LeaseTerminationReviewDecision::Open);
        }
    }

    mod context_종료검토가_열려있고_활성연체가_남은_경우 {
        use super::*;

        #[test]
        fn given_open검토와_활성연체_when_검토하면_then_open상태를유지한다() {
            let input = LeaseTerminationReviewInput {
                current_game_day: 200,
                review_after_days: 60,
                oldest_active_arrear_created_game_day: Some(180),
                review_is_open: true,
            };

            let decision = when_decide_review(input).expect("열린 검토를 판단해야 한다");

            assert_eq!(decision, LeaseTerminationReviewDecision::KeepOpen);
        }
    }

    mod context_종료검토가_열려있고_활성연체를_모두갚은_경우 {
        use super::*;

        #[test]
        fn given_open검토와_빈활성연체_when_검토하면_then_검토를해소한다() {
            let input = LeaseTerminationReviewInput {
                current_game_day: 200,
                review_after_days: 60,
                oldest_active_arrear_created_game_day: None,
                review_is_open: true,
            };

            let decision = when_decide_review(input).expect("검토 해소를 판단해야 한다");

            assert_eq!(decision, LeaseTerminationReviewDecision::Resolve);
        }
    }

    mod context_연체검토_due_game_day가_범위를_넘는_경우 {
        use super::*;

        #[test]
        fn given_u32범위끝의_연체_when_검토하면_then_overflow로거절한다() {
            let input = LeaseTerminationReviewInput {
                current_game_day: u32::MAX - 30,
                review_after_days: 60,
                oldest_active_arrear_created_game_day: Some(u32::MAX - 30),
                review_is_open: false,
            };

            let result = when_decide_review(input);

            assert_eq!(result, Err(LeaseError::ArithmeticOverflow));
        }
    }
}
