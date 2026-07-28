use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A lower-case, hyphenated UUID used as an idempotent command identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, utoipa::ToSchema)]
#[schema(
    value_type = String,
    format = "uuid",
    pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)]
pub struct CommandId(String);

impl CommandId {
    pub fn parse(raw: impl Into<String>) -> Result<Self, FinanceRuleError> {
        let raw = raw.into();
        if is_canonical_uuid(&raw) {
            Ok(Self(raw))
        } else {
            Err(FinanceRuleError::InvalidCommandId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for CommandId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for CommandId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CommandId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(raw).map_err(serde::de::Error::custom)
    }
}

/// A MySQL `BIGINT UNSIGNED` identifier represented as a decimal JSON string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, utoipa::ToSchema)]
#[schema(
    value_type = String,
    pattern = "^[1-9][0-9]*$"
)]
pub struct ResourceId(u64);

impl ResourceId {
    pub fn parse(raw: &str) -> Result<Self, FinanceRuleError> {
        if raw.is_empty() || raw == "0" || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(FinanceRuleError::InvalidResourceId);
        }

        let value = raw
            .parse::<u64>()
            .map_err(|_| FinanceRuleError::InvalidResourceId)?;
        if value.to_string() != raw {
            return Err(FinanceRuleError::InvalidResourceId);
        }

