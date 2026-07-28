use std::collections::BTreeSet;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::loan::create_loan_rules;
use super::types::*;

const COMPOSITION_HASH_DOMAIN: &[u8] = b"lifeledger.life.insolvency-composition.v1";

struct CashOnlyLiquidationRules {
    policy: InsolvencyPolicyTerms,
    loan_rules: Arc<dyn LoanRules>,
}

/// Creates the M4-E1 cash-only liquidation rules from §8.
pub fn create_insolvency_rules() -> Arc<dyn InsolvencyRules> {
    create_insolvency_rules_with_loan_rules(create_loan_rules())
}

/// Creates the M4-E1 rules with an injectable loan allocation authority.
pub fn create_insolvency_rules_with_loan_rules(
    loan_rules: Arc<dyn LoanRules>,
) -> Arc<dyn InsolvencyRules> {
    Arc::new(CashOnlyLiquidationRules {
        policy: InsolvencyPolicyTerms {
            automatic_cash_protection_krw: INSOLVENCY_AUTOMATIC_CASH_PROTECTION_KRW,
            standard_median_income_krw: INSOLVENCY_STANDARD_MEDIAN_INCOME_KRW,
            living_expense_ratio_ppm: INSOLVENCY_LIVING_EXPENSE_RATIO_PPM,
            living_expense_months: INSOLVENCY_LIVING_EXPENSE_MONTHS,
            credit_restriction_game_days: INSOLVENCY_CREDIT_RESTRICTION_GAME_DAYS,
        },
        loan_rules,
    })
}

impl InsolvencyRules for CashOnlyLiquidationRules {
    fn policy_terms(&self) -> InsolvencyPolicyTerms {
        self.policy
    }

    fn assess_eligibility(
        &self,
        input: InsolvencyEligibilityInput<'_>,
    ) -> Result<InsolvencyEligibilityAssessment, InsolvencyError> {
        assess_eligibility(input)
    }

    fn calculate_cash_protection(
        &self,
        input: InsolvencyCashProtectionInput,
    ) -> Result<InsolvencyCashProtection, InsolvencyError> {
        calculate_cash_protection(input)
    }

