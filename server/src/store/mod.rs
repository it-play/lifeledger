//! MySQL access layer (§4). Callers see only the traits.

mod annual_tax;
mod career;
mod cash_products;
mod finance;
mod m2d_assets;
mod market;
mod mysql;
mod recruitment;
mod tax_accounts;
mod types;
mod user;

pub use annual_tax::{AnnualTaxAssessmentState, AnnualTaxCalculatedState, AnnualTaxYearState};
pub use career::create_mysql_career_store;
pub use finance::create_mysql_finance_store;
pub use market::create_mysql_market_store;
pub use mysql::create_mysql_save_store;
pub use types::{
    AcceptCareerInvitationCommand, AcceptCareerOfferCommand, AccountUser, AdvanceCommandReceipt,
    AdvanceCommandStepResult, AdvanceDayResult, ApplyCareerCommand, CancelCareerActivityCommand,
    CareerActivitiesState, CareerActivityCatalogState, CareerActivityState,
    CareerApplicationReceipt, CareerApplicationSource, CareerApplicationState,
    CareerApplicationStatus, CareerApplicationsPageState, CareerArtifactPageQuery,
    CareerArtifactPageState, CareerArtifactState, CareerCompetitionBand, CareerEmploymentState,
    CareerEvidenceState, CareerInvitationReceipt, CareerInvitationState, CareerInvitationStatus,
    CareerJobState, CareerJobsPageQuery, CareerJobsPageState, CareerOfferReceipt, CareerOfferState,
    CareerOfferStatus, CareerPageQuery, CareerPlatform, CareerSpecsState, CareerStore,
    CareerStoreResult,
    CashProductStore, CashProductStoreResult, CloseIsaAccountCommand, CloseIsaAccountReceipt,
    ConfirmCareerInterviewCommand, DeclineCareerInvitationCommand, DeclineCareerOfferCommand,
    EmploymentContractState, EmploymentStatus, FinanceStore, FinanceStoreResult,
    FocusCareerCommand, GameCommandCursor, GameCommandRejection, InterviewDecision,
    IsaAccountState, M2dAssetStore, ManualAdvanceCommand, MarketStore, OpenTaxAccountCommand,
    OpenTaxAccountReceipt, PensionAccountState, PensionWithdrawalCommand, PensionWithdrawalReceipt,
    PublishCareerArtifactCommand, RecruitmentPostingStore, SaveCursor, SaveState, SaveStore,
    StartCareerActivityCommand, StartGameCommand, StartGameReceipt, StartGameResult,
    StartPensionCommand, StartPensionReceipt, TaxAccountStore, TaxAccountStoreResult,
    TradeStoreResult, TradingStore, UserStore, WithdrawCareerApplicationCommand,
};
#[cfg(test)]
pub use types::{
    ActiveMarketWorld, ActiveRunConfiguration, CareerActivityReceipt, CareerArtifactReceipt,
    CareerCatalogAssignment, CareerSnapshotState, FocusCareerReceipt, MarketHistoryState,
    MarketWorldState,
};
pub use user::create_mysql_user_store;