        Ok(Self(value))
    }

    pub const fn from_u64(value: u64) -> Self {
        assert!(value != 0, "resource ID must be positive");
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl Display for ResourceId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl Serialize for ResourceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for ResourceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunId {
    pub save_id: ResourceId,
    pub run_revision: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunPolicyContext {
    pub run: RunId,
    pub policy_set_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicySet {
    pub id: ResourceId,
    pub key: String,
    pub basis_date: String,
    pub sealed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicySetAssignment {
    pub policy_set_id: ResourceId,
    pub assignment_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FinancialAccountType {
    TaxableBrokerage,
    Cma,
    IsaGeneral,
    IsaLowIncome,
    PensionSavings,
    Irp,
    KrxGold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FinancialAccountStatus {
    Open,
    Matured,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinancialAccount {
    pub id: ResourceId,
    pub run: RunId,
    pub account_type: FinancialAccountType,
    pub status: FinancialAccountStatus,
    pub is_default: bool,
    pub cash_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LedgerAccountCode {
    Wallet,
    AccountCash,
    ProductPrincipal,
    DebtPrincipal,
    OpeningEquity,
    WithholdingTaxLiability,
    InterestIncome,
    FeeExpense,
    DistributionIncome,
    RealizedGainLoss,
    TaxSettlement,
    CareerDevelopmentExpense,
    SalaryIncome,
    EmployeeNationalPensionExpense,
    EmployeeHealthInsuranceExpense,
    EmployeeLongTermCareExpense,
    EmployeeEmploymentInsuranceExpense,
    EmploymentIncomeTaxWithholding,
    EmploymentLocalIncomeTaxWithholding,
    OtherIncomeReward,
    OtherIncomeTaxWithholding,
    OtherLocalIncomeTaxWithholding,
    PensionTaxExcludedContribution,
    PensionCreditedContribution,
    MilitaryPayIncome,
    MilitarySavingsPrincipal,
    MilitarySavingsBankInterest,
    MilitarySavingsGovernmentMatchIncome,
    LivingCostExpense,
    EssentialArrearLiability,
    LoanPrincipalLiability,
    LoanInterestExpense,
    LoanInterestLiability,
    LoanFeeExpense,
    TaxObligationLiability,
    LeaseDepositAsset,
    MovingExpense,
    LeaseRentExpense,
    LeaseArrearLiability,
    PropertyAsset,
    AcquisitionIncidentalExpense,
    PropertyDispositionExpense,
    PropertyTaxExpense,
    WelfareBenefitIncome,
    LifeEventExpense,
    InsurancePremiumExpense,
    InsuranceClaimRecovery,
    InsolvencyDischargedDebt,
    InsolvencyDischargeGain,
    CorporationInvestmentAsset,
    CorporationRegistrationExpense,
}

impl LedgerAccountCode {
    pub const fn account_requirement(self) -> PostingAccountRequirement {
        match self {
            Self::AccountCash | Self::ProductPrincipal => PostingAccountRequirement::Required,
            Self::Wallet
            | Self::DebtPrincipal
            | Self::OpeningEquity
            | Self::WithholdingTaxLiability
            | Self::InterestIncome
            | Self::FeeExpense
            | Self::DistributionIncome
            | Self::RealizedGainLoss
            | Self::TaxSettlement
            | Self::CareerDevelopmentExpense
            | Self::SalaryIncome
            | Self::EmployeeNationalPensionExpense
            | Self::EmployeeHealthInsuranceExpense
            | Self::EmployeeLongTermCareExpense
            | Self::EmployeeEmploymentInsuranceExpense
            | Self::EmploymentIncomeTaxWithholding
            | Self::EmploymentLocalIncomeTaxWithholding
            | Self::OtherIncomeReward
            | Self::OtherIncomeTaxWithholding
            | Self::OtherLocalIncomeTaxWithholding
            | Self::MilitaryPayIncome
            | Self::MilitarySavingsPrincipal
            | Self::MilitarySavingsBankInterest
            | Self::MilitarySavingsGovernmentMatchIncome
            | Self::LivingCostExpense
            | Self::EssentialArrearLiability
            | Self::LoanPrincipalLiability
            | Self::LoanInterestExpense
            | Self::LoanInterestLiability
            | Self::LoanFeeExpense
            | Self::TaxObligationLiability
            | Self::LeaseDepositAsset
            | Self::MovingExpense
            | Self::LeaseRentExpense
            | Self::LeaseArrearLiability
            | Self::PropertyAsset
            | Self::AcquisitionIncidentalExpense
            | Self::PropertyDispositionExpense
            | Self::PropertyTaxExpense
            | Self::WelfareBenefitIncome
            | Self::LifeEventExpense
            | Self::InsurancePremiumExpense
            | Self::InsuranceClaimRecovery
            | Self::InsolvencyDischargedDebt
            | Self::InsolvencyDischargeGain
            | Self::CorporationInvestmentAsset
            | Self::CorporationRegistrationExpense => PostingAccountRequirement::Forbidden,
            Self::PensionTaxExcludedContribution | Self::PensionCreditedContribution => {
                PostingAccountRequirement::Required
            }
        }
    }

    pub const fn is_military_savings(self) -> bool {
        matches!(
            self,
            Self::MilitarySavingsPrincipal
                | Self::MilitarySavingsBankInterest
                | Self::MilitarySavingsGovernmentMatchIncome
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostingAccountRequirement {
    Required,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LedgerSourceKind {
    M2OpeningBalance,
    Transfer,
    Trade,
    CashProductEnrollment,
    CashProductClose,
    IsaClose,
    PensionWithdrawal,
    InterestAccrual,
    ScheduledSettlement,
    SpecActivity,
    EmploymentPayroll,
    CareerRewardPayment,
    PensionCreditAllocation,
    MilitaryPay,
    MilitarySavingsInstallment,
    MilitarySavingsMaturity,
    MilitarySavingsGovernmentMatch,
    MilitarySavingsEarlyClose,
    LivingCostMonth,
    EssentialArrearPayment,
    LoanOrigination,
    LoanInstallment,
    LoanPrepayment,
    DebtAuthorityBridge,
    LeaseMove,
    LeaseRent,
    LeaseArrearPayment,
    PropertyPurchase,
    PropertySale,
    PropertyTaxPayment,
    WelfareBenefitPayment,
    LifeEventChoice,
    InsurancePremiumPayment,
    InsuranceClaimPayment,
    InsolvencyDistribution,
    InsolvencyDischarge,
    CorporationEstablishment,
    Correction,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LedgerSource {
    pub kind: LedgerSourceKind,
    pub source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LedgerPosting {
    pub account_code: LedgerAccountCode,
    pub financial_account_id: Option<ResourceId>,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerTransactionDraft {
    pub policy: RunPolicyContext,
    pub source: LedgerSource,
    pub game_day: u32,
    pub description: String,
    pub postings: Vec<LedgerPosting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerPostingOwner {
    MilitarySavingsContract(ResourceId),
}

/// A transaction that passed all double-entry ledger invariants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerTransaction {
    policy: RunPolicyContext,
    source: LedgerSource,
    game_day: u32,
    description: String,
    postings: Vec<LedgerPosting>,
    #[serde(skip)]
    posting_owner: Option<LedgerPostingOwner>,
}

impl LedgerTransaction {
    pub(crate) fn from_validated(draft: LedgerTransactionDraft) -> Self {
        Self {
            policy: draft.policy,
            source: draft.source,
            game_day: draft.game_day,
            description: draft.description,
            postings: draft.postings,
            posting_owner: None,
        }
    }

    pub(crate) fn with_posting_owner(
        mut self,
        owner: LedgerPostingOwner,
    ) -> Result<Self, FinanceRuleError> {
        let has_owned_posting = self
            .postings
            .iter()
            .any(|posting| posting.account_code.is_military_savings());
        if !has_owned_posting {
            return Err(FinanceRuleError::PostingOwnerForbidden);
        }
        self.posting_owner = Some(owner);
        Ok(self)
    }

    pub const fn policy(&self) -> RunPolicyContext {
        self.policy
    }

    pub fn source(&self) -> &LedgerSource {
        &self.source
    }

    pub const fn game_day(&self) -> u32 {
        self.game_day
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn postings(&self) -> &[LedgerPosting] {
        &self.postings
    }

    pub const fn posting_owner(&self) -> Option<LedgerPostingOwner> {
        self.posting_owner
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TransferDirection {
    WalletToAccount,
    AccountToWallet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferInput {
    pub policy: RunPolicyContext,
    pub command_id: CommandId,
    pub game_day: u32,
    pub wallet_cash_krw: i64,
    pub account: FinancialAccount,
    pub direction: TransferDirection,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferMutation {
    pub wallet_cash_krw: i64,
    pub account: FinancialAccount,
    pub ledger: LedgerTransaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyInterestInput {
    pub principal_krw: i64,
    pub annual_rate_bp: i32,
    pub remainder: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyInterestAccrual {
    pub interest_krw: i64,
    pub remainder: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum SettlementKind {
    CmaInterest,
    DepositMaturity,
    SavingsInstallment,
    SavingsMaturity,
    BondCoupon,
    BondMaturity,
    LlxDistribution,
    FinancialIncomeFiling,
    EmploymentPayroll,
    EmploymentReconciliation,
    MilitaryPay,
    MilitarySavingsInstallment,
    MilitarySavingsMaturity,
    MilitarySavingsGovernmentMatch,
    LoanInstallment,
    LeaseRent,
    LivingCostMonth,
    PropertyTaxPayment,
    WelfareBenefitPayment,
    InsurancePremium,
}

impl SettlementKind {
    /// Orders due obligations without changing the historical order inside pre-M4 settlements.
    pub const fn phase_rank(self) -> u16 {
        match self {
            Self::LoanInstallment => 200,
            Self::LeaseRent => 300,
            Self::LivingCostMonth => 400,
            Self::PropertyTaxPayment => 100,
            Self::WelfareBenefitPayment => 150,
            Self::InsurancePremium => 250,
            Self::CmaInterest
            | Self::DepositMaturity
            | Self::SavingsInstallment
            | Self::SavingsMaturity
            | Self::BondCoupon
            | Self::BondMaturity
            | Self::LlxDistribution
            | Self::FinancialIncomeFiling
            | Self::EmploymentPayroll
            | Self::EmploymentReconciliation
            | Self::MilitaryPay
            | Self::MilitarySavingsInstallment
            | Self::MilitarySavingsMaturity
            | Self::MilitarySavingsGovernmentMatch => 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettlementStatus {
    Pending,
    Settled,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettlementSourceKind {
    CmaAccount,
    DepositContract,
    SavingsContract,
    BondPosition,
    IndexPosition,
    TaxYear,
    EmploymentContract,
    YearEndTaxAssessment,
    MilitaryService,
    MilitarySavingsContract,
    MilitarySavingsInstallment,
    LoanContract,
    LeaseContract,
    LivingCostMonth,
    PropertyTaxEvent,
    WelfarePayment,
    InsuranceContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettlementSource {
    pub kind: SettlementSourceKind,
    pub source_id: String,
    pub occurrence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScheduledSettlement {
    pub id: ResourceId,
    pub run: RunId,
    pub due_game_day: u32,
    pub kind: SettlementKind,
    pub source: SettlementSource,
    pub status: SettlementStatus,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandCursor {
    pub expected_run_revision: u32,
    pub expected_state_revision: u64,
    pub expected_game_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_id: ResourceId,
    pub direction: TransferDirection,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferReceipt {
    pub command_id: CommandId,
    pub account_id: ResourceId,
    pub direction: TransferDirection,
    pub amount_krw: i64,
    pub run_revision: u32,
    pub state_revision: u64,
    pub game_day: u32,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerPostingRecord {
    pub account_code: LedgerAccountCode,
    pub financial_account_id: Option<ResourceId>,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerRecord {
    pub id: ResourceId,
    pub game_day: u32,
    pub description: String,
    pub source_kind: LedgerSourceKind,
    pub postings: Vec<LedgerPostingRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerPage {
    pub transactions: Vec<LedgerRecord>,
    pub next_before: Option<ResourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinanceCommandKind {
    OpenAccount,
    CloseAccount,
    Transfer,
    OpenDeposit,
    CloseDeposit,
    PlaceBondOrder,
    PlaceGoldOrder,
    CloseIsa,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinanceCommand<T> {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub kind: FinanceCommandKind,
    pub payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FinanceFailureCode {
    InvalidCommand,
    CharacterRequired,
    AccountNotFound,
    AccountClosed,
    AccountTypeNotAllowed,
    AccountNotEmpty,
    AccountAlreadyExists,
    InsufficientWalletCash,
    InsufficientAccountCash,
    PolicyNotEligible,
    LimitExceeded,
    SettlementConflict,
    IdempotencyConflict,
    Busy,
    ProductNotFound,
    ContractNotFound,
    ContractClosed,
    RateUnavailable,
    MarketClosed,
    InsufficientQuantity,
    PositionLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CashProductKind {
    CmaRp,
    CmaIssuedNote,
    TermDeposit,
    InstallmentSavings,
}

impl CashProductKind {
    pub const fn is_cma(self) -> bool {
        matches!(self, Self::CmaRp | Self::CmaIssuedNote)
    }

    pub const fn is_deposit(self) -> bool {
        matches!(self, Self::TermDeposit | Self::InstallmentSavings)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CashRateReference {
    Treasury3mBp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinancialInstitution {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashProductVersion {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
    pub kind: CashProductKind,
    pub institution: FinancialInstitution,
    pub protection_eligible: bool,
    pub rate_reference: CashRateReference,
    pub spread_bp: i32,
    pub minimum_interest_balance_krw: Option<i64>,
    pub minimum_contribution_krw: Option<i64>,
    pub maximum_contribution_krw: Option<i64>,
    pub term_days: Option<u32>,
    pub term_months: Option<u32>,
    pub installment_count: Option<u32>,
    pub early_termination_rate_bp: Option<i32>,
    pub day_count_denominator: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashProductCatalog {
    pub products: Vec<CashProductVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCmaAccountCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub product_version_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseCmaAccountCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCashProductCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub kind: CashProductKind,
    pub product_version_id: ResourceId,
    pub settlement_account_id: ResourceId,
    pub amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseCashProductCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub contract_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenCmaAccountReceipt {
    pub command_id: CommandId,
    pub account_id: ResourceId,
    pub product_version_id: ResourceId,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseCmaAccountReceipt {
    pub command_id: CommandId,
    pub account_id: ResourceId,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenCashProductReceipt {
    pub command_id: CommandId,
    pub contract_id: ResourceId,
    pub product_version_id: ResourceId,
    pub settlement_account_id: ResourceId,
    pub kind: CashProductKind,
    pub amount_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseCashProductReceipt {
    pub command_id: CommandId,
    pub contract_id: ResourceId,
    pub gross_interest_krw: i64,
    pub income_tax_krw: i64,
    pub local_income_tax_krw: i64,
    pub net_payout_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmaAccountContractState {
    pub account_id: ResourceId,
    pub product_version_id: ResourceId,
    pub annual_rate_bp: Option<i32>,
    pub minimum_interest_balance_krw: i64,
    pub interest_remainder: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CashProductContractStatus {
    Active,
    Matured,
    ClosedEarly,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SavingsInstallmentStatus {
    Pending,
    Paid,
    Missed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavingsInstallmentState {
    pub installment_no: u32,
    pub due_game_day: u32,
    pub amount_krw: i64,
    pub status: SavingsInstallmentStatus,
    pub processed_game_day: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashProductContractState {
    pub contract_id: ResourceId,
    pub product_version_id: ResourceId,
    pub settlement_account_id: ResourceId,
    pub kind: CashProductKind,
    pub status: CashProductContractStatus,
    pub installment_amount_krw: Option<i64>,
    pub annual_rate_bp: i32,
    pub current_principal_krw: i64,
    pub opened_game_day: u32,
    pub maturity_game_day: u32,
    pub paid_installment_count: u32,
    pub missed_installment_count: u32,
    pub expected_gross_interest_krw: Option<i64>,
    pub expected_income_tax_krw: Option<i64>,
    pub expected_local_income_tax_krw: Option<i64>,
    pub expected_net_payout_krw: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DepositProtectionState {
    pub institution_id: ResourceId,
    pub eligible_amount_krw: i64,
    pub protected_amount_krw: i64,
    pub unprotected_amount_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinancialIncomeYear {
    pub tax_year: u16,
    pub gross_financial_income_krw: i64,
    pub withheld_income_tax_krw: i64,
    pub withheld_local_income_tax_krw: i64,
}

impl FinancialIncomeYear {
    pub const fn zero(tax_year: u16) -> Self {
        Self {
            tax_year,
            gross_financial_income_krw: 0,
            withheld_income_tax_krw: 0,
            withheld_local_income_tax_krw: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceRuleError {
    InvalidCommandId,
    InvalidResourceId,
    InvalidLedgerPostingCount,
    ZeroLedgerPosting,
    PostingAccountRequired,
    PostingAccountForbidden,
    PostingOwnerForbidden,
    UnbalancedLedger,
    InvalidBalance,
    AccountClosed,
    AccountScopeMismatch,
    InvalidTransferAmount,
    InsufficientWalletCash,
    InsufficientAccountCash,
    InvalidInterest,
    ArithmeticOverflow,
    SettlementConflict,
}

impl FinanceRuleError {
    pub const fn failure_code(self) -> FinanceFailureCode {
        match self {
            Self::AccountClosed => FinanceFailureCode::AccountClosed,
            Self::InsufficientWalletCash => FinanceFailureCode::InsufficientWalletCash,
            Self::InsufficientAccountCash => FinanceFailureCode::InsufficientAccountCash,
            Self::SettlementConflict => FinanceFailureCode::SettlementConflict,
            Self::InvalidCommandId
            | Self::InvalidResourceId
            | Self::InvalidLedgerPostingCount
            | Self::ZeroLedgerPosting
            | Self::PostingAccountRequired
            | Self::PostingAccountForbidden
            | Self::PostingOwnerForbidden
            | Self::UnbalancedLedger
            | Self::InvalidBalance
            | Self::AccountScopeMismatch
            | Self::InvalidTransferAmount
            | Self::InvalidInterest
            | Self::ArithmeticOverflow => FinanceFailureCode::InvalidCommand,
        }
    }
}

impl Display for FinanceRuleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidCommandId => "command ID is not a canonical UUID",
            Self::InvalidResourceId => "resource ID is not a canonical unsigned decimal",
            Self::InvalidLedgerPostingCount => "a ledger transaction needs at least two postings",
            Self::ZeroLedgerPosting => "ledger postings cannot contain zero money",
            Self::PostingAccountRequired => "the ledger account code requires a financial account",
            Self::PostingAccountForbidden => {
                "the ledger account code cannot reference a financial account"
            }
            Self::PostingOwnerForbidden => {
                "the ledger transaction has no posting owned by that domain resource"
            }
            Self::UnbalancedLedger => "ledger postings do not balance to zero",
            Self::InvalidBalance => "a stored money balance is invalid",
            Self::AccountClosed => "the financial account is not open",
            Self::AccountScopeMismatch => "the financial account belongs to another run",
            Self::InvalidTransferAmount => "transfer amount must be positive",
            Self::InsufficientWalletCash => "wallet cash is insufficient",
            Self::InsufficientAccountCash => "account cash is insufficient",
            Self::InvalidInterest => "daily interest inputs are invalid",
            Self::ArithmeticOverflow => "finance arithmetic overflowed",
            Self::SettlementConflict => "scheduled settlement identity conflicts",
        };
        formatter.write_str(message)
    }
}

impl Error for FinanceRuleError {}

pub trait FinanceRules: Send + Sync + 'static {
    fn create_ledger_transaction(
        &self,
        draft: LedgerTransactionDraft,
    ) -> Result<LedgerTransaction, FinanceRuleError>;

    fn create_military_savings_ledger_transaction(
        &self,
        draft: LedgerTransactionDraft,
        contract_id: ResourceId,
    ) -> Result<LedgerTransaction, FinanceRuleError> {
        self.create_ledger_transaction(draft)?
            .with_posting_owner(LedgerPostingOwner::MilitarySavingsContract(contract_id))
    }

    fn apply_transfer(&self, input: TransferInput) -> Result<TransferMutation, FinanceRuleError>;

    fn accrue_daily_interest(
        &self,
        input: DailyInterestInput,
    ) -> Result<DailyInterestAccrual, FinanceRuleError>;
}

pub trait SettlementRules: Send + Sync + 'static {
    fn due_settlements(
        &self,
        run: RunId,
        game_day: u32,
        settlements: Vec<ScheduledSettlement>,
    ) -> Result<Vec<ScheduledSettlement>, FinanceRuleError>;
}

fn is_canonical_uuid(raw: &str) -> bool {
    if raw.len() != 36 {
        return false;
    }

    raw.as_bytes().iter().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            *byte == b'-'
        } else {
            byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_a_command_id_is_parsed {
        use super::*;

        #[test]
        fn given_a_canonical_uuid_when_parsed_then_it_is_preserved() {
            let command_id = CommandId::parse("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2")
                .expect("표준 UUID여야 한다");

            assert_eq!(command_id.as_str(), "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
        }

        #[test]
        fn given_an_uppercase_uuid_when_parsed_then_it_is_rejected() {
            let result = CommandId::parse("4F521F4C-9DD8-4D20-8E1F-15CB13CBE0F2");

            assert_eq!(result, Err(FinanceRuleError::InvalidCommandId));
        }
    }

    mod context_a_resource_id_is_parsed {
        use super::*;

        #[test]
        fn given_the_u64_maximum_when_parsed_then_it_round_trips_as_a_json_string() {
            let resource_id =
                ResourceId::parse("18446744073709551615").expect("u64 최댓값은 허용되어야 한다");

            let json = serde_json::to_string(&resource_id).expect("직렬화할 수 있어야 한다");

            assert_eq!(json, "\"18446744073709551615\"");
        }

        #[test]
        fn given_a_leading_zero_when_parsed_then_it_is_rejected() {
            let result = ResourceId::parse("01");

            assert_eq!(result, Err(FinanceRuleError::InvalidResourceId));
        }

        #[test]
        fn given_zero_when_parsed_then_it_is_rejected() {
            let result = ResourceId::parse("0");

            assert_eq!(result, Err(FinanceRuleError::InvalidResourceId));
        }

        #[test]
        fn given_a_json_number_when_deserialized_then_it_is_rejected() {
            let result = serde_json::from_str::<ResourceId>("1");

            assert!(result.is_err());
        }
    }

    mod context_a_fixed_enum_is_serialized {
        use super::*;

        #[test]
        fn given_an_isa_account_type_when_serialized_then_it_uses_camel_case() {
            let json = serde_json::to_string(&FinancialAccountType::IsaLowIncome)
                .expect("직렬화할 수 있어야 한다");

            assert_eq!(json, "\"isaLowIncome\"");
        }

        #[test]
        fn given_ledger_account_codes_when_serialized_then_the_schema_names_are_fixed() {
            let codes = [
                LedgerAccountCode::Wallet,
                LedgerAccountCode::AccountCash,
                LedgerAccountCode::ProductPrincipal,
                LedgerAccountCode::DebtPrincipal,
                LedgerAccountCode::OpeningEquity,
                LedgerAccountCode::WithholdingTaxLiability,
                LedgerAccountCode::InterestIncome,
                LedgerAccountCode::FeeExpense,
                LedgerAccountCode::DistributionIncome,
                LedgerAccountCode::RealizedGainLoss,
                LedgerAccountCode::TaxSettlement,
                LedgerAccountCode::CareerDevelopmentExpense,
            ];

            let json = serde_json::to_value(codes).expect("직렬화할 수 있어야 한다");

            assert_eq!(
                json,
                serde_json::json!([
                    "wallet",
                    "accountCash",
                    "productPrincipal",
                    "debtPrincipal",
                    "openingEquity",
                    "withholdingTaxLiability",
                    "interestIncome",
                    "feeExpense",
                    "distributionIncome",
                    "realizedGainLoss",
                    "taxSettlement",
                    "careerDevelopmentExpense"
                ])
            );
        }

        #[test]
        fn given_스펙활동_원천_when_직렬화하면_then_고정된_계약명을_사용한다() {
            let json = serde_json::to_string(&LedgerSourceKind::SpecActivity)
                .expect("직렬화할 수 있어야 한다");

            assert_eq!(json, "\"specActivity\"");
        }

        #[test]
        fn given_m4a_protocol_enums_when_serialized_then_exact_names_are_preserved() {
            let json = serde_json::json!({
                "settlement": serde_json::to_value(SettlementKind::LivingCostMonth)
                    .expect("정산 종류를 직렬화할 수 있어야 한다"),
                "settlementSource": serde_json::to_value(SettlementSourceKind::LivingCostMonth)
                    .expect("정산 원천을 직렬화할 수 있어야 한다"),
                "ledgerSources": serde_json::to_value([
                    LedgerSourceKind::LivingCostMonth,
                    LedgerSourceKind::EssentialArrearPayment,
                ])
                .expect("원장 원천을 직렬화할 수 있어야 한다"),
                "accounts": serde_json::to_value([
                    LedgerAccountCode::LivingCostExpense,
                    LedgerAccountCode::EssentialArrearLiability,
                ])
                .expect("원장 계정을 직렬화할 수 있어야 한다"),
            });

            assert_eq!(
                json,
                serde_json::json!({
                    "settlement": "livingCostMonth",
                    "settlementSource": "livingCostMonth",
                    "ledgerSources": ["livingCostMonth", "essentialArrearPayment"],
                    "accounts": ["livingCostExpense", "essentialArrearLiability"],
                })
            );
        }

        #[test]
        fn given_m4b_protocol_enums_when_serialized_then_exact_names_are_preserved() {
            let json = serde_json::json!({
                "settlement": serde_json::to_value(SettlementKind::LoanInstallment)
                    .expect("정산 종류를 직렬화할 수 있어야 한다"),
                "settlementSource": serde_json::to_value(SettlementSourceKind::LoanContract)
                    .expect("정산 원천을 직렬화할 수 있어야 한다"),
                "ledgerSources": serde_json::to_value([
                    LedgerSourceKind::LoanOrigination,
                    LedgerSourceKind::LoanInstallment,
                    LedgerSourceKind::LoanPrepayment,
                    LedgerSourceKind::DebtAuthorityBridge,
                ])
                .expect("원장 원천을 직렬화할 수 있어야 한다"),
                "accounts": serde_json::to_value([
                    LedgerAccountCode::LoanPrincipalLiability,
                    LedgerAccountCode::LoanInterestExpense,
                    LedgerAccountCode::LoanInterestLiability,
                    LedgerAccountCode::LoanFeeExpense,
                    LedgerAccountCode::TaxObligationLiability,
                ])
                .expect("원장 계정을 직렬화할 수 있어야 한다"),
            });

            assert_eq!(
                json,
                serde_json::json!({
                    "settlement": "loanInstallment",
                    "settlementSource": "loanContract",
                    "ledgerSources": [
                        "loanOrigination",
                        "loanInstallment",
                        "loanPrepayment",
                        "debtAuthorityBridge",
                    ],
                    "accounts": [
                        "loanPrincipalLiability",
                        "loanInterestExpense",
                        "loanInterestLiability",
                        "loanFeeExpense",
                        "taxObligationLiability",
                    ],
                })
            );
        }

        #[test]
        fn given_m4c2a_protocol_enums_when_serialized_then_exact_names_are_preserved() {
            let json = serde_json::json!({
                "ledgerSource": serde_json::to_value(LedgerSourceKind::LeaseMove)
                    .expect("원장 원천을 직렬화할 수 있어야 한다"),
                "accounts": serde_json::to_value([
                    LedgerAccountCode::LeaseDepositAsset,
                    LedgerAccountCode::MovingExpense,
                ])
                .expect("원장 계정을 직렬화할 수 있어야 한다"),
            });

            assert_eq!(
                json,
                serde_json::json!({
                    "ledgerSource": "leaseMove",
                    "accounts": ["leaseDepositAsset", "movingExpense"],
                })
            );
        }

        #[test]
        fn given_m4c2b1_protocol_enums_when_serialized_then_exact_names_are_preserved() {
            let json = serde_json::json!({
                "settlement": serde_json::to_value(SettlementKind::LeaseRent)
                    .expect("정산 종류를 직렬화할 수 있어야 한다"),
                "settlementSource": serde_json::to_value(SettlementSourceKind::LeaseContract)
                    .expect("정산 원천을 직렬화할 수 있어야 한다"),
                "ledgerSources": serde_json::to_value([
                    LedgerSourceKind::LeaseRent,
                    LedgerSourceKind::LeaseArrearPayment,
                ])
                .expect("원장 원천을 직렬화할 수 있어야 한다"),
                "accounts": serde_json::to_value([
                    LedgerAccountCode::LeaseRentExpense,
                    LedgerAccountCode::LeaseArrearLiability,
                ])
                .expect("원장 계정을 직렬화할 수 있어야 한다"),
            });

            assert_eq!(
                json,
                serde_json::json!({
                    "settlement": "leaseRent",
                    "settlementSource": "leaseContract",
                    "ledgerSources": ["leaseRent", "leaseArrearPayment"],
                    "accounts": ["leaseRentExpense", "leaseArrearLiability"],
                })
            );
        }

        #[test]
        fn given_m4d_protocol_enums_when_serialized_then_exact_names_are_preserved() {
            let json = serde_json::json!({
                "settlement": serde_json::to_value(SettlementKind::WelfareBenefitPayment)
                    .expect("정산 종류를 직렬화할 수 있어야 한다"),
                "settlementSource": serde_json::to_value(SettlementSourceKind::WelfarePayment)
                    .expect("정산 원천을 직렬화할 수 있어야 한다"),
                "ledgerSource": serde_json::to_value(LedgerSourceKind::WelfareBenefitPayment)
                    .expect("원장 원천을 직렬화할 수 있어야 한다"),
                "account": serde_json::to_value(LedgerAccountCode::WelfareBenefitIncome)
                    .expect("원장 계정을 직렬화할 수 있어야 한다"),
            });

            assert_eq!(
                json,
                serde_json::json!({
                    "settlement": "welfareBenefitPayment",
                    "settlementSource": "welfarePayment",
                    "ledgerSource": "welfareBenefitPayment",
                    "account": "welfareBenefitIncome",
                })
            );
        }
    }
}