    fn allocate_distribution(
        &self,
        liquidatable_krw: i64,
        claims: &[InsolvencyDistributionClaimInput<'_>],
    ) -> Result<InsolvencyDistributionPlan, InsolvencyError> {
        allocate_distribution(self.loan_rules.as_ref(), liquidatable_krw, claims)
    }

    fn composition_sha256(
        &self,
        input: InsolvencyCompositionInput<'_>,
    ) -> Result<String, InsolvencyError> {
        composition_sha256(input)
    }

    fn plan_submit(
        &self,
        current_status: InsolvencyCaseStatus,
        submitted_game_day: u32,
    ) -> Result<InsolvencySubmitPlan, InsolvencyError> {
        plan_submit(self.policy, current_status, submitted_game_day)
    }

    fn plan_withdraw(
        &self,
        current_status: InsolvencyCaseStatus,
        game_day: u32,
    ) -> Result<InsolvencyCaseTransition, InsolvencyError> {
        if current_status != InsolvencyCaseStatus::Prepared {
            return Err(InsolvencyError::InvalidCaseTransition);
        }
        Ok(InsolvencyCaseTransition {
            sequence: 1,
            from: InsolvencyCaseStatus::Prepared,
            to: InsolvencyCaseStatus::Withdrawn,
            game_day,
        })
    }

    fn is_credit_restricted(
        &self,
        status: InsolvencyCaseStatus,
        current_game_day: u32,
        end_exclusive_game_day: Option<u32>,
    ) -> Result<bool, InsolvencyError> {
        is_credit_restricted(status, current_game_day, end_exclusive_game_day)
    }

    fn recovery_status(
        &self,
        status: InsolvencyCaseStatus,
        current_game_day: u32,
        end_exclusive_game_day: Option<u32>,
    ) -> Result<InsolvencyCaseStatus, InsolvencyError> {
        let restricted = is_credit_restricted(status, current_game_day, end_exclusive_game_day)?;
        if status == InsolvencyCaseStatus::Rebuilding && !restricted {
            Ok(InsolvencyCaseStatus::Recovered)
        } else {
            Ok(status)
        }
    }
}

fn assess_eligibility(
    input: InsolvencyEligibilityInput<'_>,
) -> Result<InsolvencyEligibilityAssessment, InsolvencyError> {
    let mut reasons = Vec::new();
    if !input.policy_available {
        reasons.push(InsolvencyEligibilityReason::PolicyUnavailable);
    }
    if !input.component_available {
        reasons.push(InsolvencyEligibilityReason::ComponentUnavailable);
    }
    if input.wallet_cash_krw < 0 {
        reasons.push(InsolvencyEligibilityReason::InvalidWalletCash);
    }
    if input.unsupported_asset_position_count > 0 || input.has_secured_interest {
        reasons.push(InsolvencyEligibilityReason::UnsupportedAssetComposition);
    }
    if input.unsupported_non_loan_obligation_count > 0 {
        reasons.push(InsolvencyEligibilityReason::UnsupportedNonLoanObligation);
    }
    if input.has_non_terminal_case {
        reasons.push(InsolvencyEligibilityReason::ExistingNonTerminalCase);
    }

    let mut seen = BTreeSet::new();
    let mut total_supported_claim_krw = 0_i64;
    let mut supported_claim_count = 0_u32;
    let mut has_unsupported_loan = false;
    for loan in input.loans {
        if !seen.insert(loan.contract_id) {
            return Err(InsolvencyError::DuplicateLoan(loan.contract_id));
        }
        let allowed = claim_allowed(
            loan.contract_id,
            loan.remaining_principal_krw,
            loan.accrued_interest_krw,
            loan.accrued_fee_krw,
        )?;
        if allowed == 0 {
            continue;
        }
        let supported = !loan.read_only
            && loan.status == LoanContractStatus::Defaulted
            && matches!(
                loan.product_kind,
                LoanProductKind::StudentLoan | LoanProductKind::UnsecuredLoan
            );
        if supported {
            total_supported_claim_krw = total_supported_claim_krw
                .checked_add(allowed)
                .ok_or(InsolvencyError::ArithmeticOverflow)?;
            supported_claim_count = supported_claim_count
                .checked_add(1)
                .ok_or(InsolvencyError::ArithmeticOverflow)?;
        } else {
            has_unsupported_loan = true;
        }
    }

    if has_unsupported_loan {
        reasons.push(InsolvencyEligibilityReason::UnsupportedLoanComposition);
    }
    if supported_claim_count == 0 {
        reasons.push(InsolvencyEligibilityReason::NoSupportedDefaultedDebt);
    }
    if input.wallet_cash_krw >= 0
        && supported_claim_count > 0
        && total_supported_claim_krw <= input.wallet_cash_krw
    {
        reasons.push(InsolvencyEligibilityReason::DebtNotGreaterThanCash);
    }
    reasons.sort_unstable();
    reasons.dedup();

    let unavailable = reasons.iter().any(|reason| {
        matches!(
            reason,
            InsolvencyEligibilityReason::PolicyUnavailable
                | InsolvencyEligibilityReason::ComponentUnavailable
        )
    });
    let unsupported = reasons.iter().any(|reason| {
        matches!(
            reason,
            InsolvencyEligibilityReason::UnsupportedLoanComposition
                | InsolvencyEligibilityReason::UnsupportedAssetComposition
                | InsolvencyEligibilityReason::UnsupportedNonLoanObligation
        )
    });
    let status = if reasons.is_empty() {
        InsolvencyEligibilityStatus::Eligible
    } else if unavailable {
        InsolvencyEligibilityStatus::Unavailable
    } else if unsupported {
        InsolvencyEligibilityStatus::CompositionUnsupported
    } else {
        InsolvencyEligibilityStatus::Ineligible
    };

    Ok(InsolvencyEligibilityAssessment {
        status,
        reasons,
        supported_claim_count,
        total_supported_claim_krw,
    })
}

fn calculate_cash_protection(
    input: InsolvencyCashProtectionInput,
) -> Result<InsolvencyCashProtection, InsolvencyError> {
    if input.wallet_cash_krw < 0 {
        return Err(InsolvencyError::InvalidWalletCash);
    }
    validate_policy(input.policy)?;

    let additional_protection_cap_krw = i64::try_from(
        i128::from(input.policy.standard_median_income_krw)
            .checked_mul(i128::from(input.policy.living_expense_ratio_ppm))
            .and_then(|value| value.checked_mul(i128::from(input.policy.living_expense_months)))
            .ok_or(InsolvencyError::ArithmeticOverflow)?
            / i128::from(INSOLVENCY_RATIO_SCALE_PPM),
    )
    .map_err(|_| InsolvencyError::ArithmeticOverflow)?;
    let automatic_protected_krw = input
        .wallet_cash_krw
        .min(input.policy.automatic_cash_protection_krw);
    let cash_after_automatic = input
        .wallet_cash_krw
        .checked_sub(automatic_protected_krw)
        .ok_or(InsolvencyError::ArithmeticOverflow)?;
    let additional_protected_krw = cash_after_automatic.min(additional_protection_cap_krw);
    let liquidatable_krw = cash_after_automatic
        .checked_sub(additional_protected_krw)
        .ok_or(InsolvencyError::ArithmeticOverflow)?;

    Ok(InsolvencyCashProtection {
        wallet_cash_krw: input.wallet_cash_krw,
        automatic_protected_krw,
        additional_protection_cap_krw,
        additional_protected_krw,
        liquidatable_krw,
    })
}

fn validate_policy(policy: InsolvencyPolicyTerms) -> Result<(), InsolvencyError> {
    if policy.automatic_cash_protection_krw < 0
        || policy.standard_median_income_krw < 0
        || !(0..=INSOLVENCY_RATIO_SCALE_PPM).contains(&policy.living_expense_ratio_ppm)
        || policy.living_expense_months == 0
        || policy.credit_restriction_game_days == 0
    {
        return Err(InsolvencyError::InvalidPolicy);
    }
    Ok(())
}

fn allocate_distribution(
    loan_rules: &dyn LoanRules,
    liquidatable_krw: i64,
    claims: &[InsolvencyDistributionClaimInput<'_>],
) -> Result<InsolvencyDistributionPlan, InsolvencyError> {
    if liquidatable_krw < 0 {
        return Err(InsolvencyError::InvalidLiquidatableCash);
    }
    let mut ordered = claims.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|claim| claim.contract_id);
    let mut seen = BTreeSet::new();
    let mut total_claim_krw = 0_i64;
    let mut allowed = Vec::with_capacity(ordered.len());
    for claim in &ordered {
        if !seen.insert(claim.contract_id) {
            return Err(InsolvencyError::DuplicateClaim(claim.contract_id));
        }
        let amount = claim_allowed(
            claim.contract_id,
            claim.principal_krw,
            claim.interest_krw,
            claim.fee_krw,
        )?;
        if amount == 0
            || bucket_totals(claim)? != (claim.principal_krw, claim.interest_krw, claim.fee_krw)
        {
            return Err(InsolvencyError::InvalidClaim(claim.contract_id));
        }
        total_claim_krw = total_claim_krw
            .checked_add(amount)
            .ok_or(InsolvencyError::ArithmeticOverflow)?;
        allowed.push(amount);
    }
    if total_claim_krw == 0 || liquidatable_krw > total_claim_krw {
        return Err(InsolvencyError::InvalidLiquidatableCash);
    }

    let mut shares = allowed
        .iter()
        .map(|amount| {
            i64::try_from(
                i128::from(liquidatable_krw)
                    .checked_mul(i128::from(*amount))
                    .ok_or(InsolvencyError::ArithmeticOverflow)?
                    / i128::from(total_claim_krw),
            )
            .map_err(|_| InsolvencyError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let allocated_floor = shares.iter().try_fold(0_i64, |sum, value| {
        sum.checked_add(*value)
            .ok_or(InsolvencyError::ArithmeticOverflow)
    })?;
    let remainder = liquidatable_krw
        .checked_sub(allocated_floor)
        .ok_or(InsolvencyError::ArithmeticOverflow)?;
    let remainder_count =
        usize::try_from(remainder).map_err(|_| InsolvencyError::ArithmeticOverflow)?;
    if remainder_count > shares.len() {
        return Err(InsolvencyError::ArithmeticOverflow);
    }
    for share in shares.iter_mut().take(remainder_count) {
        *share = share
            .checked_add(1)
            .ok_or(InsolvencyError::ArithmeticOverflow)?;
    }

    let mut results = Vec::with_capacity(ordered.len());
    let mut total_discharged_krw = 0_i64;
    for ((claim, original_claim_krw), distributed_krw) in
        ordered.into_iter().zip(allowed).zip(shares)
    {
        let repayment = loan_rules
            .allocate_repayment(RepaymentAllocationInput {
                wallet_cash_krw: distributed_krw,
                buckets: claim.buckets,
            })
            .map_err(|_| InsolvencyError::LoanAllocationFailed)?;
        if repayment.wallet_cash_after_krw != 0 {
            return Err(InsolvencyError::LoanAllocationFailed);
        }
        let discharged_krw = original_claim_krw
            .checked_sub(distributed_krw)
            .ok_or(InsolvencyError::ArithmeticOverflow)?;
        total_discharged_krw = total_discharged_krw
            .checked_add(discharged_krw)
            .ok_or(InsolvencyError::ArithmeticOverflow)?;
        results.push(InsolvencyClaimDistribution {
            contract_id: claim.contract_id,
            original_claim_krw,
            distributed_krw,
            discharged_krw,
            repayment,
        });
    }

    Ok(InsolvencyDistributionPlan {
        liquidatable_krw,
        total_claim_krw,
        total_distributed_krw: liquidatable_krw,
        total_discharged_krw,
        claims: results,
    })
}

fn bucket_totals(
    claim: &InsolvencyDistributionClaimInput<'_>,
) -> Result<(i64, i64, i64), InsolvencyError> {
    let mut principal = 0_i64;
    let mut interest = 0_i64;
    let mut fee = 0_i64;
    for bucket in claim.buckets {
        if bucket.due_krw < 0 {
            return Err(InsolvencyError::InvalidClaim(claim.contract_id));
        }
        let target = match bucket.kind {
            RepaymentBucketKind::OverduePrincipal | RepaymentBucketKind::CurrentPrincipal => {
                &mut principal
            }
            RepaymentBucketKind::OverdueInterest | RepaymentBucketKind::CurrentInterest => {
                &mut interest
            }
            RepaymentBucketKind::OverdueFee | RepaymentBucketKind::CurrentFee => &mut fee,
        };
        *target = target
            .checked_add(bucket.due_krw)
            .ok_or(InsolvencyError::ArithmeticOverflow)?;
    }
    Ok((principal, interest, fee))
}

fn claim_allowed(
    contract_id: crate::finance::ResourceId,
    principal_krw: i64,
    interest_krw: i64,
    fee_krw: i64,
) -> Result<i64, InsolvencyError> {
    if principal_krw < 0 || interest_krw < 0 || fee_krw < 0 {
        return Err(InsolvencyError::InvalidLoan(contract_id));
    }
    principal_krw
        .checked_add(interest_krw)
        .and_then(|value| value.checked_add(fee_krw))
        .ok_or(InsolvencyError::ArithmeticOverflow)
}

fn composition_sha256(input: InsolvencyCompositionInput<'_>) -> Result<String, InsolvencyError> {
    if input.wallet_cash_krw < 0 {
        return Err(InsolvencyError::InvalidWalletCash);
    }
    let mut claims = input.claims.iter().collect::<Vec<_>>();
    claims.sort_by_key(|claim| claim.contract_id);
    let mut claim_ids = BTreeSet::new();
    let mut facts = input.facts.iter().collect::<Vec<_>>();
    facts.sort_by_key(|fact| fact.authority_key);
    let mut fact_keys = BTreeSet::new();

    let mut hasher = Sha256::new();
    update_bytes(&mut hasher, COMPOSITION_HASH_DOMAIN);
    hasher.update(input.wallet_cash_krw.to_be_bytes());
    hasher.update((claims.len() as u64).to_be_bytes());
    for claim in claims {
        if !claim_ids.insert(claim.contract_id) {
            return Err(InsolvencyError::DuplicateLoan(claim.contract_id));
        }
        claim_allowed(
            claim.contract_id,
            claim.remaining_principal_krw,
            claim.accrued_interest_krw,
            claim.accrued_fee_krw,
        )?;
        hasher.update(claim.contract_id.get().to_be_bytes());
        hasher.update([loan_product_kind_code(claim.product_kind)]);
        hasher.update([loan_status_code(claim.status)]);
        hasher.update([u8::from(claim.read_only)]);
        hasher.update(claim.remaining_principal_krw.to_be_bytes());
        hasher.update(claim.accrued_interest_krw.to_be_bytes());
        hasher.update(claim.accrued_fee_krw.to_be_bytes());
    }
    hasher.update((facts.len() as u64).to_be_bytes());
    for fact in facts {
        if fact.authority_key.is_empty() || fact.canonical_value.is_empty() {
            return Err(InsolvencyError::InvalidCompositionFact);
        }
        if !fact_keys.insert(fact.authority_key) {
            return Err(InsolvencyError::DuplicateCompositionFact);
        }
        update_bytes(&mut hasher, fact.authority_key.as_bytes());
        update_bytes(&mut hasher, fact.canonical_value.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn update_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn loan_product_kind_code(kind: LoanProductKind) -> u8 {
    match kind {
        LoanProductKind::StudentLoan => 1,
        LoanProductKind::UnsecuredLoan => 2,
        LoanProductKind::LeaseDepositLoan => 3,
        LoanProductKind::Mortgage => 4,
        LoanProductKind::LegacyDebt => 5,
    }
}

fn loan_status_code(status: LoanContractStatus) -> u8 {
    match status {
        LoanContractStatus::Pending => 1,
        LoanContractStatus::Active => 2,
        LoanContractStatus::Delinquent => 3,
        LoanContractStatus::Defaulted => 4,
        LoanContractStatus::PaidOff => 5,
        LoanContractStatus::Restructured => 6,
        LoanContractStatus::Discharged => 7,
        LoanContractStatus::ChargedOff => 8,
        LoanContractStatus::Cancelled => 9,
    }
}

fn plan_submit(
    policy: InsolvencyPolicyTerms,
    current_status: InsolvencyCaseStatus,
    submitted_game_day: u32,
) -> Result<InsolvencySubmitPlan, InsolvencyError> {
    validate_policy(policy)?;
    if current_status != InsolvencyCaseStatus::Prepared {
        return Err(InsolvencyError::InvalidCaseTransition);
    }
    let credit_restriction_end_exclusive = submitted_game_day
        .checked_add(policy.credit_restriction_game_days)
        .ok_or(InsolvencyError::ArithmeticOverflow)?;
    let statuses = [
        InsolvencyCaseStatus::Prepared,
        InsolvencyCaseStatus::Filed,
        InsolvencyCaseStatus::Liquidation,
        InsolvencyCaseStatus::Discharged,
        InsolvencyCaseStatus::Rebuilding,
    ];
    let transitions = statuses
        .windows(2)
        .enumerate()
        .map(|(index, pair)| InsolvencyCaseTransition {
            sequence: (index + 1) as u8,
            from: pair[0],
            to: pair[1],
            game_day: submitted_game_day,
        })
        .collect();
    Ok(InsolvencySubmitPlan {
        transitions,
        current_status: InsolvencyCaseStatus::Rebuilding,
        submitted_game_day,
        credit_restriction_end_exclusive,
    })
}

fn is_credit_restricted(
    status: InsolvencyCaseStatus,
    current_game_day: u32,
    end_exclusive_game_day: Option<u32>,
) -> Result<bool, InsolvencyError> {
    if status != InsolvencyCaseStatus::Rebuilding {
        return Ok(false);
    }
    let end_exclusive = end_exclusive_game_day.ok_or(InsolvencyError::InvalidCreditRestriction)?;
    Ok(current_game_day < end_exclusive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::ResourceId;

    fn given_rules() -> Arc<dyn InsolvencyRules> {
        create_insolvency_rules()
    }

    fn given_loan(
        id: u64,
        product_kind: LoanProductKind,
        principal_krw: i64,
    ) -> InsolvencyLoanPosition {
        InsolvencyLoanPosition {
            contract_id: ResourceId::from_u64(id),
            product_kind,
            status: LoanContractStatus::Defaulted,
            read_only: false,
            remaining_principal_krw: principal_krw,
            accrued_interest_krw: 0,
            accrued_fee_krw: 0,
        }
    }

    mod context_보호현금을_계산하는_경우 {
        use super::*;

        #[test]
        fn given_기준정책_when_상한을계산하면_then_전체곱을마지막에내린다() {
            let policy = given_rules().policy_terms();

            let result = given_rules()
                .calculate_cash_protection(InsolvencyCashProtectionInput {
                    wallet_cash_krw: 18_087_372,
                    policy,
                })
                .expect("보호 현금을 계산해야 한다");

            assert_eq!(
                (
                    result.automatic_protected_krw,
                    result.additional_protection_cap_krw,
                    result.additional_protected_krw,
                    result.liquidatable_krw,
                ),
                (2_500_000, 15_587_371, 15_587_371, 1)
            );
        }

        #[test]
        fn given_자동보호상한과같은지갑_when_계산하면_then_추가보호와청산액은0이다() {
            let result = given_rules()
                .calculate_cash_protection(InsolvencyCashProtectionInput {
                    wallet_cash_krw: 2_500_000,
                    policy: given_rules().policy_terms(),
                })
                .expect("exclusive 경계를 계산해야 한다");

            assert_eq!(
                (result.additional_protected_krw, result.liquidatable_krw),
                (0, 0)
            );
        }
    }

    mod context_현금청산_자격을_판정하는_경우 {
        use super::*;

        #[test]
        fn given_학자금과무담보default채무_when_판정하면_then_eligible이다() {
            let loans = [
                given_loan(1, LoanProductKind::StudentLoan, 100),
                given_loan(2, LoanProductKind::UnsecuredLoan, 200),
            ];

            let result = given_rules()
                .assess_eligibility(InsolvencyEligibilityInput {
                    policy_available: true,
                    component_available: true,
                    wallet_cash_krw: 10,
                    loans: &loans,
                    unsupported_asset_position_count: 0,
                    unsupported_non_loan_obligation_count: 0,
                    has_secured_interest: false,
                    has_non_terminal_case: false,
                })
                .expect("지원 채무를 판정해야 한다");

            assert_eq!(result.status, InsolvencyEligibilityStatus::Eligible);
            assert_eq!(result.total_supported_claim_krw, 300);
        }

        #[test]
        fn given_mortgage와비대출의무_when_판정하면_then_unsupported로닫는다() {
            let loans = [given_loan(1, LoanProductKind::Mortgage, 100)];

            let result = given_rules()
                .assess_eligibility(InsolvencyEligibilityInput {
                    policy_available: true,
                    component_available: true,
                    wallet_cash_krw: 0,
                    loans: &loans,
                    unsupported_asset_position_count: 0,
                    unsupported_non_loan_obligation_count: 1,
                    has_secured_interest: true,
                    has_non_terminal_case: false,
                })
                .expect("지원하지 않는 구성을 분류해야 한다");

            assert_eq!(
                result.status,
                InsolvencyEligibilityStatus::CompositionUnsupported
            );
            assert!(
                result
                    .reasons
                    .contains(&InsolvencyEligibilityReason::UnsupportedLoanComposition)
            );
            assert!(
                result
                    .reasons
                    .contains(&InsolvencyEligibilityReason::UnsupportedNonLoanObligation)
            );
        }
    }

    mod context_claim을_비례배분하는_경우 {
        use super::*;

        #[test]
        fn given_2대1claim과5원_when_배분하면_then_잔여1원은작은계약id가받는다() {
            let first_buckets = [RepaymentBucketBalance {
                kind: RepaymentBucketKind::OverduePrincipal,
                due_krw: 2,
            }];
            let second_buckets = [RepaymentBucketBalance {
                kind: RepaymentBucketKind::OverduePrincipal,
                due_krw: 4,
            }];
            let claims = [
                InsolvencyDistributionClaimInput {
                    contract_id: ResourceId::from_u64(20),
                    principal_krw: 4,
                    interest_krw: 0,
                    fee_krw: 0,
                    buckets: &second_buckets,
                },
                InsolvencyDistributionClaimInput {
                    contract_id: ResourceId::from_u64(10),
                    principal_krw: 2,
                    interest_krw: 0,
                    fee_krw: 0,
                    buckets: &first_buckets,
                },
            ];

            let result = given_rules()
                .allocate_distribution(5, &claims)
                .expect("계약 ID 순으로 잔여 원을 배분해야 한다");

            assert_eq!(
                result
                    .claims
                    .iter()
                    .map(|claim| (claim.contract_id.get(), claim.distributed_krw))
                    .collect::<Vec<_>>(),
                vec![(10, 2), (20, 3)]
            );
            assert_eq!(
                result.total_claim_krw,
                result.total_distributed_krw + result.total_discharged_krw
            );
        }

        #[test]
        fn given_0원청산액_when_배분하면_then_payment몫은모두0이다() {
            let buckets = [RepaymentBucketBalance {
                kind: RepaymentBucketKind::OverduePrincipal,
                due_krw: 100,
            }];
            let claims = [InsolvencyDistributionClaimInput {
                contract_id: ResourceId::from_u64(1),
                principal_krw: 100,
                interest_krw: 0,
                fee_krw: 0,
                buckets: &buckets,
            }];

            let result = given_rules()
                .allocate_distribution(0, &claims)
                .expect("0원 청산을 보존해야 한다");

            assert_eq!(
                (result.total_distributed_krw, result.total_discharged_krw),
                (0, 100)
            );
        }
    }

    mod context_case를_제출하고_회복하는_경우 {
        use super::*;

        #[test]
        fn given_prepared_case_when_제출하면_then_같은날rebuilding까지전이한다() {
            let result = given_rules()
                .plan_submit(InsolvencyCaseStatus::Prepared, 10)
                .expect("같은 날 제출 전이를 계획해야 한다");

            assert_eq!(result.transitions.len(), 4);
            assert!(result.transitions.iter().all(|item| item.game_day == 10));
            assert_eq!(result.current_status, InsolvencyCaseStatus::Rebuilding);
            assert_eq!(result.credit_restriction_end_exclusive, 1_835);
        }

        #[test]
        fn given_end_exclusive_when_제한을판정하면_then_바로그날recovered이다() {
            let rules = given_rules();

            let restricted = rules
                .is_credit_restricted(InsolvencyCaseStatus::Rebuilding, 1_834, Some(1_835))
                .expect("마지막 제한일을 판정해야 한다");
            let recovered = rules
                .recovery_status(InsolvencyCaseStatus::Rebuilding, 1_835, Some(1_835))
                .expect("exclusive 종료일에 회복해야 한다");

            assert!(restricted);
            assert_eq!(recovered, InsolvencyCaseStatus::Recovered);
        }
    }

    mod context_구성hash를_계산하는_경우 {
        use super::*;

        #[test]
        fn given_같은권위집합_when_순서만바꾸면_then_hash는같다() {
            let claims = [
                given_loan(2, LoanProductKind::UnsecuredLoan, 20),
                given_loan(1, LoanProductKind::StudentLoan, 10),
            ];
            let facts = [
                InsolvencyCompositionFact {
                    authority_key: "tax",
                    canonical_value: "none",
                },
                InsolvencyCompositionFact {
                    authority_key: "assets",
                    canonical_value: "walletOnly",
                },
            ];

            let first = given_rules()
                .composition_sha256(InsolvencyCompositionInput {
                    wallet_cash_krw: 1,
                    claims: &claims,
                    facts: &facts,
                })
                .expect("구성 hash를 계산해야 한다");
            let second = given_rules()
                .composition_sha256(InsolvencyCompositionInput {
                    wallet_cash_krw: 1,
                    claims: &[claims[1], claims[0]],
                    facts: &[facts[1], facts[0]],
                })
                .expect("canonical 순서로 hash해야 한다");

            assert_eq!(first, second);
        }

        #[test]
        fn given_저장후지갑변경_when_hash하면_then_구성이달라진다() {
            let claims = [given_loan(1, LoanProductKind::UnsecuredLoan, 10)];

            let first = given_rules()
                .composition_sha256(InsolvencyCompositionInput {
                    wallet_cash_krw: 1,
                    claims: &claims,
                    facts: &[],
                })
                .expect("첫 hash를 계산해야 한다");
            let second = given_rules()
                .composition_sha256(InsolvencyCompositionInput {
                    wallet_cash_krw: 2,
                    claims: &claims,
                    facts: &[],
                })
                .expect("변경된 hash를 계산해야 한다");

            assert_ne!(first, second);
        }
    }
}
