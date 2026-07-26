use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use time::{Date, Duration, Month};

use super::types::{
    DailyInterestInput, FinanceRuleError, FinanceRules, FinancialAccountType, LedgerAccountCode,
    LedgerPosting, LedgerSource, LedgerSourceKind, LedgerTransaction, LedgerTransactionDraft,
    ResourceId, RunPolicyContext,
};

pub const CASH_SETTLEMENT_PAYLOAD_VERSION: u8 = 1;

const RATE_SCALE_PPM: i128 = 1_000_000;
const BASIS_POINT_SCALE: i128 = 10_000;
const ACTUAL_365_DAYS: i128 = 365;
const DAILY_INTEREST_DENOMINATOR: i64 = 365 * 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashProductError {
    InvalidPayload,
    UnsupportedPayloadVersion,
    InvalidMoney,
    InvalidRate,
    InvalidGameDay,
    InvalidSchedule,
    InvalidInstallment,
    DuplicateInstallment,
    InvalidInstitutionId,
    ArithmeticOverflow,
    DuplicateSettlement,
    FutureSettlement,
    AccountNotFound,
    AccountTypeNotAllowed,
    InvalidSettlementExecution,
    FinanceRule(FinanceRuleError),
}

impl Display for CashProductError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidPayload => "cash settlement payload is invalid",
            Self::UnsupportedPayloadVersion => "cash settlement payload version is unsupported",
            Self::InvalidMoney => "cash product money must satisfy its non-negative constraints",
            Self::InvalidRate => "cash product rate is invalid",
            Self::InvalidGameDay => "cash product game-day range is invalid",
            Self::InvalidSchedule => "cash product calendar schedule is invalid",
            Self::InvalidInstallment => "savings installment is invalid",
            Self::DuplicateInstallment => "savings installment number is duplicated",
            Self::InvalidInstitutionId => "deposit-protection institution ID is invalid",
            Self::ArithmeticOverflow => "cash product arithmetic overflowed",
            Self::DuplicateSettlement => "cash settlement ID is duplicated",
            Self::FutureSettlement => "a future cash settlement cannot be planned today",
            Self::AccountNotFound => "cash settlement account is missing from the daily state",
            Self::AccountTypeNotAllowed => {
                "cash product is not allowed for the settlement account type"
            }
            Self::InvalidSettlementExecution => {
                "cash settlement execution violates planner invariants"
            }
            Self::FinanceRule(error) => return Display::fmt(error, formatter),
        };
        formatter.write_str(message)
    }
}

impl Error for CashProductError {}

