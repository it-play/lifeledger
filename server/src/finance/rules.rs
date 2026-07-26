use std::sync::Arc;

use super::types::{
    DailyInterestAccrual, DailyInterestInput, FinanceRuleError, FinanceRules,
    FinancialAccountStatus, LedgerAccountCode, LedgerPosting, LedgerSource, LedgerSourceKind,
    LedgerTransaction, LedgerTransactionDraft, PostingAccountRequirement, TransferDirection,
    TransferInput, TransferMutation,
};

const INTEREST_DENOMINATOR: i128 = 365 * 10_000;

struct DefaultFinanceRules;

pub fn create_finance_rules() -> Arc<dyn FinanceRules> {
    Arc::new(DefaultFinanceRules)
}

impl FinanceRules for DefaultFinanceRules {
    fn create_ledger_transaction(
        &self,
        draft: LedgerTransactionDraft,
    ) -> Result<LedgerTransaction, FinanceRuleError> {
        if draft.postings.len() < 2 {
            return Err(FinanceRuleError::InvalidLedgerPostingCount);
        }
        if draft.source.source_id.is_empty() {
            return Err(FinanceRuleError::InvalidCommandId);
        }

        let mut sum = 0_i128;
        for posting in &draft.postings {
            if posting.amount_krw == 0 {
                return Err(FinanceRuleError::ZeroLedgerPosting);
            }
            match (
                posting.account_code.account_requirement(),
                posting.financial_account_id,
            ) {
                (PostingAccountRequirement::Required, None) => {
                    return Err(FinanceRuleError::PostingAccountRequired);
                }
                (PostingAccountRequirement::Forbidden, Some(_)) => {
                    return Err(FinanceRuleError::PostingAccountForbidden);
                }
                (PostingAccountRequirement::Required, Some(_))
                | (PostingAccountRequirement::Forbidden, None) => {}
            }
            sum = sum
                .checked_add(i128::from(posting.amount_krw))
                .ok_or(FinanceRuleError::ArithmeticOverflow)?;
        }
        if sum != 0 {
            return Err(FinanceRuleError::UnbalancedLedger);
        }

        Ok(LedgerTransaction::from_validated(draft))
    }