impl From<FinanceRuleError> for CashProductError {
    fn from(error: FinanceRuleError) -> Self {
        Self::FinanceRule(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterestTaxPolicy {
    pub income_tax_rate_ppm: i64,
    pub local_income_tax_rate_ppm: i64,
}

impl InterestTaxPolicy {
    pub fn validate(self) -> Result<(), CashProductError> {
        if self.income_tax_rate_ppm < 0
            || self.local_income_tax_rate_ppm < 0
            || i128::from(self.income_tax_rate_ppm) > RATE_SCALE_PPM
            || i128::from(self.local_income_tax_rate_ppm) > RATE_SCALE_PPM
            || i128::from(self.income_tax_rate_ppm)
                .checked_add(i128::from(self.local_income_tax_rate_ppm))
                .ok_or(CashProductError::ArithmeticOverflow)?
                > RATE_SCALE_PPM
        {
            return Err(CashProductError::InvalidRate);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepositProtectionPolicy {
    pub limit_krw: i64,
}

impl DepositProtectionPolicy {
    pub fn validate(self) -> Result<(), CashProductError> {
        if self.limit_krw < 0 {
            return Err(CashProductError::InvalidMoney);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CashProductPolicy {
    pub interest_tax: InterestTaxPolicy,
    pub deposit_protection: DepositProtectionPolicy,
}

impl CashProductPolicy {
    pub fn validate(self) -> Result<(), CashProductError> {
        self.interest_tax.validate()?;
        self.deposit_protection.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CashPayloadVersion;

impl CashPayloadVersion {
    pub const V1: Self = Self;
}

impl Serialize for CashPayloadVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(CASH_SETTLEMENT_PAYLOAD_VERSION)
    }
}

impl<'de> Deserialize<'de> for CashPayloadVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u8::deserialize(deserializer)?;
        if version == CASH_SETTLEMENT_PAYLOAD_VERSION {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("unsupported payload version"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CashSettlementKind {
    CmaInterest,
    DepositMaturity,
    SavingsInstallment,
    SavingsMaturity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CashSettlementSourceKind {
    CmaAccount,
    DepositContract,
    SavingsContract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashSettlementSource {
    pub kind: CashSettlementSourceKind,
    pub source_id: ResourceId,
    pub occurrence: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmaInterestPayloadV1 {
    pub version: CashPayloadVersion,
    pub account_id: ResourceId,
    pub cma_terms_id: ResourceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepositMaturityPayloadV1 {
    pub version: CashPayloadVersion,
    pub account_id: ResourceId,
    pub contract_id: ResourceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavingsInstallmentPayloadV1 {
    pub version: CashPayloadVersion,
    pub account_id: ResourceId,
    pub contract_id: ResourceId,
    pub installment_no: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavingsMaturityPayloadV1 {
    pub version: CashPayloadVersion,
    pub account_id: ResourceId,
    pub contract_id: ResourceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashSettlementPayload {
    CmaInterest(CmaInterestPayloadV1),
    DepositMaturity(DepositMaturityPayloadV1),
    SavingsInstallment(SavingsInstallmentPayloadV1),
    SavingsMaturity(SavingsMaturityPayloadV1),
}

impl CashSettlementPayload {
    pub fn decode(kind: CashSettlementKind, payload: Value) -> Result<Self, CashProductError> {
        validate_payload_version(&payload)?;

        match kind {
            CashSettlementKind::CmaInterest => serde_json::from_value(payload)
                .map(Self::CmaInterest)
                .map_err(|_| CashProductError::InvalidPayload),
            CashSettlementKind::DepositMaturity => serde_json::from_value(payload)
                .map(Self::DepositMaturity)
                .map_err(|_| CashProductError::InvalidPayload),
            CashSettlementKind::SavingsInstallment => {
                let payload: SavingsInstallmentPayloadV1 = serde_json::from_value(payload)
                    .map_err(|_| CashProductError::InvalidPayload)?;
                if payload.installment_no == 0 {
                    return Err(CashProductError::InvalidPayload);
                }
                Ok(Self::SavingsInstallment(payload))
            }
            CashSettlementKind::SavingsMaturity => serde_json::from_value(payload)
                .map(Self::SavingsMaturity)
                .map_err(|_| CashProductError::InvalidPayload),
        }
    }

    pub const fn kind(self) -> CashSettlementKind {
        match self {
            Self::CmaInterest(_) => CashSettlementKind::CmaInterest,
            Self::DepositMaturity(_) => CashSettlementKind::DepositMaturity,
            Self::SavingsInstallment(_) => CashSettlementKind::SavingsInstallment,
            Self::SavingsMaturity(_) => CashSettlementKind::SavingsMaturity,
        }
    }

    pub const fn account_id(self) -> ResourceId {
        match self {
            Self::CmaInterest(payload) => payload.account_id,
            Self::DepositMaturity(payload) => payload.account_id,
            Self::SavingsInstallment(payload) => payload.account_id,
            Self::SavingsMaturity(payload) => payload.account_id,
        }
    }
}

fn validate_payload_version(payload: &Value) -> Result<(), CashProductError> {
    let Some(version) = payload.get("version") else {
        return Err(CashProductError::InvalidPayload);
    };
    let Some(version) = version.as_u64() else {
        return Err(CashProductError::InvalidPayload);
    };
    if version != u64::from(CASH_SETTLEMENT_PAYLOAD_VERSION) {
        return Err(CashProductError::UnsupportedPayloadVersion);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DayCountBasis {
    Actual365,
}

impl DayCountBasis {
    const fn days(self) -> i128 {
        match self {
            Self::Actual365 => ACTUAL_365_DAYS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WithholdingTax {
    pub gross_interest_krw: i64,
    pub income_tax_krw: i64,
    pub local_income_tax_krw: i64,
    pub net_interest_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinancialIncomeDelta {
    pub gross_financial_income_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashProductTaxTreatment {
    Taxable,
    Isa,
    Pension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxAdvantagedInterestDelta {
    None,
    IsaTaxProfit { amount_krw: i64 },
    PensionEarnings { amount_krw: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CashInterestTreatment {
    pub tax_treatment: CashProductTaxTreatment,
    pub withholding: WithholdingTax,
    pub financial_income_delta: FinancialIncomeDelta,
    pub tax_advantaged_interest_delta: TaxAdvantagedInterestDelta,
}

pub const fn cash_product_tax_treatment(
    account_type: FinancialAccountType,
) -> Option<CashProductTaxTreatment> {
    match account_type {
        FinancialAccountType::TaxableBrokerage => Some(CashProductTaxTreatment::Taxable),
        FinancialAccountType::IsaGeneral | FinancialAccountType::IsaLowIncome => {
            Some(CashProductTaxTreatment::Isa)
        }
        FinancialAccountType::PensionSavings | FinancialAccountType::Irp => {
            Some(CashProductTaxTreatment::Pension)
        }
        FinancialAccountType::Cma | FinancialAccountType::KrxGold => None,
    }
}

pub fn calculate_cash_interest_treatment(
    gross_interest_krw: i64,
    account_type: FinancialAccountType,
    policy: InterestTaxPolicy,
) -> Result<CashInterestTreatment, CashProductError> {
    let tax_treatment =
        cash_product_tax_treatment(account_type).ok_or(CashProductError::AccountTypeNotAllowed)?;
    calculate_interest_treatment(gross_interest_krw, tax_treatment, policy)
}

fn calculate_interest_treatment(
    gross_interest_krw: i64,
    tax_treatment: CashProductTaxTreatment,
    policy: InterestTaxPolicy,
) -> Result<CashInterestTreatment, CashProductError> {
    if gross_interest_krw < 0 {
        return Err(CashProductError::InvalidMoney);
    }
    policy.validate()?;

    let withholding = match tax_treatment {
        CashProductTaxTreatment::Taxable => {
            calculate_interest_withholding(gross_interest_krw, policy)?
        }
        CashProductTaxTreatment::Isa | CashProductTaxTreatment::Pension => WithholdingTax {
            gross_interest_krw,
            income_tax_krw: 0,
            local_income_tax_krw: 0,
            net_interest_krw: gross_interest_krw,
        },
    };
    let financial_income_delta = match tax_treatment {
        CashProductTaxTreatment::Taxable => FinancialIncomeDelta::from(withholding),
        CashProductTaxTreatment::Isa | CashProductTaxTreatment::Pension => {
            FinancialIncomeDelta::ZERO
        }
    };
    let tax_advantaged_interest_delta = match (tax_treatment, gross_interest_krw) {
        (_, 0) | (CashProductTaxTreatment::Taxable, _) => TaxAdvantagedInterestDelta::None,
        (CashProductTaxTreatment::Isa, amount_krw) => {
            TaxAdvantagedInterestDelta::IsaTaxProfit { amount_krw }
        }
        (CashProductTaxTreatment::Pension, amount_krw) => {
            TaxAdvantagedInterestDelta::PensionEarnings { amount_krw }
        }
    };

    Ok(CashInterestTreatment {
        tax_treatment,
        withholding,
        financial_income_delta,
        tax_advantaged_interest_delta,
    })
}

impl FinancialIncomeDelta {
    pub const ZERO: Self = Self {
        gross_financial_income_krw: 0,
        withheld_income_tax_krw: 0,
        withheld_local_income_tax_krw: 0,
    };
}

impl From<WithholdingTax> for FinancialIncomeDelta {
    fn from(withholding: WithholdingTax) -> Self {
        Self {
            gross_financial_income_krw: withholding.gross_interest_krw,
            withheld_income_tax_krw: withholding.income_tax_krw,
            withheld_local_income_tax_krw: withholding.local_income_tax_krw,
        }
    }
}

pub fn calculate_interest_withholding(
    gross_interest_krw: i64,
    policy: InterestTaxPolicy,
) -> Result<WithholdingTax, CashProductError> {
    if gross_interest_krw < 0 {
        return Err(CashProductError::InvalidMoney);
    }
    policy.validate()?;

    let income_tax_krw = floor_rate(gross_interest_krw, policy.income_tax_rate_ppm)?;
    let local_income_tax_krw = floor_rate(gross_interest_krw, policy.local_income_tax_rate_ppm)?;
    let net_interest_krw = gross_interest_krw
        .checked_sub(income_tax_krw)
        .and_then(|value| value.checked_sub(local_income_tax_krw))
        .ok_or(CashProductError::ArithmeticOverflow)?;

    Ok(WithholdingTax {
        gross_interest_krw,
        income_tax_krw,
        local_income_tax_krw,
        net_interest_krw,
    })
}

fn floor_rate(amount_krw: i64, rate_ppm: i64) -> Result<i64, CashProductError> {
    let value = i128::from(amount_krw)
        .checked_mul(i128::from(rate_ppm))
        .ok_or(CashProductError::ArithmeticOverflow)?
        / RATE_SCALE_PPM;
    i64::try_from(value).map_err(|_| CashProductError::ArithmeticOverflow)
}

pub fn calculate_simple_interest_krw(
    principal_krw: i64,
    annual_rate_bp: i32,
    held_days: u32,
    day_count_basis: DayCountBasis,
) -> Result<i64, CashProductError> {
    if principal_krw < 0 {
        return Err(CashProductError::InvalidMoney);
    }
    if annual_rate_bp < 0 {
        return Err(CashProductError::InvalidRate);
    }

    let numerator = i128::from(principal_krw)
        .checked_mul(i128::from(annual_rate_bp))
        .and_then(|value| value.checked_mul(i128::from(held_days)))
        .ok_or(CashProductError::ArithmeticOverflow)?;
    let denominator = day_count_basis
        .days()
        .checked_mul(BASIS_POINT_SCALE)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    i64::try_from(numerator / denominator).map_err(|_| CashProductError::ArithmeticOverflow)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CashNoMovementReason {
    BelowMinimumBalance,
    FractionalInterest,
    InsufficientAccountCash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashSettlementOutcome {
    Applied,
    NoMovement(CashNoMovementReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmaDailyTerms {
    pub spread_bp: i32,
    pub minimum_interest_balance_krw: i64,
    pub day_count_basis: DayCountBasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmaDailyAccrualInput {
    pub principal_krw: i64,
    pub treasury_3m_bp: i32,
    pub interest_remainder: i64,
    pub terms: CmaDailyTerms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmaDailyAccrual {
    pub outcome: CashSettlementOutcome,
    pub annual_rate_bp: i32,
    pub gross_interest_krw: i64,
    pub withholding: WithholdingTax,
    pub reinvested_interest_krw: i64,
    pub next_principal_krw: i64,
    pub next_interest_remainder: i64,
}

pub fn accrue_cma_daily(
    finance_rules: &dyn FinanceRules,
    input: CmaDailyAccrualInput,
    tax_policy: InterestTaxPolicy,
) -> Result<CmaDailyAccrual, CashProductError> {
    if input.principal_krw < 0 || input.terms.minimum_interest_balance_krw < 0 {
        return Err(CashProductError::InvalidMoney);
    }
    if !(0..DAILY_INTEREST_DENOMINATOR).contains(&input.interest_remainder) {
        return Err(CashProductError::InvalidRate);
    }
    let annual_rate_bp = input
        .treasury_3m_bp
        .checked_add(input.terms.spread_bp)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    if annual_rate_bp < 0 {
        return Err(CashProductError::InvalidRate);
    }

    if input.principal_krw < input.terms.minimum_interest_balance_krw {
        return cma_no_movement(
            input.principal_krw,
            annual_rate_bp,
            input.interest_remainder,
            CashNoMovementReason::BelowMinimumBalance,
            tax_policy,
        );
    }

    let accrual = finance_rules.accrue_daily_interest(DailyInterestInput {
        principal_krw: input.principal_krw,
        annual_rate_bp,
        remainder: input.interest_remainder,
    })?;
    if accrual.interest_krw == 0 {
        return cma_no_movement(
            input.principal_krw,
            annual_rate_bp,
            accrual.remainder,
            CashNoMovementReason::FractionalInterest,
            tax_policy,
        );
    }

    let withholding = calculate_interest_withholding(accrual.interest_krw, tax_policy)?;
    let next_principal_krw = input
        .principal_krw
        .checked_add(withholding.net_interest_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    Ok(CmaDailyAccrual {
        outcome: CashSettlementOutcome::Applied,
        annual_rate_bp,
        gross_interest_krw: accrual.interest_krw,
        withholding,
        reinvested_interest_krw: withholding.net_interest_krw,
        next_principal_krw,
        next_interest_remainder: accrual.remainder,
    })
}

fn cma_no_movement(
    principal_krw: i64,
    annual_rate_bp: i32,
    interest_remainder: i64,
    reason: CashNoMovementReason,
    tax_policy: InterestTaxPolicy,
) -> Result<CmaDailyAccrual, CashProductError> {
    let withholding = calculate_interest_withholding(0, tax_policy)?;
    Ok(CmaDailyAccrual {
        outcome: CashSettlementOutcome::NoMovement(reason),
        annual_rate_bp,
        gross_interest_krw: 0,
        withholding,
        reinvested_interest_krw: 0,
        next_principal_krw: principal_krw,
        next_interest_remainder: interest_remainder,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermDepositContract {
    pub principal_krw: i64,
    pub annual_rate_bp: i32,
    pub early_close_rate_bp: i32,
    pub opened_game_day: u32,
    pub maturity_game_day: u32,
    pub day_count_basis: DayCountBasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterestPayout {
    pub principal_krw: i64,
    pub held_days: u32,
    pub withholding: WithholdingTax,
    pub financial_income_delta: FinancialIncomeDelta,
    pub tax_advantaged_interest_delta: TaxAdvantagedInterestDelta,
    pub cash_payout_krw: i64,
}

pub fn settle_term_deposit_maturity(
    contract: TermDepositContract,
    settlement_game_day: u32,
    tax_policy: InterestTaxPolicy,
) -> Result<InterestPayout, CashProductError> {
    settle_term_deposit_maturity_with_treatment(
        contract,
        settlement_game_day,
        CashProductTaxTreatment::Taxable,
        tax_policy,
    )
}

pub fn settle_term_deposit_maturity_for_account(
    contract: TermDepositContract,
    settlement_game_day: u32,
    account_type: FinancialAccountType,
    tax_policy: InterestTaxPolicy,
) -> Result<InterestPayout, CashProductError> {
    let tax_treatment =
        cash_product_tax_treatment(account_type).ok_or(CashProductError::AccountTypeNotAllowed)?;
    settle_term_deposit_maturity_with_treatment(
        contract,
        settlement_game_day,
        tax_treatment,
        tax_policy,
    )
}

fn settle_term_deposit_maturity_with_treatment(
    contract: TermDepositContract,
    settlement_game_day: u32,
    tax_treatment: CashProductTaxTreatment,
    tax_policy: InterestTaxPolicy,
) -> Result<InterestPayout, CashProductError> {
    validate_term_deposit(contract)?;
    if settlement_game_day < contract.maturity_game_day {
        return Err(CashProductError::InvalidGameDay);
    }

    let held_days = contract
        .maturity_game_day
        .checked_sub(contract.opened_game_day)
        .ok_or(CashProductError::InvalidGameDay)?;
    calculate_interest_payout(
        contract.principal_krw,
        contract.annual_rate_bp,
        held_days,
        contract.day_count_basis,
        tax_treatment,
        tax_policy,
    )
}

pub fn settle_term_deposit_early_close(
    contract: TermDepositContract,
    close_game_day: u32,
    tax_policy: InterestTaxPolicy,
) -> Result<InterestPayout, CashProductError> {
    settle_term_deposit_early_close_with_treatment(
        contract,
        close_game_day,
        CashProductTaxTreatment::Taxable,
        tax_policy,
    )
}

pub fn settle_term_deposit_early_close_for_account(
    contract: TermDepositContract,
    close_game_day: u32,
    account_type: FinancialAccountType,
    tax_policy: InterestTaxPolicy,
) -> Result<InterestPayout, CashProductError> {
    let tax_treatment =
        cash_product_tax_treatment(account_type).ok_or(CashProductError::AccountTypeNotAllowed)?;
    settle_term_deposit_early_close_with_treatment(
        contract,
        close_game_day,
        tax_treatment,
        tax_policy,
    )
}

fn settle_term_deposit_early_close_with_treatment(
    contract: TermDepositContract,
    close_game_day: u32,
    tax_treatment: CashProductTaxTreatment,
    tax_policy: InterestTaxPolicy,
) -> Result<InterestPayout, CashProductError> {
    validate_term_deposit(contract)?;
    if close_game_day < contract.opened_game_day || close_game_day >= contract.maturity_game_day {
        return Err(CashProductError::InvalidGameDay);
    }

    let held_days = close_game_day
        .checked_sub(contract.opened_game_day)
        .ok_or(CashProductError::InvalidGameDay)?;
    calculate_interest_payout(
        contract.principal_krw,
        contract.early_close_rate_bp,
        held_days,
        contract.day_count_basis,
        tax_treatment,
        tax_policy,
    )
}

fn validate_term_deposit(contract: TermDepositContract) -> Result<(), CashProductError> {
    if contract.principal_krw <= 0 {
        return Err(CashProductError::InvalidMoney);
    }
    if contract.annual_rate_bp < 0 || contract.early_close_rate_bp < 0 {
        return Err(CashProductError::InvalidRate);
    }
    if contract.opened_game_day >= contract.maturity_game_day {
        return Err(CashProductError::InvalidGameDay);
    }
    Ok(())
}

fn calculate_interest_payout(
    principal_krw: i64,
    annual_rate_bp: i32,
    held_days: u32,
    day_count_basis: DayCountBasis,
    tax_treatment: CashProductTaxTreatment,
    tax_policy: InterestTaxPolicy,
) -> Result<InterestPayout, CashProductError> {
    let gross_interest_krw =
        calculate_simple_interest_krw(principal_krw, annual_rate_bp, held_days, day_count_basis)?;
    let interest = calculate_interest_treatment(gross_interest_krw, tax_treatment, tax_policy)?;
    let cash_payout_krw = principal_krw
        .checked_add(interest.withholding.net_interest_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    Ok(InterestPayout {
        principal_krw,
        held_days,
        withholding: interest.withholding,
        financial_income_delta: interest.financial_income_delta,
        tax_advantaged_interest_delta: interest.tax_advantaged_interest_delta,
        cash_payout_krw,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallmentSavingsScheduleInput {
    pub world_start_date: Date,
    pub opened_market_date: Date,
    pub opened_game_day: u32,
    pub term_months: u32,
    pub installment_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavingsInstallmentDue {
    pub installment_no: u32,
    pub due_date: Date,
    pub due_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallmentSavingsSchedule {
    pub installments: Vec<SavingsInstallmentDue>,
    pub maturity_date: Date,
    pub maturity_game_day: u32,
}

pub fn create_installment_savings_schedule(
    input: InstallmentSavingsScheduleInput,
) -> Result<InstallmentSavingsSchedule, CashProductError> {
    if input.term_months == 0
        || input.installment_count == 0
        || input.installment_count > input.term_months
    {
        return Err(CashProductError::InvalidSchedule);
    }
    let expected_opened_date = input
        .world_start_date
        .checked_add(Duration::days(i64::from(input.opened_game_day)))
        .ok_or(CashProductError::InvalidGameDay)?;
    if expected_opened_date != input.opened_market_date {
        return Err(CashProductError::InvalidGameDay);
    }

    let maturity_date = add_months_clamped(input.opened_market_date, input.term_months)?;
    let maturity_game_day = game_day_for_date(input.world_start_date, maturity_date)?;
    let mut installments = Vec::with_capacity(
        usize::try_from(input.installment_count)
            .map_err(|_| CashProductError::ArithmeticOverflow)?,
    );
    for installment_no in 1..=input.installment_count {
        let month_offset = installment_no
            .checked_sub(1)
            .ok_or(CashProductError::ArithmeticOverflow)?;
        let due_date = add_months_clamped(input.opened_market_date, month_offset)?;
        let due_game_day = game_day_for_date(input.world_start_date, due_date)?;
        installments.push(SavingsInstallmentDue {
            installment_no,
            due_date,
            due_game_day,
        });
    }

    let last_installment = installments
        .last()
        .ok_or(CashProductError::InvalidSchedule)?;
    if last_installment.due_date >= maturity_date
        || last_installment.due_game_day >= maturity_game_day
    {
        return Err(CashProductError::InvalidSchedule);
    }
    Ok(InstallmentSavingsSchedule {
        installments,
        maturity_date,
        maturity_game_day,
    })
}

fn add_months_clamped(date: Date, months: u32) -> Result<Date, CashProductError> {
    let base_month = i64::from(date.year())
        .checked_mul(12)
        .and_then(|value| value.checked_add(i64::from(u8::from(date.month())) - 1))
        .ok_or(CashProductError::ArithmeticOverflow)?;
    let target_month = base_month
        .checked_add(i64::from(months))
        .ok_or(CashProductError::ArithmeticOverflow)?;
    let year =
        i32::try_from(target_month.div_euclid(12)).map_err(|_| CashProductError::InvalidGameDay)?;
    let month_number = u8::try_from(target_month.rem_euclid(12) + 1)
        .map_err(|_| CashProductError::InvalidGameDay)?;
    let month = Month::try_from(month_number).map_err(|_| CashProductError::InvalidGameDay)?;

    for day in (1..=date.day()).rev() {
        if let Ok(candidate) = Date::from_calendar_date(year, month, day) {
            return Ok(candidate);
        }
    }
    Err(CashProductError::InvalidGameDay)
}

fn game_day_for_date(world_start_date: Date, date: Date) -> Result<u32, CashProductError> {
    u32::try_from((date - world_start_date).whole_days())
        .map_err(|_| CashProductError::InvalidGameDay)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavingsInstallmentPrincipal {
    pub installment_no: u32,
    pub principal_krw: i64,
    pub paid_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallmentSavingsContract {
    pub annual_rate_bp: i32,
    pub early_close_rate_bp: i32,
    pub maturity_game_day: u32,
    pub day_count_basis: DayCountBasis,
    pub paid_installments: Vec<SavingsInstallmentPrincipal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavingsInstallmentInterest {
    pub installment_no: u32,
    pub principal_krw: i64,
    pub held_days: u32,
    pub gross_interest_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallmentSavingsPayout {
    pub installments: Vec<SavingsInstallmentInterest>,
    pub principal_krw: i64,
    pub withholding: WithholdingTax,
    pub financial_income_delta: FinancialIncomeDelta,
    pub tax_advantaged_interest_delta: TaxAdvantagedInterestDelta,
    pub cash_payout_krw: i64,
}

pub fn settle_installment_savings_maturity(
    contract: &InstallmentSavingsContract,
    settlement_game_day: u32,
    tax_policy: InterestTaxPolicy,
) -> Result<InstallmentSavingsPayout, CashProductError> {
    settle_installment_savings_maturity_with_treatment(
        contract,
        settlement_game_day,
        CashProductTaxTreatment::Taxable,
        tax_policy,
    )
}

pub fn settle_installment_savings_maturity_for_account(
    contract: &InstallmentSavingsContract,
    settlement_game_day: u32,
    account_type: FinancialAccountType,
    tax_policy: InterestTaxPolicy,
) -> Result<InstallmentSavingsPayout, CashProductError> {
    let tax_treatment =
        cash_product_tax_treatment(account_type).ok_or(CashProductError::AccountTypeNotAllowed)?;
    settle_installment_savings_maturity_with_treatment(
        contract,
        settlement_game_day,
        tax_treatment,
        tax_policy,
    )
}

fn settle_installment_savings_maturity_with_treatment(
    contract: &InstallmentSavingsContract,
    settlement_game_day: u32,
    tax_treatment: CashProductTaxTreatment,
    tax_policy: InterestTaxPolicy,
) -> Result<InstallmentSavingsPayout, CashProductError> {
    if settlement_game_day < contract.maturity_game_day {
        return Err(CashProductError::InvalidGameDay);
    }
    calculate_installment_savings_payout(
        contract,
        contract.maturity_game_day,
        contract.annual_rate_bp,
        tax_treatment,
        tax_policy,
    )
}

pub fn settle_installment_savings_early_close(
    contract: &InstallmentSavingsContract,
    close_game_day: u32,
    tax_policy: InterestTaxPolicy,
) -> Result<InstallmentSavingsPayout, CashProductError> {
    settle_installment_savings_early_close_with_treatment(
        contract,
        close_game_day,
        CashProductTaxTreatment::Taxable,
        tax_policy,
    )
}

pub fn settle_installment_savings_early_close_for_account(
    contract: &InstallmentSavingsContract,
    close_game_day: u32,
    account_type: FinancialAccountType,
    tax_policy: InterestTaxPolicy,
) -> Result<InstallmentSavingsPayout, CashProductError> {
    let tax_treatment =
        cash_product_tax_treatment(account_type).ok_or(CashProductError::AccountTypeNotAllowed)?;
    settle_installment_savings_early_close_with_treatment(
        contract,
        close_game_day,
        tax_treatment,
        tax_policy,
    )
}

fn settle_installment_savings_early_close_with_treatment(
    contract: &InstallmentSavingsContract,
    close_game_day: u32,
    tax_treatment: CashProductTaxTreatment,
    tax_policy: InterestTaxPolicy,
) -> Result<InstallmentSavingsPayout, CashProductError> {
    if close_game_day >= contract.maturity_game_day {
        return Err(CashProductError::InvalidGameDay);
    }
    calculate_installment_savings_payout(
        contract,
        close_game_day,
        contract.early_close_rate_bp,
        tax_treatment,
        tax_policy,
    )
}

fn calculate_installment_savings_payout(
    contract: &InstallmentSavingsContract,
    payout_game_day: u32,
    annual_rate_bp: i32,
    tax_treatment: CashProductTaxTreatment,
    tax_policy: InterestTaxPolicy,
) -> Result<InstallmentSavingsPayout, CashProductError> {
    if contract.annual_rate_bp < 0 || contract.early_close_rate_bp < 0 || annual_rate_bp < 0 {
        return Err(CashProductError::InvalidRate);
    }
    if contract.paid_installments.is_empty() {
        return Err(CashProductError::InvalidInstallment);
    }

    let mut seen = BTreeSet::new();
    let mut paid_installments = contract.paid_installments.clone();
    paid_installments.sort_by_key(|installment| installment.installment_no);
    let mut installments = Vec::with_capacity(paid_installments.len());
    let mut principal_krw = 0_i64;
    let mut gross_interest_krw = 0_i64;

    for installment in paid_installments {
        if installment.installment_no == 0
            || installment.principal_krw <= 0
            || installment.paid_game_day > payout_game_day
        {
            return Err(CashProductError::InvalidInstallment);
        }
        if !seen.insert(installment.installment_no) {
            return Err(CashProductError::DuplicateInstallment);
        }

        let held_days = payout_game_day
            .checked_sub(installment.paid_game_day)
            .ok_or(CashProductError::InvalidGameDay)?;
        let installment_interest_krw = calculate_simple_interest_krw(
            installment.principal_krw,
            annual_rate_bp,
            held_days,
            contract.day_count_basis,
        )?;
        principal_krw = principal_krw
            .checked_add(installment.principal_krw)
            .ok_or(CashProductError::ArithmeticOverflow)?;
        gross_interest_krw = gross_interest_krw
            .checked_add(installment_interest_krw)
            .ok_or(CashProductError::ArithmeticOverflow)?;
        installments.push(SavingsInstallmentInterest {
            installment_no: installment.installment_no,
            principal_krw: installment.principal_krw,
            held_days,
            gross_interest_krw: installment_interest_krw,
        });
    }

    let interest = calculate_interest_treatment(gross_interest_krw, tax_treatment, tax_policy)?;
    let cash_payout_krw = principal_krw
        .checked_add(interest.withholding.net_interest_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    Ok(InstallmentSavingsPayout {
        installments,
        principal_krw,
        withholding: interest.withholding,
        financial_income_delta: interest.financial_income_delta,
        tax_advantaged_interest_delta: interest.tax_advantaged_interest_delta,
        cash_payout_krw,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavingsInstallmentCollection {
    pub outcome: CashSettlementOutcome,
    pub next_account_cash_krw: i64,
    pub collected_principal_krw: i64,
}

pub fn collect_savings_installment(
    account_cash_krw: i64,
    installment_principal_krw: i64,
) -> Result<SavingsInstallmentCollection, CashProductError> {
    if account_cash_krw < 0 || installment_principal_krw <= 0 {
        return Err(CashProductError::InvalidMoney);
    }
    if account_cash_krw < installment_principal_krw {
        return Ok(SavingsInstallmentCollection {
            outcome: CashSettlementOutcome::NoMovement(
                CashNoMovementReason::InsufficientAccountCash,
            ),
            next_account_cash_krw: account_cash_krw,
            collected_principal_krw: 0,
        });
    }

    let next_account_cash_krw = account_cash_krw
        .checked_sub(installment_principal_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    Ok(SavingsInstallmentCollection {
        outcome: CashSettlementOutcome::Applied,
        next_account_cash_krw,
        collected_principal_krw: installment_principal_krw,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementLedgerContext {
    pub policy: RunPolicyContext,
    pub source: LedgerSource,
    pub game_day: u32,
    pub description: String,
    pub account_id: ResourceId,
}

pub fn create_interest_payout_ledger(
    finance_rules: &dyn FinanceRules,
    context: SettlementLedgerContext,
    principal_krw: i64,
    withholding: WithholdingTax,
    tax_policy: InterestTaxPolicy,
) -> Result<Option<LedgerTransaction>, CashProductError> {
    let interest = calculate_interest_treatment(
        withholding.gross_interest_krw,
        CashProductTaxTreatment::Taxable,
        tax_policy,
    )?;
    create_interest_payout_ledger_with_treatment(
        finance_rules,
        context,
        principal_krw,
        withholding,
        interest,
    )
}

pub fn create_interest_payout_ledger_for_account(
    finance_rules: &dyn FinanceRules,
    context: SettlementLedgerContext,
    principal_krw: i64,
    withholding: WithholdingTax,
    account_type: FinancialAccountType,
    tax_policy: InterestTaxPolicy,
) -> Result<Option<LedgerTransaction>, CashProductError> {
    let interest = calculate_cash_interest_treatment(
        withholding.gross_interest_krw,
        account_type,
        tax_policy,
    )?;
    create_interest_payout_ledger_with_treatment(
        finance_rules,
        context,
        principal_krw,
        withholding,
        interest,
    )
}

fn create_interest_payout_ledger_with_treatment(
    finance_rules: &dyn FinanceRules,
    context: SettlementLedgerContext,
    principal_krw: i64,
    withholding: WithholdingTax,
    interest: CashInterestTreatment,
) -> Result<Option<LedgerTransaction>, CashProductError> {
    if principal_krw < 0 {
        return Err(CashProductError::InvalidMoney);
    }
    if interest.withholding != withholding {
        return Err(CashProductError::InvalidMoney);
    }
    if principal_krw == 0 && withholding.gross_interest_krw == 0 {
        return Ok(None);
    }

    let account_credit_krw = principal_krw
        .checked_add(withholding.net_interest_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    let mut postings = vec![LedgerPosting {
        account_code: LedgerAccountCode::AccountCash,
        financial_account_id: Some(context.account_id),
        amount_krw: account_credit_krw,
    }];
    if principal_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::ProductPrincipal,
            financial_account_id: Some(context.account_id),
            amount_krw: principal_krw
                .checked_neg()
                .ok_or(CashProductError::ArithmeticOverflow)?,
        });
    }
    if withholding.gross_interest_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::InterestIncome,
            financial_account_id: None,
            amount_krw: withholding
                .gross_interest_krw
                .checked_neg()
                .ok_or(CashProductError::ArithmeticOverflow)?,
        });
    }
    let total_tax_krw = withholding
        .income_tax_krw
        .checked_add(withholding.local_income_tax_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    if total_tax_krw > 0 {
        postings.push(LedgerPosting {
            account_code: LedgerAccountCode::WithholdingTaxLiability,
            financial_account_id: None,
            amount_krw: total_tax_krw,
        });
    }

    finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: context.policy,
            source: context.source,
            game_day: context.game_day,
            description: context.description,
            postings,
        })
        .map(Some)
        .map_err(Into::into)
}

pub fn create_product_principal_funding_ledger(
    finance_rules: &dyn FinanceRules,
    context: SettlementLedgerContext,
    principal_krw: i64,
) -> Result<LedgerTransaction, CashProductError> {
    if principal_krw <= 0 {
        return Err(CashProductError::InvalidMoney);
    }
    finance_rules
        .create_ledger_transaction(LedgerTransactionDraft {
            policy: context.policy,
            source: context.source,
            game_day: context.game_day,
            description: context.description,
            postings: vec![
                LedgerPosting {
                    account_code: LedgerAccountCode::AccountCash,
                    financial_account_id: Some(context.account_id),
                    amount_krw: principal_krw
                        .checked_neg()
                        .ok_or(CashProductError::ArithmeticOverflow)?,
                },
                LedgerPosting {
                    account_code: LedgerAccountCode::ProductPrincipal,
                    financial_account_id: Some(context.account_id),
                    amount_krw: principal_krw,
                },
            ],
        })
        .map_err(Into::into)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedDepositAmount {
    pub institution_id: String,
    pub principal_krw: i64,
    pub prescribed_interest_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositProtectionSummary {
    pub institution_id: String,
    pub eligible_amount_krw: i64,
    pub protected_amount_krw: i64,
    pub unprotected_amount_krw: i64,
}

pub fn aggregate_deposit_protection(
    deposits: &[ProtectedDepositAmount],
    policy: DepositProtectionPolicy,
) -> Result<Vec<DepositProtectionSummary>, CashProductError> {
    policy.validate()?;
    let mut eligible_by_institution = BTreeMap::<String, i64>::new();
    for deposit in deposits {
        if deposit.institution_id.trim().is_empty() {
            return Err(CashProductError::InvalidInstitutionId);
        }
        if deposit.principal_krw < 0 || deposit.prescribed_interest_krw < 0 {
            return Err(CashProductError::InvalidMoney);
        }
        let eligible_amount_krw = deposit
            .principal_krw
            .checked_add(deposit.prescribed_interest_krw)
            .ok_or(CashProductError::ArithmeticOverflow)?;
        let current = eligible_by_institution
            .entry(deposit.institution_id.clone())
            .or_default();
        *current = current
            .checked_add(eligible_amount_krw)
            .ok_or(CashProductError::ArithmeticOverflow)?;
    }

    eligible_by_institution
        .into_iter()
        .map(|(institution_id, eligible_amount_krw)| {
            let protected_amount_krw = eligible_amount_krw.min(policy.limit_krw);
            let unprotected_amount_krw = eligible_amount_krw
                .checked_sub(protected_amount_krw)
                .ok_or(CashProductError::ArithmeticOverflow)?;
            Ok(DepositProtectionSummary {
                institution_id,
                eligible_amount_krw,
                protected_amount_krw,
                unprotected_amount_krw,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CashSettlementTask {
    pub id: ResourceId,
    pub due_game_day: u32,
    pub source: CashSettlementSource,
    pub payload: CashSettlementPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CashSettlementFollowUpDraft {
    pub due_game_day: u32,
    pub source: CashSettlementSource,
    pub payload: CashSettlementPayload,
}

impl CashSettlementTask {
    pub fn decode(
        id: ResourceId,
        due_game_day: u32,
        source: CashSettlementSource,
        kind: CashSettlementKind,
        payload: Value,
    ) -> Result<Self, CashProductError> {
        let task = Self {
            id,
            due_game_day,
            source,
            payload: CashSettlementPayload::decode(kind, payload)?,
        };
        task.validate_source_identity()?;
        Ok(task)
    }

    pub fn validate_source_identity(self) -> Result<(), CashProductError> {
        let valid = match (self.payload, self.source) {
            (
                CashSettlementPayload::CmaInterest(payload),
                CashSettlementSource {
                    kind: CashSettlementSourceKind::CmaAccount,
                    source_id,
                    occurrence,
                },
            ) => source_id == payload.account_id && occurrence > 0,
            (
                CashSettlementPayload::DepositMaturity(payload),
                CashSettlementSource {
                    kind: CashSettlementSourceKind::DepositContract,
                    source_id,
                    occurrence: 0,
                },
            ) => source_id == payload.contract_id,
            (
                CashSettlementPayload::SavingsInstallment(payload),
                CashSettlementSource {
                    kind: CashSettlementSourceKind::SavingsContract,
                    source_id,
                    occurrence,
                },
            ) => {
                payload.installment_no > 0
                    && source_id == payload.contract_id
                    && occurrence == payload.installment_no
            }
            (
                CashSettlementPayload::SavingsMaturity(payload),
                CashSettlementSource {
                    kind: CashSettlementSourceKind::SavingsContract,
                    source_id,
                    occurrence: 0,
                },
            ) => source_id == payload.contract_id,
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(CashProductError::InvalidPayload)
        }
    }

    pub fn required_follow_up(
        self,
        game_day: u32,
    ) -> Result<Option<CashSettlementFollowUpDraft>, CashProductError> {
        if !matches!(self.payload, CashSettlementPayload::CmaInterest(_)) {
            return Ok(None);
        }
        let due_game_day = game_day
            .checked_add(1)
            .ok_or(CashProductError::ArithmeticOverflow)?;
        let occurrence = self
            .source
            .occurrence
            .checked_add(1)
            .ok_or(CashProductError::ArithmeticOverflow)?;
        Ok(Some(CashSettlementFollowUpDraft {
            due_game_day,
            source: CashSettlementSource {
                occurrence,
                ..self.source
            },
            payload: self.payload,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashProductMutation {
    CmaAccrued {
        next_principal_krw: i64,
        next_interest_remainder: i64,
    },
    DepositMatured,
    SavingsInstallmentPaid {
        installment_no: u32,
        principal_krw: i64,
    },
    SavingsInstallmentMissed {
        installment_no: u32,
    },
    SavingsMatured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CashSettlementExecution {
    Applied {
        next_account_cash_krw: i64,
        ledger: LedgerTransaction,
        product_mutation: CashProductMutation,
        financial_income_delta: FinancialIncomeDelta,
        follow_up: Option<CashSettlementFollowUpDraft>,
    },
    NoMovement {
        reason: CashNoMovementReason,
        product_mutation: CashProductMutation,
        financial_income_delta: FinancialIncomeDelta,
        follow_up: Option<CashSettlementFollowUpDraft>,
    },
}

impl CashSettlementExecution {
    pub const fn outcome(&self) -> CashSettlementOutcome {
        match self {
            Self::Applied { .. } => CashSettlementOutcome::Applied,
            Self::NoMovement { reason, .. } => CashSettlementOutcome::NoMovement(*reason),
        }
    }

    pub const fn follow_up(&self) -> Option<CashSettlementFollowUpDraft> {
        match self {
            Self::Applied { follow_up, .. } | Self::NoMovement { follow_up, .. } => *follow_up,
        }
    }

    pub const fn financial_income_delta(&self) -> FinancialIncomeDelta {
        match self {
            Self::Applied {
                financial_income_delta,
                ..
            }
            | Self::NoMovement {
                financial_income_delta,
                ..
            } => *financial_income_delta,
        }
    }
}

pub trait CashSettlementExecutorRegistry: Send + Sync + 'static {
    fn execute(
        &self,
        task: &CashSettlementTask,
        game_day: u32,
        current_account_cash_krw: i64,
        account_type: FinancialAccountType,
        policy: CashProductPolicy,
    ) -> Result<CashSettlementExecution, CashProductError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCashSettlementPlanInput {
    pub game_day: u32,
    pub policy: CashProductPolicy,
    pub account_cash_by_id: BTreeMap<ResourceId, i64>,
    pub account_type_by_id: BTreeMap<ResourceId, FinancialAccountType>,
    pub settlements: Vec<CashSettlementTask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCashSettlement {
    pub settlement_id: ResourceId,
    pub outcome: CashSettlementOutcome,
    pub financial_income_delta: FinancialIncomeDelta,
    pub tax_advantaged_interest_delta: TaxAdvantagedInterestDelta,
    pub follow_up: Option<CashSettlementFollowUpDraft>,
    pub execution: CashSettlementExecution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCashSettlementPlan {
    pub account_cash_by_id: BTreeMap<ResourceId, i64>,
    pub settlements: Vec<PlannedCashSettlement>,
}

pub trait CashSettlementPlanner: Send + Sync + 'static {
    fn plan(
        &self,
        input: DailyCashSettlementPlanInput,
    ) -> Result<DailyCashSettlementPlan, CashProductError>;
}

struct DefaultCashSettlementPlanner {
    registry: Arc<dyn CashSettlementExecutorRegistry>,
}

pub fn create_cash_settlement_planner(
    registry: Arc<dyn CashSettlementExecutorRegistry>,
) -> Arc<dyn CashSettlementPlanner> {
    Arc::new(DefaultCashSettlementPlanner { registry })
}

impl CashSettlementPlanner for DefaultCashSettlementPlanner {
    fn plan(
        &self,
        input: DailyCashSettlementPlanInput,
    ) -> Result<DailyCashSettlementPlan, CashProductError> {
        input.policy.validate()?;
        if input.account_cash_by_id.values().any(|cash| *cash < 0) {
            return Err(CashProductError::InvalidMoney);
        }

        let mut seen_ids = BTreeSet::new();
        let mut seen_sources = BTreeSet::new();
        let mut settlements = input.settlements;
        for settlement in &settlements {
            settlement.validate_source_identity()?;
            if !seen_ids.insert(settlement.id) || !seen_sources.insert(settlement.source) {
                return Err(CashProductError::DuplicateSettlement);
            }
            if settlement.due_game_day > input.game_day {
                return Err(CashProductError::FutureSettlement);
            }
        }
        settlements.sort_by_key(|settlement| (settlement.due_game_day, settlement.id));

        let mut account_cash_by_id = input.account_cash_by_id;
        let mut planned = Vec::with_capacity(settlements.len());
        for task in settlements {
            let account_id = task.payload.account_id();
            let current_account_cash_krw = account_cash_by_id
                .get(&account_id)
                .copied()
                .ok_or(CashProductError::AccountNotFound)?;
            let account_type = input
                .account_type_by_id
                .get(&account_id)
                .copied()
                .ok_or(CashProductError::AccountNotFound)?;
            validate_settlement_account_type(task, account_type)?;
            let execution = self.registry.execute(
                &task,
                input.game_day,
                current_account_cash_krw,
                account_type,
                input.policy,
            )?;
            validate_product_mutation(&task, &execution)?;
            validate_account_product_consistency(task, current_account_cash_krw, &execution)?;
            let tax_advantaged_interest_delta = validate_interest_treatment(
                task,
                account_type,
                &execution,
                input.policy.interest_tax,
            )?;
            validate_follow_up(task, input.game_day, execution.follow_up())?;

            if let CashSettlementExecution::Applied {
                next_account_cash_krw,
                ledger,
                ..
            } = &execution
            {
                validate_applied_execution(
                    input.game_day,
                    task.id,
                    account_id,
                    current_account_cash_krw,
                    *next_account_cash_krw,
                    ledger,
                )?;
                account_cash_by_id.insert(account_id, *next_account_cash_krw);
            }
            planned.push(PlannedCashSettlement {
                settlement_id: task.id,
                outcome: execution.outcome(),
                financial_income_delta: execution.financial_income_delta(),
                tax_advantaged_interest_delta,
                follow_up: execution.follow_up(),
                execution,
            });
        }

        Ok(DailyCashSettlementPlan {
            account_cash_by_id,
            settlements: planned,
        })
    }
}

fn validate_settlement_account_type(
    task: CashSettlementTask,
    account_type: FinancialAccountType,
) -> Result<(), CashProductError> {
    let valid = match task.payload {
        CashSettlementPayload::CmaInterest(_) => account_type == FinancialAccountType::Cma,
        CashSettlementPayload::DepositMaturity(_)
        | CashSettlementPayload::SavingsInstallment(_)
        | CashSettlementPayload::SavingsMaturity(_) => {
            cash_product_tax_treatment(account_type).is_some()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(CashProductError::AccountTypeNotAllowed)
    }
}

fn validate_interest_treatment(
    task: CashSettlementTask,
    account_type: FinancialAccountType,
    execution: &CashSettlementExecution,
    tax_policy: InterestTaxPolicy,
) -> Result<TaxAdvantagedInterestDelta, CashProductError> {
    let delta = execution.financial_income_delta();
    if delta.gross_financial_income_krw < 0
        || delta.withheld_income_tax_krw < 0
        || delta.withheld_local_income_tax_krw < 0
    {
        return Err(CashProductError::InvalidSettlementExecution);
    }
    let total_withheld_krw = delta
        .withheld_income_tax_krw
        .checked_add(delta.withheld_local_income_tax_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    if total_withheld_krw > delta.gross_financial_income_krw {
        return Err(CashProductError::InvalidSettlementExecution);
    }

    let CashSettlementExecution::Applied { ledger, .. } = execution else {
        if delta != FinancialIncomeDelta::ZERO {
            return Err(CashProductError::InvalidSettlementExecution);
        }
        return Ok(TaxAdvantagedInterestDelta::None);
    };

    let (gross_interest_krw, _) = ledger_interest_amounts(ledger)?;
    let interest = match task.payload {
        CashSettlementPayload::CmaInterest(_) => calculate_interest_treatment(
            gross_interest_krw,
            CashProductTaxTreatment::Taxable,
            tax_policy,
        )?,
        CashSettlementPayload::SavingsInstallment(_) => {
            if gross_interest_krw != 0 {
                return Err(CashProductError::InvalidSettlementExecution);
            }
            calculate_interest_treatment(0, CashProductTaxTreatment::Taxable, tax_policy)?
        }
        CashSettlementPayload::DepositMaturity(_) | CashSettlementPayload::SavingsMaturity(_) => {
            calculate_cash_interest_treatment(gross_interest_krw, account_type, tax_policy)?
        }
    };
    if delta != interest.financial_income_delta {
        return Err(CashProductError::InvalidSettlementExecution);
    }
    validate_interest_ledger(ledger, interest.withholding)?;
    Ok(interest.tax_advantaged_interest_delta)
}

fn ledger_interest_amounts(ledger: &LedgerTransaction) -> Result<(i64, i64), CashProductError> {
    let mut gross_posting_krw = 0_i128;
    let mut withholding_posting_krw = 0_i128;
    for posting in ledger.postings() {
        match posting.account_code {
            LedgerAccountCode::InterestIncome => {
                gross_posting_krw = gross_posting_krw
                    .checked_add(i128::from(posting.amount_krw))
                    .ok_or(CashProductError::ArithmeticOverflow)?;
            }
            LedgerAccountCode::WithholdingTaxLiability => {
                withholding_posting_krw = withholding_posting_krw
                    .checked_add(i128::from(posting.amount_krw))
                    .ok_or(CashProductError::ArithmeticOverflow)?;
            }
            _ => {}
        }
    }
    let gross_interest_krw = gross_posting_krw
        .checked_neg()
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(CashProductError::InvalidSettlementExecution)?;
    let withholding_krw = i64::try_from(withholding_posting_krw)
        .map_err(|_| CashProductError::InvalidSettlementExecution)?;
    if gross_interest_krw < 0 || withholding_krw < 0 {
        return Err(CashProductError::InvalidSettlementExecution);
    }
    Ok((gross_interest_krw, withholding_krw))
}

fn validate_interest_ledger(
    ledger: &LedgerTransaction,
    withholding: WithholdingTax,
) -> Result<(), CashProductError> {
    let (gross_interest_krw, withholding_krw) = ledger_interest_amounts(ledger)?;
    let expected_withholding_krw = withholding
        .income_tax_krw
        .checked_add(withholding.local_income_tax_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    if gross_interest_krw != withholding.gross_interest_krw
        || withholding_krw != expected_withholding_krw
    {
        return Err(CashProductError::InvalidSettlementExecution);
    }
    Ok(())
}

fn validate_account_product_consistency(
    task: CashSettlementTask,
    current_account_cash_krw: i64,
    execution: &CashSettlementExecution,
) -> Result<(), CashProductError> {
    let valid = match (task.payload, execution) {
        (
            CashSettlementPayload::CmaInterest(_),
            CashSettlementExecution::Applied {
                next_account_cash_krw,
                product_mutation:
                    CashProductMutation::CmaAccrued {
                        next_principal_krw, ..
                    },
                ..
            },
        ) => next_account_cash_krw == next_principal_krw,
        (
            CashSettlementPayload::CmaInterest(_),
            CashSettlementExecution::NoMovement {
                product_mutation:
                    CashProductMutation::CmaAccrued {
                        next_principal_krw, ..
                    },
                ..
            },
        ) => *next_principal_krw == current_account_cash_krw,
        (
            CashSettlementPayload::SavingsInstallment(_),
            CashSettlementExecution::Applied {
                next_account_cash_krw,
                product_mutation: CashProductMutation::SavingsInstallmentPaid { principal_krw, .. },
                ..
            },
        ) => current_account_cash_krw.checked_sub(*principal_krw) == Some(*next_account_cash_krw),
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(CashProductError::InvalidSettlementExecution)
    }
}

fn validate_follow_up(
    task: CashSettlementTask,
    game_day: u32,
    follow_up: Option<CashSettlementFollowUpDraft>,
) -> Result<(), CashProductError> {
    let expected = task.required_follow_up(game_day)?;
    if follow_up != expected || follow_up.is_some_and(|draft| draft.due_game_day <= game_day) {
        return Err(CashProductError::InvalidSettlementExecution);
    }
    Ok(())
}

fn validate_product_mutation(
    task: &CashSettlementTask,
    execution: &CashSettlementExecution,
) -> Result<(), CashProductError> {
    let valid = match (task.payload, execution) {
        (
            CashSettlementPayload::CmaInterest(_),
            CashSettlementExecution::Applied {
                product_mutation:
                    CashProductMutation::CmaAccrued {
                        next_principal_krw,
                        next_interest_remainder,
                    },
                ..
            },
        ) => {
            *next_principal_krw >= 0
                && (0..DAILY_INTEREST_DENOMINATOR).contains(next_interest_remainder)
        }
        (
            CashSettlementPayload::CmaInterest(_),
            CashSettlementExecution::NoMovement {
                reason,
                product_mutation:
                    CashProductMutation::CmaAccrued {
                        next_principal_krw,
                        next_interest_remainder,
                    },
                ..
            },
        ) => {
            matches!(
                reason,
                CashNoMovementReason::BelowMinimumBalance
                    | CashNoMovementReason::FractionalInterest
            ) && *next_principal_krw >= 0
                && (0..DAILY_INTEREST_DENOMINATOR).contains(next_interest_remainder)
        }
        (
            CashSettlementPayload::DepositMaturity(_),
            CashSettlementExecution::Applied {
                product_mutation: CashProductMutation::DepositMatured,
                ..
            },
        ) => true,
        (
            CashSettlementPayload::SavingsInstallment(payload),
            CashSettlementExecution::Applied {
                product_mutation:
                    CashProductMutation::SavingsInstallmentPaid {
                        installment_no,
                        principal_krw,
                    },
                ..
            },
        ) => payload.installment_no == *installment_no && *principal_krw > 0,
        (
            CashSettlementPayload::SavingsInstallment(payload),
            CashSettlementExecution::NoMovement {
                reason: CashNoMovementReason::InsufficientAccountCash,
                product_mutation: CashProductMutation::SavingsInstallmentMissed { installment_no },
                ..
            },
        ) => payload.installment_no == *installment_no,
        (
            CashSettlementPayload::SavingsMaturity(_),
            CashSettlementExecution::Applied {
                product_mutation: CashProductMutation::SavingsMatured,
                ..
            },
        ) => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(CashProductError::InvalidSettlementExecution)
    }
}

fn validate_applied_execution(
    game_day: u32,
    settlement_id: ResourceId,
    account_id: ResourceId,
    current_account_cash_krw: i64,
    next_account_cash_krw: i64,
    ledger: &LedgerTransaction,
) -> Result<(), CashProductError> {
    if next_account_cash_krw < 0 || next_account_cash_krw == current_account_cash_krw {
        return Err(CashProductError::InvalidSettlementExecution);
    }
    if ledger.game_day() != game_day
        || ledger.source().kind != LedgerSourceKind::ScheduledSettlement
        || ledger.source().source_id != settlement_id.to_string()
    {
        return Err(CashProductError::InvalidSettlementExecution);
    }

    let expected_delta = next_account_cash_krw
        .checked_sub(current_account_cash_krw)
        .ok_or(CashProductError::ArithmeticOverflow)?;
    let mut ledger_delta = 0_i128;
    for posting in ledger.postings() {
        if posting.account_code == LedgerAccountCode::AccountCash {
            if posting.financial_account_id != Some(account_id) {
                return Err(CashProductError::InvalidSettlementExecution);
            }
            ledger_delta = ledger_delta
                .checked_add(i128::from(posting.amount_krw))
                .ok_or(CashProductError::ArithmeticOverflow)?;
        }
    }
    if ledger_delta != i128::from(expected_delta) {
        return Err(CashProductError::InvalidSettlementExecution);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{RunId, create_finance_rules};
    use super::*;

    const SAVE_ID: ResourceId = ResourceId::from_u64(11);
    const ACCOUNT_ID: ResourceId = ResourceId::from_u64(17);
    const POLICY_SET_ID: ResourceId = ResourceId::from_u64(3);

    fn given_tax_policy() -> InterestTaxPolicy {
        InterestTaxPolicy {
            income_tax_rate_ppm: 140_000,
            local_income_tax_rate_ppm: 14_000,
        }
    }

    fn given_cash_product_policy() -> CashProductPolicy {
        CashProductPolicy {
            interest_tax: given_tax_policy(),
            deposit_protection: DepositProtectionPolicy {
                limit_krw: 100_000_000,
            },
        }
    }

    fn given_date(year: i32, month: Month, day: u8) -> Date {
        Date::from_calendar_date(year, month, day).expect("유효한 테스트 날짜여야 한다")
    }

    fn given_policy() -> RunPolicyContext {
        RunPolicyContext {
            run: RunId {
                save_id: SAVE_ID,
                run_revision: 4,
            },
            policy_set_id: POLICY_SET_ID,
        }
    }

    fn given_ledger_context(settlement_id: ResourceId, game_day: u32) -> SettlementLedgerContext {
        SettlementLedgerContext {
            policy: given_policy(),
            source: LedgerSource {
                kind: LedgerSourceKind::ScheduledSettlement,
                source_id: settlement_id.to_string(),
            },
            game_day,
            description: "현금성 상품 정산".to_owned(),
            account_id: ACCOUNT_ID,
        }
    }

    fn given_term_deposit() -> TermDepositContract {
        TermDepositContract {
            principal_krw: 1_000_000,
            annual_rate_bp: 250,
            early_close_rate_bp: 50,
            opened_game_day: 100,
            maturity_game_day: 465,
            day_count_basis: DayCountBasis::Actual365,
        }
    }

    fn given_savings_contract() -> InstallmentSavingsContract {
        InstallmentSavingsContract {
            annual_rate_bp: 100,
            early_close_rate_bp: 50,
            maturity_game_day: 10,
            day_count_basis: DayCountBasis::Actual365,
            paid_installments: vec![
                SavingsInstallmentPrincipal {
                    installment_no: 2,
                    principal_krw: 100_000,
                    paid_game_day: 9,
                },
                SavingsInstallmentPrincipal {
                    installment_no: 1,
                    principal_krw: 100_000,
                    paid_game_day: 9,
                },
            ],
        }
    }

    mod context_예적금_부모_계좌를_판단할_때 {
        use super::*;

        #[test]
        fn given_허용_계좌_when_세제_분류를_조회하면_then_다섯_계좌를_허용한다() {
            let account_types = [
                FinancialAccountType::TaxableBrokerage,
                FinancialAccountType::IsaGeneral,
                FinancialAccountType::IsaLowIncome,
                FinancialAccountType::PensionSavings,
                FinancialAccountType::Irp,
            ];

            let treatments = account_types.map(cash_product_tax_treatment);

            assert_eq!(
                treatments,
                [
                    Some(CashProductTaxTreatment::Taxable),
                    Some(CashProductTaxTreatment::Isa),
                    Some(CashProductTaxTreatment::Isa),
                    Some(CashProductTaxTreatment::Pension),
                    Some(CashProductTaxTreatment::Pension),
                ]
            );
        }

        #[test]
        fn given_cma와_금계좌_when_세제_분류를_조회하면_then_가입을_허용하지_않는다() {
            let account_types = [FinancialAccountType::Cma, FinancialAccountType::KrxGold];

            let treatments = account_types.map(cash_product_tax_treatment);

            assert_eq!(treatments, [None, None]);
        }
    }

    mod context_예적금_이자를_세제별로_처리할_때 {
        use super::*;

        #[test]
        fn given_일반계좌_when_이자처리를_계산하면_then_기존_원천징수와_금융소득을_유지한다() {
            let gross_interest_krw = 10_000;

            let treatment = calculate_cash_interest_treatment(
                gross_interest_krw,
                FinancialAccountType::TaxableBrokerage,
                given_tax_policy(),
            )
            .expect("일반계좌 이자 세제를 계산할 수 있어야 한다");

            assert_eq!(
                treatment,
                CashInterestTreatment {
                    tax_treatment: CashProductTaxTreatment::Taxable,
                    withholding: WithholdingTax {
                        gross_interest_krw,
                        income_tax_krw: 1_400,
                        local_income_tax_krw: 140,
                        net_interest_krw: 8_460,
                    },
                    financial_income_delta: FinancialIncomeDelta {
                        gross_financial_income_krw: gross_interest_krw,
                        withheld_income_tax_krw: 1_400,
                        withheld_local_income_tax_krw: 140,
                    },
                    tax_advantaged_interest_delta: TaxAdvantagedInterestDelta::None,
                }
            );
        }

        #[test]
        fn given_isa계좌_when_이자처리를_계산하면_then_원천징수_없이_isa이익만_늘린다() {
            let account_types = [
                FinancialAccountType::IsaGeneral,
                FinancialAccountType::IsaLowIncome,
            ];

            let treatments = account_types.map(|account_type| {
                calculate_cash_interest_treatment(10_000, account_type, given_tax_policy())
                    .expect("ISA 이자 세제를 계산할 수 있어야 한다")
            });

            assert_eq!(
                treatments,
                [
                    given_tax_advantaged_treatment(
                        CashProductTaxTreatment::Isa,
                        TaxAdvantagedInterestDelta::IsaTaxProfit { amount_krw: 10_000 },
                    ),
                    given_tax_advantaged_treatment(
                        CashProductTaxTreatment::Isa,
                        TaxAdvantagedInterestDelta::IsaTaxProfit { amount_krw: 10_000 },
                    ),
                ]
            );
        }

        #[test]
        fn given_연금계좌_when_이자처리를_계산하면_then_원천징수_없이_운용수익만_늘린다() {
            let account_types = [
                FinancialAccountType::PensionSavings,
                FinancialAccountType::Irp,
            ];

            let treatments = account_types.map(|account_type| {
                calculate_cash_interest_treatment(10_000, account_type, given_tax_policy())
                    .expect("연금 이자 세제를 계산할 수 있어야 한다")
            });

            assert_eq!(
                treatments,
                [
                    given_tax_advantaged_treatment(
                        CashProductTaxTreatment::Pension,
                        TaxAdvantagedInterestDelta::PensionEarnings { amount_krw: 10_000 },
                    ),
                    given_tax_advantaged_treatment(
                        CashProductTaxTreatment::Pension,
                        TaxAdvantagedInterestDelta::PensionEarnings { amount_krw: 10_000 },
                    ),
                ]
            );
        }

        #[test]
        fn given_이자가_0원인_isa계좌_when_이자처리를_계산하면_then_세금요약을_변경하지_않는다() {
            let gross_interest_krw = 0;

            let treatment = calculate_cash_interest_treatment(
                gross_interest_krw,
                FinancialAccountType::IsaGeneral,
                given_tax_policy(),
            )
            .expect("0원 ISA 이자 세제를 계산할 수 있어야 한다");

            assert_eq!(
                treatment,
                CashInterestTreatment {
                    tax_treatment: CashProductTaxTreatment::Isa,
                    withholding: WithholdingTax {
                        gross_interest_krw: 0,
                        income_tax_krw: 0,
                        local_income_tax_krw: 0,
                        net_interest_krw: 0,
                    },
                    financial_income_delta: FinancialIncomeDelta::ZERO,
                    tax_advantaged_interest_delta: TaxAdvantagedInterestDelta::None,
                }
            );
        }

        fn given_tax_advantaged_treatment(
            tax_treatment: CashProductTaxTreatment,
            tax_advantaged_interest_delta: TaxAdvantagedInterestDelta,
        ) -> CashInterestTreatment {
            CashInterestTreatment {
                tax_treatment,
                withholding: WithholdingTax {
                    gross_interest_krw: 10_000,
                    income_tax_krw: 0,
                    local_income_tax_krw: 0,
                    net_interest_krw: 10_000,
                },
                financial_income_delta: FinancialIncomeDelta::ZERO,
                tax_advantaged_interest_delta,
            }
        }
    }

    mod context_절세계좌_예적금을_정산할_때 {
        use super::*;

        #[test]
        fn given_isa_정기예금_when_만기정산하면_then_원금은_소득에서_제외하고_이자만_누계한다() {
            let contract = given_term_deposit();

            let payout = settle_term_deposit_maturity_for_account(
                contract,
                contract.maturity_game_day,
                FinancialAccountType::IsaGeneral,
                given_tax_policy(),
            )
            .expect("ISA 정기예금을 만기 정산할 수 있어야 한다");

            assert_eq!(
                (
                    payout.principal_krw,
                    payout.withholding,
                    payout.financial_income_delta,
                    payout.tax_advantaged_interest_delta,
                    payout.cash_payout_krw,
                ),
                (
                    1_000_000,
                    WithholdingTax {
                        gross_interest_krw: 25_000,
                        income_tax_krw: 0,
                        local_income_tax_krw: 0,
                        net_interest_krw: 25_000,
                    },
                    FinancialIncomeDelta::ZERO,
                    TaxAdvantagedInterestDelta::IsaTaxProfit { amount_krw: 25_000 },
                    1_025_000,
                )
            );
        }

        #[test]
        fn given_연금_정기예금_when_중도해지하면_then_세금없이_중도해지_이자만_운용수익이_된다() {
            let contract = given_term_deposit();

            let payout = settle_term_deposit_early_close_for_account(
                contract,
                200,
                FinancialAccountType::PensionSavings,
                given_tax_policy(),
            )
            .expect("연금 정기예금을 중도해지할 수 있어야 한다");

            assert_eq!(
                (
                    payout.withholding,
                    payout.financial_income_delta,
                    payout.tax_advantaged_interest_delta,
                    payout.cash_payout_krw,
                ),
                (
                    WithholdingTax {
                        gross_interest_krw: 1_369,
                        income_tax_krw: 0,
                        local_income_tax_krw: 0,
                        net_interest_krw: 1_369,
                    },
                    FinancialIncomeDelta::ZERO,
                    TaxAdvantagedInterestDelta::PensionEarnings { amount_krw: 1_369 },
                    1_001_369,
                )
            );
        }

        #[test]
        fn given_isa_정기적금_when_만기정산하면_then_납입원금은_소득이_아니고_이자만_누계한다() {
            let contract = given_savings_contract();

            let payout = settle_installment_savings_maturity_for_account(
                &contract,
                contract.maturity_game_day,
                FinancialAccountType::IsaLowIncome,
                given_tax_policy(),
            )
            .expect("ISA 정기적금을 만기 정산할 수 있어야 한다");

            assert_eq!(
                (
                    payout.principal_krw,
                    payout.withholding,
                    payout.financial_income_delta,
                    payout.tax_advantaged_interest_delta,
                    payout.cash_payout_krw,
                ),
                (
                    200_000,
                    WithholdingTax {
                        gross_interest_krw: 4,
                        income_tax_krw: 0,
                        local_income_tax_krw: 0,
                        net_interest_krw: 4,
                    },
                    FinancialIncomeDelta::ZERO,
                    TaxAdvantagedInterestDelta::IsaTaxProfit { amount_krw: 4 },
                    200_004,
                )
            );
        }

        #[test]
        fn given_irp_정기적금_when_중도해지하면_then_중도해지_이자만_운용수익이_된다() {
            let mut contract = given_savings_contract();
            for installment in &mut contract.paid_installments {
                installment.paid_game_day = 0;
            }

            let payout = settle_installment_savings_early_close_for_account(
                &contract,
                5,
                FinancialAccountType::Irp,
                given_tax_policy(),
            )
            .expect("IRP 정기적금을 중도해지할 수 있어야 한다");

            assert_eq!(
                (
                    payout.principal_krw,
                    payout.withholding,
                    payout.financial_income_delta,
                    payout.tax_advantaged_interest_delta,
                    payout.cash_payout_krw,
                ),
                (
                    200_000,
                    WithholdingTax {
                        gross_interest_krw: 12,
                        income_tax_krw: 0,
                        local_income_tax_krw: 0,
                        net_interest_krw: 12,
                    },
                    FinancialIncomeDelta::ZERO,
                    TaxAdvantagedInterestDelta::PensionEarnings { amount_krw: 12 },
                    200_012,
                )
            );
        }
    }

    fn given_deposit_task(id: u64, due_game_day: u32) -> CashSettlementTask {
        let contract_id = ResourceId::from_u64(100 + id);
        CashSettlementTask {
            id: ResourceId::from_u64(id),
            due_game_day,
            source: CashSettlementSource {
                kind: CashSettlementSourceKind::DepositContract,
                source_id: contract_id,
                occurrence: 0,
            },
            payload: CashSettlementPayload::DepositMaturity(DepositMaturityPayloadV1 {
                version: CashPayloadVersion::V1,
                account_id: ACCOUNT_ID,
                contract_id,
            }),
        }
    }

    fn given_cma_task(id: u64, due_game_day: u32, occurrence: u32) -> CashSettlementTask {
        CashSettlementTask {
            id: ResourceId::from_u64(id),
            due_game_day,
            source: CashSettlementSource {
                kind: CashSettlementSourceKind::CmaAccount,
                source_id: ACCOUNT_ID,
                occurrence,
            },
            payload: CashSettlementPayload::CmaInterest(CmaInterestPayloadV1 {
                version: CashPayloadVersion::V1,
                account_id: ACCOUNT_ID,
                cma_terms_id: ResourceId::from_u64(23),
            }),
        }
    }

    mod context_a_settlement_payload_is_decoded {
        use super::*;

        #[test]
        fn given_a_valid_cma_payload_when_decoded_then_ids_are_preserved() {
            let payload = serde_json::json!({
                "version": 1,
                "accountId": "17",
                "cmaTermsId": "23"
            });

            let decoded = CashSettlementPayload::decode(CashSettlementKind::CmaInterest, payload)
                .expect("CMA payload를 해석할 수 있어야 한다");

            assert_eq!(
                decoded,
                CashSettlementPayload::CmaInterest(CmaInterestPayloadV1 {
                    version: CashPayloadVersion::V1,
                    account_id: ACCOUNT_ID,
                    cma_terms_id: ResourceId::from_u64(23),
                })
            );
        }

        #[test]
        fn given_an_unknown_field_when_decoded_then_the_payload_is_rejected() {
            let payload = serde_json::json!({
                "version": 1,
                "accountId": "17",
                "contractId": "23",
                "amountKrw": 1000
            });

            let result =
                CashSettlementPayload::decode(CashSettlementKind::DepositMaturity, payload);

            assert_eq!(result, Err(CashProductError::InvalidPayload));
        }

        #[test]
        fn given_an_unknown_version_when_decoded_then_the_version_is_rejected() {
            let payload = serde_json::json!({
                "version": 2,
                "accountId": "17",
                "contractId": "23"
            });

            let result =
                CashSettlementPayload::decode(CashSettlementKind::DepositMaturity, payload);

            assert_eq!(result, Err(CashProductError::UnsupportedPayloadVersion));
        }

        #[test]
        fn given_a_payload_for_another_kind_when_decoded_then_the_combination_is_rejected() {
            let payload = serde_json::json!({
                "version": 1,
                "accountId": "17",
                "contractId": "23"
            });

            let result = CashSettlementPayload::decode(CashSettlementKind::CmaInterest, payload);

            assert_eq!(result, Err(CashProductError::InvalidPayload));
        }

        #[test]
        fn given_a_numeric_resource_id_when_decoded_then_the_payload_is_rejected() {
            let payload = serde_json::json!({
                "version": 1,
                "accountId": 17,
                "contractId": "23"
            });

            let result =
                CashSettlementPayload::decode(CashSettlementKind::DepositMaturity, payload);

            assert_eq!(result, Err(CashProductError::InvalidPayload));
        }

        #[test]
        fn given_installment_zero_when_decoded_then_the_payload_is_rejected() {
            let payload = serde_json::json!({
                "version": 1,
                "accountId": "17",
                "contractId": "23",
                "installmentNo": 0
            });

            let result =
                CashSettlementPayload::decode(CashSettlementKind::SavingsInstallment, payload);

            assert_eq!(result, Err(CashProductError::InvalidPayload));
        }
    }

    mod context_general_interest_tax_is_withheld {
        use super::*;

        #[test]
        fn given_ten_thousand_won_gross_when_withheld_then_both_taxes_are_floored_separately() {
            let tax = calculate_interest_withholding(10_000, given_tax_policy())
                .expect("원천세를 계산할 수 있어야 한다");

            assert_eq!(tax.income_tax_krw, 1_400);
            assert_eq!(tax.local_income_tax_krw, 140);
            assert_eq!(tax.net_interest_krw, 8_460);
        }

        #[test]
        fn given_seven_won_gross_when_withheld_then_combined_rounding_is_not_used() {
            let tax = calculate_interest_withholding(7, given_tax_policy())
                .expect("소액 원천세를 계산할 수 있어야 한다");

            assert_eq!(tax.income_tax_krw, 0);
            assert_eq!(tax.local_income_tax_krw, 0);
            assert_eq!(tax.net_interest_krw, 7);
        }

        #[test]
        fn given_negative_gross_when_withheld_then_it_is_rejected() {
            let result = calculate_interest_withholding(-1, given_tax_policy());

            assert_eq!(result, Err(CashProductError::InvalidMoney));
        }

        #[test]
        fn given_tax_rates_above_one_hundred_percent_when_withheld_then_policy_is_rejected() {
            let policy = InterestTaxPolicy {
                income_tax_rate_ppm: 900_000,
                local_income_tax_rate_ppm: 100_001,
            };

            let result = calculate_interest_withholding(10_000, policy);

            assert_eq!(result, Err(CashProductError::InvalidRate));
        }

        #[test]
        fn given_another_pinned_tax_policy_when_withheld_then_its_rates_are_used() {
            let policy = InterestTaxPolicy {
                income_tax_rate_ppm: 100_000,
                local_income_tax_rate_ppm: 10_000,
            };

            let tax = calculate_interest_withholding(10_000, policy)
                .expect("주입한 policy 세율을 적용할 수 있어야 한다");

            assert_eq!(tax.income_tax_krw, 1_000);
            assert_eq!(tax.local_income_tax_krw, 100);
            assert_eq!(tax.net_interest_krw, 8_900);
        }
    }

    mod context_a_settlement_source_is_validated {
        use super::*;

        #[test]
        fn given_a_cma_source_for_another_account_when_validated_then_it_is_rejected() {
            let mut task = given_cma_task(1, 10, 4);
            task.source.source_id = ResourceId::from_u64(99);

            let result = task.validate_source_identity();

            assert_eq!(result, Err(CashProductError::InvalidPayload));
        }

        #[test]
        fn given_a_cma_source_with_zero_occurrence_when_validated_then_it_is_rejected() {
            let task = given_cma_task(1, 10, 0);

            let result = task.validate_source_identity();

            assert_eq!(result, Err(CashProductError::InvalidPayload));
        }

        #[test]
        fn given_a_deposit_maturity_with_nonzero_occurrence_when_validated_then_it_is_rejected() {
            let mut task = given_deposit_task(1, 10);
            task.source.occurrence = 1;

            let result = task.validate_source_identity();

            assert_eq!(result, Err(CashProductError::InvalidPayload));
        }

        #[test]
        fn given_a_savings_installment_with_matching_occurrence_when_validated_then_it_is_accepted()
        {
            let contract_id = ResourceId::from_u64(71);
            let task = CashSettlementTask {
                id: ResourceId::from_u64(1),
                due_game_day: 10,
                source: CashSettlementSource {
                    kind: CashSettlementSourceKind::SavingsContract,
                    source_id: contract_id,
                    occurrence: 3,
                },
                payload: CashSettlementPayload::SavingsInstallment(SavingsInstallmentPayloadV1 {
                    version: CashPayloadVersion::V1,
                    account_id: ACCOUNT_ID,
                    contract_id,
                    installment_no: 3,
                }),
            };

            let result = task.validate_source_identity();

            assert_eq!(result, Ok(()));
        }
    }

    mod context_an_installment_savings_schedule_is_created {
        use super::*;

        fn when_schedule_is_created(
            opened_market_date: Date,
            opened_game_day: u32,
            term_months: u32,
            installment_count: u32,
        ) -> Result<InstallmentSavingsSchedule, CashProductError> {
            create_installment_savings_schedule(InstallmentSavingsScheduleInput {
                world_start_date: given_date(opened_market_date.year(), Month::January, 1),
                opened_market_date,
                opened_game_day,
                term_months,
                installment_count,
            })
        }

        #[test]
        fn given_january_29_when_scheduled_then_february_is_clamped_and_march_restores_29() {
            let schedule = when_schedule_is_created(given_date(2026, Month::January, 29), 28, 3, 3)
                .expect("29일 가입 일정을 만들 수 있어야 한다");

            assert_eq!(
                schedule
                    .installments
                    .iter()
                    .map(|installment| installment.due_date)
                    .collect::<Vec<_>>(),
                vec![
                    given_date(2026, Month::January, 29),
                    given_date(2026, Month::February, 28),
                    given_date(2026, Month::March, 29),
                ]
            );
            assert_eq!(schedule.maturity_date, given_date(2026, Month::April, 29));
        }

        #[test]
        fn given_january_30_when_scheduled_then_february_is_clamped_and_march_restores_30() {
            let schedule = when_schedule_is_created(given_date(2026, Month::January, 30), 29, 3, 3)
                .expect("30일 가입 일정을 만들 수 있어야 한다");

            assert_eq!(
                schedule.installments[1].due_date,
                given_date(2026, Month::February, 28)
            );
            assert_eq!(
                schedule.installments[2].due_date,
                given_date(2026, Month::March, 30)
            );
        }

        #[test]
        fn given_january_31_when_scheduled_then_each_month_clamps_from_the_original_day() {
            let schedule = when_schedule_is_created(given_date(2026, Month::January, 31), 30, 3, 3)
                .expect("31일 가입 일정을 만들 수 있어야 한다");

            assert_eq!(schedule.installments[0].due_game_day, 30);
            assert_eq!(
                schedule.installments[1].due_date,
                given_date(2026, Month::February, 28)
            );
            assert_eq!(
                schedule.installments[2].due_date,
                given_date(2026, Month::March, 31)
            );
            assert_eq!(schedule.maturity_date, given_date(2026, Month::April, 30));
            assert!(schedule.installments[2].due_game_day < schedule.maturity_game_day);
        }

        #[test]
        fn given_a_leap_year_january_31_when_scheduled_then_february_29_is_used() {
            let schedule = when_schedule_is_created(given_date(2028, Month::January, 31), 30, 2, 2)
                .expect("윤년 일정을 만들 수 있어야 한다");

            assert_eq!(
                schedule.installments[1].due_date,
                given_date(2028, Month::February, 29)
            );
            assert_eq!(schedule.maturity_date, given_date(2028, Month::March, 31));
        }

        #[test]
        fn given_an_opened_date_inconsistent_with_game_day_when_scheduled_then_it_is_rejected() {
            let input = InstallmentSavingsScheduleInput {
                world_start_date: given_date(2026, Month::January, 1),
                opened_market_date: given_date(2026, Month::January, 31),
                opened_game_day: 29,
                term_months: 12,
                installment_count: 12,
            };

            let result = create_installment_savings_schedule(input);

            assert_eq!(result, Err(CashProductError::InvalidGameDay));
        }

        #[test]
        fn given_installments_reaching_maturity_when_scheduled_then_it_is_rejected() {
            let result = when_schedule_is_created(given_date(2026, Month::January, 1), 0, 2, 3);

            assert_eq!(result, Err(CashProductError::InvalidSchedule));
        }

        #[test]
        fn given_a_date_beyond_supported_range_when_scheduled_then_it_is_rejected() {
            let input = InstallmentSavingsScheduleInput {
                world_start_date: Date::MAX,
                opened_market_date: Date::MAX,
                opened_game_day: 0,
                term_months: 1,
                installment_count: 1,
            };

            let result = create_installment_savings_schedule(input);

            assert_eq!(result, Err(CashProductError::InvalidGameDay));
        }

        #[test]
        fn given_an_overflowing_opened_game_day_when_scheduled_then_it_is_rejected() {
            let input = InstallmentSavingsScheduleInput {
                world_start_date: given_date(2026, Month::January, 1),
                opened_market_date: given_date(2026, Month::January, 1),
                opened_game_day: u32::MAX,
                term_months: 1,
                installment_count: 1,
            };

            let result = create_installment_savings_schedule(input);

            assert_eq!(result, Err(CashProductError::InvalidGameDay));
        }
    }

    mod context_a_term_deposit_is_settled {
        use super::*;

        #[test]
        fn given_a_mature_contract_when_settled_then_interest_and_tax_are_exact() {
            let payout =
                settle_term_deposit_maturity(given_term_deposit(), 465, given_tax_policy())
                    .expect("정기예금을 만기 정산할 수 있어야 한다");

            assert_eq!(payout.held_days, 365);
            assert_eq!(payout.withholding.gross_interest_krw, 25_000);
            assert_eq!(payout.withholding.income_tax_krw, 3_500);
            assert_eq!(payout.withholding.local_income_tax_krw, 350);
            assert_eq!(payout.cash_payout_krw, 1_021_150);
        }

        #[test]
        fn given_a_late_execution_when_settled_then_interest_stops_at_contract_maturity() {
            let payout =
                settle_term_deposit_maturity(given_term_deposit(), 500, given_tax_policy())
                    .expect("지연된 만기 정산을 계산할 수 있어야 한다");

            assert_eq!(payout.held_days, 365);
            assert_eq!(payout.withholding.gross_interest_krw, 25_000);
        }

        #[test]
        fn given_an_early_close_when_settled_then_the_fixed_early_rate_is_used_through_that_day() {
            let payout =
                settle_term_deposit_early_close(given_term_deposit(), 200, given_tax_policy())
                    .expect("정기예금을 중도해지할 수 있어야 한다");

            assert_eq!(payout.held_days, 100);
            assert_eq!(payout.withholding.gross_interest_krw, 1_369);
            assert_eq!(payout.withholding.income_tax_krw, 191);
            assert_eq!(payout.withholding.local_income_tax_krw, 19);
            assert_eq!(payout.cash_payout_krw, 1_001_159);
        }

        #[test]
        fn given_a_close_on_maturity_when_early_close_is_requested_then_it_is_rejected() {
            let result =
                settle_term_deposit_early_close(given_term_deposit(), 465, given_tax_policy());

            assert_eq!(result, Err(CashProductError::InvalidGameDay));
        }
    }

    mod context_installment_savings_are_settled {
        use super::*;

        #[test]
        fn given_two_paid_installments_when_matured_then_each_interest_is_floored_before_sum() {
            let payout = settle_installment_savings_maturity(
                &given_savings_contract(),
                10,
                given_tax_policy(),
            )
            .expect("정기적금을 만기 정산할 수 있어야 한다");

            assert_eq!(payout.installments[0].installment_no, 1);
            assert_eq!(payout.installments[0].gross_interest_krw, 2);
            assert_eq!(payout.installments[1].installment_no, 2);
            assert_eq!(payout.installments[1].gross_interest_krw, 2);
            assert_eq!(payout.withholding.gross_interest_krw, 4);
            assert_eq!(payout.principal_krw, 200_000);
        }

        #[test]
        fn given_paid_installments_when_closed_early_then_each_holding_period_uses_the_early_rate()
        {
            let contract = InstallmentSavingsContract {
                annual_rate_bp: 300,
                early_close_rate_bp: 50,
                maturity_game_day: 400,
                day_count_basis: DayCountBasis::Actual365,
                paid_installments: vec![
                    SavingsInstallmentPrincipal {
                        installment_no: 1,
                        principal_krw: 100_000,
                        paid_game_day: 0,
                    },
                    SavingsInstallmentPrincipal {
                        installment_no: 2,
                        principal_krw: 100_000,
                        paid_game_day: 100,
                    },
                ],
            };

            let payout = settle_installment_savings_early_close(&contract, 200, given_tax_policy())
                .expect("정기적금을 중도해지할 수 있어야 한다");

            assert_eq!(payout.installments[0].held_days, 200);
            assert_eq!(payout.installments[0].gross_interest_krw, 273);
            assert_eq!(payout.installments[1].held_days, 100);
            assert_eq!(payout.installments[1].gross_interest_krw, 136);
            assert_eq!(payout.withholding.gross_interest_krw, 409);
        }

        #[test]
        fn given_a_duplicate_installment_number_when_settled_then_it_is_rejected() {
            let mut contract = given_savings_contract();
            contract.paid_installments[1].installment_no = 2;

            let result = settle_installment_savings_maturity(&contract, 10, given_tax_policy());

            assert_eq!(result, Err(CashProductError::DuplicateInstallment));
        }
    }

    mod context_a_cma_accrues_daily_interest {
        use super::*;

        fn given_cma_input() -> CmaDailyAccrualInput {
            CmaDailyAccrualInput {
                principal_krw: 1_000_000,
                treasury_3m_bp: 250,
                interest_remainder: 0,
                terms: CmaDailyTerms {
                    spread_bp: -20,
                    minimum_interest_balance_krw: 10_000,
                    day_count_basis: DayCountBasis::Actual365,
                },
            }
        }

        #[test]
        fn given_an_eligible_balance_when_accrued_then_existing_daily_math_and_tax_are_applied() {
            let rules = create_finance_rules();

            let accrual = accrue_cma_daily(rules.as_ref(), given_cma_input(), given_tax_policy())
                .expect("CMA 일 이자를 계산할 수 있어야 한다");

            assert_eq!(accrual.outcome, CashSettlementOutcome::Applied);
            assert_eq!(accrual.annual_rate_bp, 230);
            assert_eq!(accrual.gross_interest_krw, 63);
            assert_eq!(accrual.withholding.income_tax_krw, 8);
            assert_eq!(accrual.withholding.local_income_tax_krw, 0);
            assert_eq!(accrual.reinvested_interest_krw, 55);
            assert_eq!(accrual.next_principal_krw, 1_000_055);
            assert_eq!(accrual.next_interest_remainder, 50_000);
        }

        #[test]
        fn given_a_fractional_interest_when_accrued_then_only_the_remainder_changes() {
            let rules = create_finance_rules();
            let mut input = given_cma_input();
            input.principal_krw = 10_000;
            input.treasury_3m_bp = 21;

            let accrual = accrue_cma_daily(rules.as_ref(), input, given_tax_policy())
                .expect("1원 미만 이자를 이월할 수 있어야 한다");

            assert_eq!(
                accrual.outcome,
                CashSettlementOutcome::NoMovement(CashNoMovementReason::FractionalInterest)
            );
            assert_eq!(accrual.next_principal_krw, 10_000);
            assert_eq!(accrual.next_interest_remainder, 10_000);
        }

        #[test]
        fn given_a_balance_below_the_minimum_when_accrued_then_the_remainder_is_unchanged() {
            let rules = create_finance_rules();
            let mut input = given_cma_input();
            input.principal_krw = 9_999;
            input.interest_remainder = 123;

            let accrual = accrue_cma_daily(rules.as_ref(), input, given_tax_policy())
                .expect("최소 잔액 미만을 정상 처리할 수 있어야 한다");

            assert_eq!(
                accrual.outcome,
                CashSettlementOutcome::NoMovement(CashNoMovementReason::BelowMinimumBalance)
            );
            assert_eq!(accrual.next_interest_remainder, 123);
        }

        #[test]
        fn given_a_negative_computed_rate_when_accrued_then_the_day_is_rejected() {
            let rules = create_finance_rules();
            let mut input = given_cma_input();
            input.treasury_3m_bp = 19;

            let result = accrue_cma_daily(rules.as_ref(), input, given_tax_policy());

            assert_eq!(result, Err(CashProductError::InvalidRate));
        }
    }

    mod context_a_savings_installment_is_collected {
        use super::*;

        #[test]
        fn given_enough_cash_when_collected_then_principal_moves_from_cash() {
            let collection = collect_savings_installment(100_000, 30_000)
                .expect("적금 회차를 납입할 수 있어야 한다");

            assert_eq!(collection.outcome, CashSettlementOutcome::Applied);
            assert_eq!(collection.next_account_cash_krw, 70_000);
            assert_eq!(collection.collected_principal_krw, 30_000);
        }

        #[test]
        fn given_insufficient_cash_when_collected_then_the_installment_is_no_movement() {
            let collection = collect_savings_installment(29_999, 30_000)
                .expect("잔액 부족 회차를 정상 처리할 수 있어야 한다");

            assert_eq!(
                collection.outcome,
                CashSettlementOutcome::NoMovement(CashNoMovementReason::InsufficientAccountCash)
            );
            assert_eq!(collection.next_account_cash_krw, 29_999);
            assert_eq!(collection.collected_principal_krw, 0);
        }
    }

    mod context_an_interest_ledger_is_created {
        use super::*;

        #[test]
        fn given_principal_interest_and_tax_when_created_then_the_validated_ledger_balances() {
            let rules = create_finance_rules();
            let withholding = calculate_interest_withholding(10_000, given_tax_policy())
                .expect("원천세를 계산할 수 있어야 한다");

            let ledger = create_interest_payout_ledger(
                rules.as_ref(),
                given_ledger_context(ResourceId::from_u64(31), 20),
                1_000_000,
                withholding,
                given_tax_policy(),
            )
            .expect("지급 원장을 만들 수 있어야 한다")
            .expect("돈이 움직이면 원장이 있어야 한다");

            assert_eq!(ledger.postings().len(), 4);
            assert_eq!(ledger.postings()[0].amount_krw, 1_008_460);
            assert_eq!(ledger.postings()[1].amount_krw, -1_000_000);
            assert_eq!(ledger.postings()[2].amount_krw, -10_000);
            assert_eq!(ledger.postings()[3].amount_krw, 1_540);
        }

        #[test]
        fn given_tax_below_one_won_when_created_then_no_zero_tax_posting_is_added() {
            let rules = create_finance_rules();
            let withholding = calculate_interest_withholding(7, given_tax_policy())
                .expect("소액 원천세를 계산할 수 있어야 한다");

            let ledger = create_interest_payout_ledger(
                rules.as_ref(),
                given_ledger_context(ResourceId::from_u64(32), 20),
                0,
                withholding,
                given_tax_policy(),
            )
            .expect("CMA 이자 원장을 만들 수 있어야 한다")
            .expect("이자가 움직이면 원장이 있어야 한다");

            assert_eq!(ledger.postings().len(), 2);
            assert!(
                ledger
                    .postings()
                    .iter()
                    .all(|posting| posting.amount_krw != 0)
            );
        }

        #[test]
        fn given_no_principal_or_interest_when_created_then_no_ledger_is_returned() {
            let rules = create_finance_rules();
            let withholding = calculate_interest_withholding(0, given_tax_policy())
                .expect("0원 세액을 계산할 수 있어야 한다");

            let ledger = create_interest_payout_ledger(
                rules.as_ref(),
                given_ledger_context(ResourceId::from_u64(33), 20),
                0,
                withholding,
                given_tax_policy(),
            )
            .expect("noMovement를 만들 수 있어야 한다");

            assert_eq!(ledger, None);
        }

        #[test]
        fn given_isa_원금과_이자_when_원장을_만들면_then_원천세없이_원금과_이자만_분개한다() {
            let rules = create_finance_rules();
            let withholding = calculate_cash_interest_treatment(
                10_000,
                FinancialAccountType::IsaGeneral,
                given_tax_policy(),
            )
            .expect("ISA 이자 세제를 계산할 수 있어야 한다")
            .withholding;

            let ledger = create_interest_payout_ledger_for_account(
                rules.as_ref(),
                given_ledger_context(ResourceId::from_u64(34), 20),
                1_000_000,
                withholding,
                FinancialAccountType::IsaGeneral,
                given_tax_policy(),
            )
            .expect("ISA 지급 원장을 만들 수 있어야 한다")
            .expect("원금과 이자가 움직이면 원장이 있어야 한다");

            assert_eq!(
                ledger
                    .postings()
                    .iter()
                    .map(|posting| (posting.account_code, posting.amount_krw))
                    .collect::<Vec<_>>(),
                vec![
                    (LedgerAccountCode::AccountCash, 1_010_000),
                    (LedgerAccountCode::ProductPrincipal, -1_000_000),
                    (LedgerAccountCode::InterestIncome, -10_000),
                ]
            );
        }
    }

    mod context_deposit_protection_is_aggregated {
        use super::*;

        #[test]
        fn given_multiple_institutions_when_aggregated_then_each_limit_is_applied_independently() {
            let deposits = vec![
                ProtectedDepositAmount {
                    institution_id: "life-bank-b".to_owned(),
                    principal_krw: 5_000_000,
                    prescribed_interest_krw: 0,
                },
                ProtectedDepositAmount {
                    institution_id: "life-bank-a".to_owned(),
                    principal_krw: 80_000_000,
                    prescribed_interest_krw: 1_000_000,
                },
                ProtectedDepositAmount {
                    institution_id: "life-bank-a".to_owned(),
                    principal_krw: 25_000_000,
                    prescribed_interest_krw: 0,
                },
            ];

            let summaries = aggregate_deposit_protection(
                &deposits,
                given_cash_product_policy().deposit_protection,
            )
            .expect("기관별 보호 금액을 집계할 수 있어야 한다");

            assert_eq!(summaries[0].institution_id, "life-bank-a");
            assert_eq!(summaries[0].eligible_amount_krw, 106_000_000);
            assert_eq!(summaries[0].protected_amount_krw, 100_000_000);
            assert_eq!(summaries[0].unprotected_amount_krw, 6_000_000);
            assert_eq!(summaries[1].institution_id, "life-bank-b");
            assert_eq!(summaries[1].protected_amount_krw, 5_000_000);
            assert_eq!(summaries[1].unprotected_amount_krw, 0);
        }

        #[test]
        fn given_an_overflowing_institution_total_when_aggregated_then_it_is_rejected() {
            let deposits = vec![
                ProtectedDepositAmount {
                    institution_id: "life-bank-a".to_owned(),
                    principal_krw: i64::MAX,
                    prescribed_interest_krw: 0,
                },
                ProtectedDepositAmount {
                    institution_id: "life-bank-a".to_owned(),
                    principal_krw: 1,
                    prescribed_interest_krw: 0,
                },
            ];

            let result = aggregate_deposit_protection(
                &deposits,
                given_cash_product_policy().deposit_protection,
            );

            assert_eq!(result, Err(CashProductError::ArithmeticOverflow));
        }

        #[test]
        fn given_a_negative_policy_limit_when_aggregated_then_policy_is_rejected() {
            let policy = DepositProtectionPolicy { limit_krw: -1 };

            let result = aggregate_deposit_protection(&[], policy);

            assert_eq!(result, Err(CashProductError::InvalidMoney));
        }

        #[test]
        fn given_another_pinned_limit_when_aggregated_then_that_limit_is_used() {
            let deposits = vec![ProtectedDepositAmount {
                institution_id: "life-bank-a".to_owned(),
                principal_krw: 60_000_000,
                prescribed_interest_krw: 1_000_000,
            }];
            let policy = DepositProtectionPolicy {
                limit_krw: 50_000_000,
            };

            let summaries = aggregate_deposit_protection(&deposits, policy)
                .expect("주입한 보호 한도를 적용할 수 있어야 한다");

            assert_eq!(summaries[0].protected_amount_krw, 50_000_000);
            assert_eq!(summaries[0].unprotected_amount_krw, 11_000_000);
        }
    }

    struct TestSettlementRegistry {
        fail_on_id: Option<ResourceId>,
        invalid_source: bool,
    }

    impl CashSettlementExecutorRegistry for TestSettlementRegistry {
        fn execute(
            &self,
            task: &CashSettlementTask,
            game_day: u32,
            current_account_cash_krw: i64,
            _account_type: FinancialAccountType,
            policy: CashProductPolicy,
        ) -> Result<CashSettlementExecution, CashProductError> {
            if self.fail_on_id == Some(task.id) {
                return Err(CashProductError::InvalidRate);
            }
            let principal_krw =
                i64::try_from(task.id.get()).map_err(|_| CashProductError::ArithmeticOverflow)?;
            let mut context = given_ledger_context(task.id, game_day);
            if self.invalid_source {
                context.source.source_id = "wrong-source".to_owned();
            }
            let ledger = create_interest_payout_ledger(
                create_finance_rules().as_ref(),
                context,
                principal_krw,
                calculate_interest_withholding(0, policy.interest_tax)?,
                policy.interest_tax,
            )?
            .ok_or(CashProductError::InvalidSettlementExecution)?;
            let next_account_cash_krw = current_account_cash_krw
                .checked_add(principal_krw)
                .ok_or(CashProductError::ArithmeticOverflow)?;
            Ok(CashSettlementExecution::Applied {
                next_account_cash_krw,
                ledger,
                product_mutation: CashProductMutation::DepositMatured,
                financial_income_delta: FinancialIncomeDelta::ZERO,
                follow_up: None,
            })
        }
    }

    fn given_planner(
        fail_on_id: Option<ResourceId>,
        invalid_source: bool,
    ) -> Arc<dyn CashSettlementPlanner> {
        create_cash_settlement_planner(Arc::new(TestSettlementRegistry {
            fail_on_id,
            invalid_source,
        }))
    }

    fn given_plan_input(settlements: Vec<CashSettlementTask>) -> DailyCashSettlementPlanInput {
        DailyCashSettlementPlanInput {
            game_day: 10,
            policy: given_cash_product_policy(),
            account_cash_by_id: BTreeMap::from([(ACCOUNT_ID, 100)]),
            account_type_by_id: BTreeMap::from([(
                ACCOUNT_ID,
                FinancialAccountType::TaxableBrokerage,
            )]),
            settlements,
        }
    }

    struct CmaSettlementRegistry {
        omit_follow_up: bool,
    }

    impl CashSettlementExecutorRegistry for CmaSettlementRegistry {
        fn execute(
            &self,
            task: &CashSettlementTask,
            game_day: u32,
            current_account_cash_krw: i64,
            _account_type: FinancialAccountType,
            _policy: CashProductPolicy,
        ) -> Result<CashSettlementExecution, CashProductError> {
            let follow_up = if self.omit_follow_up {
                None
            } else {
                task.required_follow_up(game_day)?
            };
            Ok(CashSettlementExecution::NoMovement {
                reason: CashNoMovementReason::FractionalInterest,
                product_mutation: CashProductMutation::CmaAccrued {
                    next_principal_krw: current_account_cash_krw,
                    next_interest_remainder: 10,
                },
                financial_income_delta: FinancialIncomeDelta::ZERO,
                follow_up,
            })
        }
    }

    struct InterestSettlementRegistry {
        corrupt_delta: bool,
    }

    impl CashSettlementExecutorRegistry for InterestSettlementRegistry {
        fn execute(
            &self,
            task: &CashSettlementTask,
            game_day: u32,
            current_account_cash_krw: i64,
            account_type: FinancialAccountType,
            policy: CashProductPolicy,
        ) -> Result<CashSettlementExecution, CashProductError> {
            let interest =
                calculate_cash_interest_treatment(10_000, account_type, policy.interest_tax)?;
            let ledger = create_interest_payout_ledger_for_account(
                create_finance_rules().as_ref(),
                given_ledger_context(task.id, game_day),
                1,
                interest.withholding,
                account_type,
                policy.interest_tax,
            )?
            .ok_or(CashProductError::InvalidSettlementExecution)?;
            let mut financial_income_delta = interest.financial_income_delta;
            if self.corrupt_delta {
                financial_income_delta.withheld_local_income_tax_krw = financial_income_delta
                    .withheld_local_income_tax_krw
                    .checked_add(1)
                    .ok_or(CashProductError::ArithmeticOverflow)?;
            }
            let next_account_cash_krw = current_account_cash_krw
                .checked_add(1)
                .and_then(|value| value.checked_add(interest.withholding.net_interest_krw))
                .ok_or(CashProductError::ArithmeticOverflow)?;
            Ok(CashSettlementExecution::Applied {
                next_account_cash_krw,
                ledger,
                product_mutation: CashProductMutation::DepositMatured,
                financial_income_delta,
                follow_up: None,
            })
        }
    }

    mod context_daily_cash_settlements_are_planned {
        use super::*;

        #[test]
        fn given_unsorted_due_tasks_when_planned_then_order_and_running_balance_are_folded() {
            let planner = given_planner(None, false);
            let input = given_plan_input(vec![
                given_deposit_task(3, 9),
                given_deposit_task(2, 8),
                given_deposit_task(1, 8),
            ]);

            let plan = planner
                .plan(input)
                .expect("하루 정산을 계획할 수 있어야 한다");

            let ids = plan
                .settlements
                .iter()
                .map(|settlement| settlement.settlement_id.get())
                .collect::<Vec<_>>();
            assert_eq!(ids, vec![1, 2, 3]);
            assert_eq!(plan.account_cash_by_id[&ACCOUNT_ID], 106);
        }

        #[test]
        fn given_a_later_execution_failure_when_planned_then_no_partial_plan_is_returned() {
            let planner = given_planner(Some(ResourceId::from_u64(2)), false);
            let input = given_plan_input(vec![given_deposit_task(1, 8), given_deposit_task(2, 9)]);

            let result = planner.plan(input);

            assert_eq!(result, Err(CashProductError::InvalidRate));
        }

        #[test]
        fn given_duplicate_ids_when_planned_then_the_day_is_rejected_before_execution() {
            let planner = given_planner(None, false);
            let input = given_plan_input(vec![given_deposit_task(1, 8), given_deposit_task(1, 9)]);

            let result = planner.plan(input);

            assert_eq!(result, Err(CashProductError::DuplicateSettlement));
        }

        #[test]
        fn given_duplicate_source_identity_when_planned_then_the_day_is_rejected_before_execution()
        {
            let planner = given_planner(None, false);
            let first = given_deposit_task(1, 8);
            let mut second = given_deposit_task(2, 9);
            second.source = first.source;
            second.payload = first.payload;
            let input = given_plan_input(vec![first, second]);

            let result = planner.plan(input);

            assert_eq!(result, Err(CashProductError::DuplicateSettlement));
        }

        #[test]
        fn given_a_future_task_when_planned_then_the_day_is_rejected() {
            let planner = given_planner(None, false);
            let input = given_plan_input(vec![given_deposit_task(1, 11)]);

            let result = planner.plan(input);

            assert_eq!(result, Err(CashProductError::FutureSettlement));
        }

        #[test]
        fn given_a_ledger_with_another_source_when_planned_then_execution_is_rejected() {
            let planner = given_planner(None, true);
            let input = given_plan_input(vec![given_deposit_task(1, 8)]);

            let result = planner.plan(input);

            assert_eq!(result, Err(CashProductError::InvalidSettlementExecution));
        }

        #[test]
        fn given_a_cma_task_when_planned_then_exactly_the_next_daily_occurrence_is_drafted() {
            let planner = create_cash_settlement_planner(Arc::new(CmaSettlementRegistry {
                omit_follow_up: false,
            }));
            let mut input = given_plan_input(vec![given_cma_task(1, 10, 4)]);
            input
                .account_type_by_id
                .insert(ACCOUNT_ID, FinancialAccountType::Cma);

            let plan = planner
                .plan(input)
                .expect("CMA 다음 정산을 계획할 수 있어야 한다");

            let follow_up = plan.settlements[0]
                .follow_up
                .expect("CMA에는 다음 날 정산이 있어야 한다");
            assert_eq!(follow_up.due_game_day, 11);
            assert_eq!(follow_up.source.occurrence, 5);
            assert_eq!(follow_up.source.source_id, ACCOUNT_ID);
            assert_eq!(follow_up.payload, given_cma_task(1, 10, 4).payload);
        }

        #[test]
        fn given_a_cma_execution_without_follow_up_when_planned_then_execution_is_rejected() {
            let planner = create_cash_settlement_planner(Arc::new(CmaSettlementRegistry {
                omit_follow_up: true,
            }));
            let mut input = given_plan_input(vec![given_cma_task(1, 10, 4)]);
            input
                .account_type_by_id
                .insert(ACCOUNT_ID, FinancialAccountType::Cma);

            let result = planner.plan(input);

            assert_eq!(result, Err(CashProductError::InvalidSettlementExecution));
        }

        #[test]
        fn given_interest_payment_when_planned_then_gross_and_each_withheld_tax_are_preserved() {
            let planner = create_cash_settlement_planner(Arc::new(InterestSettlementRegistry {
                corrupt_delta: false,
            }));
            let input = given_plan_input(vec![given_deposit_task(1, 10)]);

            let plan = planner
                .plan(input)
                .expect("금융소득 누계 변경을 계획할 수 있어야 한다");

            assert_eq!(
                plan.settlements[0].financial_income_delta,
                FinancialIncomeDelta {
                    gross_financial_income_krw: 10_000,
                    withheld_income_tax_krw: 1_400,
                    withheld_local_income_tax_krw: 140,
                }
            );
        }

        #[test]
        fn given_isa_예금만기_when_계획하면_then_금융소득없이_isa이익_delta를_남긴다() {
            let planner = create_cash_settlement_planner(Arc::new(InterestSettlementRegistry {
                corrupt_delta: false,
            }));
            let mut input = given_plan_input(vec![given_deposit_task(1, 10)]);
            input
                .account_type_by_id
                .insert(ACCOUNT_ID, FinancialAccountType::IsaGeneral);

            let plan = planner
                .plan(input)
                .expect("ISA 예금 만기 정산을 계획할 수 있어야 한다");

            assert_eq!(
                (
                    plan.settlements[0].financial_income_delta,
                    plan.settlements[0].tax_advantaged_interest_delta,
                ),
                (
                    FinancialIncomeDelta::ZERO,
                    TaxAdvantagedInterestDelta::IsaTaxProfit { amount_krw: 10_000 },
                )
            );
        }

        #[test]
        fn given_a_delta_inconsistent_with_policy_when_planned_then_execution_is_rejected() {
            let planner = create_cash_settlement_planner(Arc::new(InterestSettlementRegistry {
                corrupt_delta: true,
            }));
            let input = given_plan_input(vec![given_deposit_task(1, 10)]);

            let result = planner.plan(input);

            assert_eq!(result, Err(CashProductError::InvalidSettlementExecution));
        }
    }
}