    fn apply_transfer(&self, input: TransferInput) -> Result<TransferMutation, FinanceRuleError> {
        if input.policy.run != input.account.run {
            return Err(FinanceRuleError::AccountScopeMismatch);
        }
        if input.account.status != FinancialAccountStatus::Open {
            return Err(FinanceRuleError::AccountClosed);
        }
        if input.wallet_cash_krw < 0 || input.account.cash_krw < 0 {
            return Err(FinanceRuleError::InvalidBalance);
        }
        if input.amount_krw <= 0 {
            return Err(FinanceRuleError::InvalidTransferAmount);
        }

        let (wallet_cash_krw, account_cash_krw, wallet_posting, account_posting, description) =
            match input.direction {
                TransferDirection::WalletToAccount => {
                    if input.wallet_cash_krw < input.amount_krw {
                        return Err(FinanceRuleError::InsufficientWalletCash);
                    }
                    let wallet_cash_krw = input
                        .wallet_cash_krw
                        .checked_sub(input.amount_krw)
                        .ok_or(FinanceRuleError::ArithmeticOverflow)?;
                    let account_cash_krw = input
                        .account
                        .cash_krw
                        .checked_add(input.amount_krw)
                        .ok_or(FinanceRuleError::ArithmeticOverflow)?;
                    (
                        wallet_cash_krw,
                        account_cash_krw,
                        -input.amount_krw,
                        input.amount_krw,
                        "지갑에서 금융계좌로 이체",
                    )
                }
                TransferDirection::AccountToWallet => {
                    if input.account.cash_krw < input.amount_krw {
                        return Err(FinanceRuleError::InsufficientAccountCash);
                    }
                    let wallet_cash_krw = input
                        .wallet_cash_krw
                        .checked_add(input.amount_krw)
                        .ok_or(FinanceRuleError::ArithmeticOverflow)?;
                    let account_cash_krw = input
                        .account
                        .cash_krw
                        .checked_sub(input.amount_krw)
                        .ok_or(FinanceRuleError::ArithmeticOverflow)?;
                    (
                        wallet_cash_krw,
                        account_cash_krw,
                        input.amount_krw,
                        -input.amount_krw,
                        "금융계좌에서 지갑으로 이체",
                    )
                }
            };

        let ledger = self.create_ledger_transaction(LedgerTransactionDraft {
            policy: input.policy,
            source: LedgerSource {
                kind: LedgerSourceKind::Transfer,
                source_id: input.command_id.to_string(),
            },
            game_day: input.game_day,
            description: description.to_owned(),
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: wallet_posting,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::AccountCash,
                    financial_account_id: Some(input.account.id),
                    amount_krw: account_posting,
                },
            ],
        })?;

        let mut account = input.account;
        account.cash_krw = account_cash_krw;
        Ok(TransferMutation {
            wallet_cash_krw,
            account,
            ledger,
        })
    }

    fn accrue_daily_interest(
        &self,
        input: DailyInterestInput,
    ) -> Result<DailyInterestAccrual, FinanceRuleError> {
        if input.principal_krw < 0
            || input.annual_rate_bp < 0
            || !(0..i64::try_from(INTEREST_DENOMINATOR)
                .map_err(|_| FinanceRuleError::ArithmeticOverflow)?)
                .contains(&input.remainder)
        {
            return Err(FinanceRuleError::InvalidInterest);
        }

        let numerator = i128::from(input.principal_krw)
            .checked_mul(i128::from(input.annual_rate_bp))
            .and_then(|value| value.checked_add(i128::from(input.remainder)))
            .ok_or(FinanceRuleError::ArithmeticOverflow)?;
        let interest_krw = i64::try_from(numerator / INTEREST_DENOMINATOR)
            .map_err(|_| FinanceRuleError::ArithmeticOverflow)?;
        let remainder = i64::try_from(numerator % INTEREST_DENOMINATOR)
            .map_err(|_| FinanceRuleError::ArithmeticOverflow)?;

        Ok(DailyInterestAccrual {
            interest_krw,
            remainder,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        CommandId, FinancialAccount, FinancialAccountType, LedgerSource, ResourceId, RunId,
        RunPolicyContext,
    };
    use super::*;

    const SAVE_ID: ResourceId = ResourceId::from_u64(11);
    const ACCOUNT_ID: ResourceId = ResourceId::from_u64(17);
    const POLICY_SET_ID: ResourceId = ResourceId::from_u64(3);

    fn given_run() -> RunId {
        RunId {
            save_id: SAVE_ID,
            run_revision: 4,
        }
    }

    fn given_policy() -> RunPolicyContext {
        RunPolicyContext {
            run: given_run(),
            policy_set_id: POLICY_SET_ID,
        }
    }

    fn given_account(cash_krw: i64) -> FinancialAccount {
        FinancialAccount {
            id: ACCOUNT_ID,
            run: given_run(),
            account_type: FinancialAccountType::TaxableBrokerage,
            status: FinancialAccountStatus::Open,
            is_default: true,
            cash_krw,
        }
    }

    fn given_transfer(direction: TransferDirection, amount_krw: i64) -> TransferInput {
        TransferInput {
            policy: given_policy(),
            command_id: CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                .expect("표준 UUID여야 한다"),
            game_day: 8,
            wallet_cash_krw: 1_000,
            account: given_account(500),
            direction,
            amount_krw,
        }
    }

    fn given_ledger(postings: Vec<LedgerPosting>) -> LedgerTransactionDraft {
        LedgerTransactionDraft {
            policy: given_policy(),
            source: LedgerSource {
                kind: LedgerSourceKind::Correction,
                source_id: "correction-1".to_owned(),
            },
            game_day: 8,
            description: "정정".to_owned(),
            postings,
        }
    }

    mod context_a_ledger_transaction_is_created {
        use super::*;

        #[test]
        fn given_balanced_nonzero_postings_when_created_then_the_transaction_is_accepted() {
            let rules = create_finance_rules();
            let draft = given_ledger(vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: 500,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::OpeningEquity,
                    financial_account_id: None,
                    amount_krw: -500,
                },
            ]);

            let ledger = rules
                .create_ledger_transaction(draft)
                .expect("균형 원장을 만들 수 있어야 한다");

            assert_eq!(ledger.postings().len(), 2);
        }

        #[test]
        fn given_i64_extreme_postings_when_summed_then_i128_preserves_the_zero_balance() {
            let rules = create_finance_rules();
            let draft = given_ledger(vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: i64::MAX,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::FeeExpense,
                    financial_account_id: None,
                    amount_krw: 1,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::OpeningEquity,
                    financial_account_id: None,
                    amount_krw: i64::MIN,
                },
            ]);

            let result = rules.create_ledger_transaction(draft);

            assert!(result.is_ok());
        }

        #[test]
        fn given_one_posting_when_created_then_it_is_rejected() {
            let rules = create_finance_rules();
            let draft = given_ledger(vec![LedgerPosting {
                account_code: LedgerAccountCode::Wallet,
                financial_account_id: None,
                amount_krw: 1,
            }]);

            let result = rules.create_ledger_transaction(draft);

            assert_eq!(result, Err(FinanceRuleError::InvalidLedgerPostingCount));
        }

        #[test]
        fn given_a_zero_posting_when_created_then_it_is_rejected() {
            let rules = create_finance_rules();
            let draft = given_ledger(vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: 0,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::OpeningEquity,
                    financial_account_id: None,
                    amount_krw: 1,
                },
            ]);

            let result = rules.create_ledger_transaction(draft);

            assert_eq!(result, Err(FinanceRuleError::ZeroLedgerPosting));
        }

        #[test]
        fn given_an_account_code_without_an_account_when_created_then_it_is_rejected() {
            let rules = create_finance_rules();
            let draft = given_ledger(vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::AccountCash,
                    financial_account_id: None,
                    amount_krw: 10,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::OpeningEquity,
                    financial_account_id: None,
                    amount_krw: -10,
                },
            ]);

            let result = rules.create_ledger_transaction(draft);

            assert_eq!(result, Err(FinanceRuleError::PostingAccountRequired));
        }

        #[test]
        fn given_a_wallet_code_with_an_account_when_created_then_it_is_rejected() {
            let rules = create_finance_rules();
            let draft = given_ledger(vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: Some(ACCOUNT_ID),
                    amount_krw: 10,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::OpeningEquity,
                    financial_account_id: None,
                    amount_krw: -10,
                },
            ]);

            let result = rules.create_ledger_transaction(draft);

            assert_eq!(result, Err(FinanceRuleError::PostingAccountForbidden));
        }

        #[test]
        fn given_nonzero_postings_with_a_nonzero_sum_when_created_then_they_are_rejected() {
            let rules = create_finance_rules();
            let draft = given_ledger(vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::Wallet,
                    financial_account_id: None,
                    amount_krw: 10,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::OpeningEquity,
                    financial_account_id: None,
                    amount_krw: -9,
                },
            ]);

            let result = rules.create_ledger_transaction(draft);

            assert_eq!(result, Err(FinanceRuleError::UnbalancedLedger));
        }
    }

    mod context_cash_is_transferred_between_wallet_and_account {
        use super::*;

        #[test]
        fn given_enough_wallet_cash_when_transferred_then_both_balances_and_ledger_move_together() {
            let rules = create_finance_rules();
            let input = given_transfer(TransferDirection::WalletToAccount, 300);

            let mutation = rules.apply_transfer(input).expect("이체할 수 있어야 한다");

            assert_eq!(mutation.wallet_cash_krw, 700);
            assert_eq!(mutation.account.cash_krw, 800);
            assert_eq!(mutation.ledger.postings()[0].amount_krw, -300);
            assert_eq!(mutation.ledger.postings()[1].amount_krw, 300);
        }

        #[test]
        fn given_enough_account_cash_when_withdrawn_then_both_balances_and_ledger_move_together() {
            let rules = create_finance_rules();
            let input = given_transfer(TransferDirection::AccountToWallet, 300);

            let mutation = rules.apply_transfer(input).expect("출금할 수 있어야 한다");

            assert_eq!(mutation.wallet_cash_krw, 1_300);
            assert_eq!(mutation.account.cash_krw, 200);
            assert_eq!(mutation.ledger.postings()[0].amount_krw, 300);
            assert_eq!(mutation.ledger.postings()[1].amount_krw, -300);
        }

        #[test]
        fn given_insufficient_wallet_cash_when_depositing_then_it_is_rejected() {
            let rules = create_finance_rules();
            let input = given_transfer(TransferDirection::WalletToAccount, 1_001);

            let result = rules.apply_transfer(input);

            assert_eq!(result, Err(FinanceRuleError::InsufficientWalletCash));
        }

        #[test]
        fn given_insufficient_account_cash_when_withdrawing_then_it_is_rejected() {
            let rules = create_finance_rules();
            let input = given_transfer(TransferDirection::AccountToWallet, 501);

            let result = rules.apply_transfer(input);

            assert_eq!(result, Err(FinanceRuleError::InsufficientAccountCash));
        }

        #[test]
        fn given_a_closed_account_when_transferred_then_it_is_rejected() {
            let rules = create_finance_rules();
            let mut input = given_transfer(TransferDirection::WalletToAccount, 1);
            input.account.status = FinancialAccountStatus::Closed;

            let result = rules.apply_transfer(input);

            assert_eq!(result, Err(FinanceRuleError::AccountClosed));
        }

        #[test]
        fn given_an_invalid_stored_balance_when_transferred_then_it_is_rejected() {
            let rules = create_finance_rules();
            let mut input = given_transfer(TransferDirection::WalletToAccount, 1);
            input.account.cash_krw = -1;

            let result = rules.apply_transfer(input);

            assert_eq!(result, Err(FinanceRuleError::InvalidBalance));
        }

        #[test]
        fn given_an_account_from_another_run_when_transferred_then_it_is_rejected() {
            let rules = create_finance_rules();
            let mut input = given_transfer(TransferDirection::WalletToAccount, 1);
            input.account.run.run_revision = 3;

            let result = rules.apply_transfer(input);

            assert_eq!(result, Err(FinanceRuleError::AccountScopeMismatch));
        }

        #[test]
        fn given_an_overflowing_destination_balance_when_transferred_then_it_is_rejected() {
            let rules = create_finance_rules();
            let mut input = given_transfer(TransferDirection::WalletToAccount, 1);
            input.account.cash_krw = i64::MAX;

            let result = rules.apply_transfer(input);

            assert_eq!(result, Err(FinanceRuleError::ArithmeticOverflow));
        }
    }

    mod context_daily_interest_is_accrued {
        use super::*;

        #[test]
        fn given_principal_rate_and_remainder_when_accrued_then_quotient_and_remainder_are_exact() {
            let rules = create_finance_rules();
            let input = DailyInterestInput {
                principal_krw: 1_000_000,
                annual_rate_bp: 250,
                remainder: 0,
            };

            let accrual = rules
                .accrue_daily_interest(input)
                .expect("일 이자를 계산할 수 있어야 한다");

            assert_eq!(accrual.interest_krw, 68);
            assert_eq!(accrual.remainder, 1_800_000);
        }

        #[test]
        fn given_a_carried_fraction_when_it_completes_one_won_then_one_won_is_paid() {
            let rules = create_finance_rules();
            let input = DailyInterestInput {
                principal_krw: 1,
                annual_rate_bp: 1,
                remainder: 3_649_999,
            };

            let accrual = rules
                .accrue_daily_interest(input)
                .expect("잔여분을 이월할 수 있어야 한다");

            assert_eq!(accrual.interest_krw, 1);
            assert_eq!(accrual.remainder, 0);
        }

        #[test]
        fn given_a_negative_rate_when_accrued_then_it_is_rejected() {
            let rules = create_finance_rules();
            let input = DailyInterestInput {
                principal_krw: 1,
                annual_rate_bp: -1,
                remainder: 0,
            };

            let result = rules.accrue_daily_interest(input);

            assert_eq!(result, Err(FinanceRuleError::InvalidInterest));
        }

        #[test]
        fn given_a_remainder_at_the_denominator_when_accrued_then_it_is_rejected() {
            let rules = create_finance_rules();
            let input = DailyInterestInput {
                principal_krw: 1,
                annual_rate_bp: 1,
                remainder: 3_650_000,
            };

            let result = rules.accrue_daily_interest(input);

            assert_eq!(result, Err(FinanceRuleError::InvalidInterest));
        }

        #[test]
        fn given_an_interest_result_beyond_i64_when_accrued_then_it_is_rejected() {
            let rules = create_finance_rules();
            let input = DailyInterestInput {
                principal_krw: i64::MAX,
                annual_rate_bp: i32::MAX,
                remainder: 0,
            };

            let result = rules.accrue_daily_interest(input);

            assert_eq!(result, Err(FinanceRuleError::ArithmeticOverflow));
        }
    }
}
