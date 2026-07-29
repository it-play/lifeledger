//! MySQL access layer (§4). Callers see only the traits.

mod annual_tax;
mod career;
mod cash_products;
mod corporations;
mod employment;
mod employment_income;
mod employment_tax;
mod finalization;
mod finance;
mod housing;
mod insolvency;
mod insurance;
mod leases;
mod life;
mod life_events;
mod loans;
mod m2d_assets;
mod market;
mod military;
mod mysql;
mod offline;
mod properties;
mod property_tax;
mod recruitment;
mod runs;
mod tax_accounts;
mod types;
mod user;
mod welfare;

pub use annual_tax::{AnnualTaxAssessmentState, AnnualTaxCalculatedState, AnnualTaxYearState};
pub use career::create_mysql_career_store;
pub use finance::create_mysql_finance_store;
pub use life::create_mysql_life_store;
pub use market::create_mysql_market_store;
pub use mysql::create_mysql_save_store;
pub use offline::create_mysql_offline_progress_store;
pub use runs::create_mysql_run_store;
pub use types::{
    AcceptCareerInvitationCommand, AcceptCareerOfferCommand, AccountUser,
    ActOnInsolvencyCaseCommand, ActiveHousingLeaseState, ActiveLeaseTermState,
    ActiveMilitarySavingsState, ActiveMilitaryServiceState, ActiveWelfareApplicationState,
    AdvanceCommandReceipt, AdvanceCommandStepResult, AdvanceDayResult, ApplyCareerCommand,
    ApplyWelfareProgramCommand, CancelCareerActivityCommand, CancelInsuranceContractCommand,
    CancelPropertySaleOrderCommand, CareerActivitiesState, CareerActivityCatalogState,
    CareerActivityState, CareerApplicationReceipt, CareerApplicationSource, CareerApplicationState,
    CareerApplicationStatus, CareerApplicationsPageState, CareerArtifactPageQuery,
    CareerArtifactPageState, CareerArtifactState, CareerCompetitionBand, CareerEmploymentState,
    CareerEmploymentTaxYearSource, CareerEmploymentTaxYearState, CareerEmploymentTaxYearStatus,
    CareerEvidenceState, CareerInvitationReceipt, CareerInvitationState, CareerInvitationStatus,
    CareerJobState, CareerJobsPageQuery, CareerJobsPageState, CareerOfferReceipt, CareerOfferState,
    CareerOfferStatus, CareerPageQuery, CareerPayrollPageState, CareerPayrollState,
    CareerPendingScheduleItemState, CareerPlatform, CareerRewardPaymentState,
    CareerScheduledActionKind, CareerScheduledSettlementKind, CareerSpecsState, CareerStore,
    CareerStoreResult, CashProductStore, CashProductStoreResult, CloseIsaAccountCommand,
    CloseIsaAccountReceipt, CloseMilitarySavingsCommand, ConfirmCareerInterviewCommand,
    CorporationAvailabilityState, CorporationDividendReceipt, CorporationNextMonthSettingState,
    CorporationOperatingMonthPageState, CorporationOperatingMonthState,
    CorporationOperatingScaleState, CorporationOperatingSettingState, CorporationReadResult,
    CorporationReceipt, CorporationSettingsReceipt, CorporationSnapshotState,
    CorporationStatusState, CorporationSummaryState, CorporationTemplateState,
    CorporationTemplatesState, CreateCorporationCommand, CreateLeaseDepositLoanQuoteCommand,
    CreateLoanQuoteCommand, CreateMortgageQuoteCommand, CreatePropertySaleOrderCommand,
    CreditOverviewState, CreditReasonState, DeclineCareerInvitationCommand,
    DeclineCareerOfferCommand, DepositLoanExecutionReceipt, EmploymentContractState,
    EmploymentStatus, EnrollInsuranceContractCommand, EssentialArrearPaymentReceipt,
    EssentialArrearState, ExecuteLoanCommand, FileInsuranceClaimCommand, FinanceStore,
    FinanceStoreResult, FocusCareerCommand, GameCommandCursor, GameCommandRejection,
    HousingLeaseCurrentState, HousingLeaseMoveReceipt, HousingListingState,
    HousingListingsQueryState, HousingListingsState, HousingMovingCostState,
    HousingPropertyHoldingsState, HousingPurchaseCapabilityState, HousingRateStatusState,
    HousingRegionState, InsolvencyActionState, InsolvencyAvailabilityState,
    InsolvencyCaseDetailState, InsolvencyCaseReceipt, InsolvencyCaseSummaryState,
    InsolvencyClaimPageState, InsolvencyClaimState, InsolvencyLiquidationPageState,
    InsolvencyLiquidationState, InsolvencyReadResult, InsolvencySnapshotState,
    InsolvencyWalletAssetState, InsuranceCancellationReceipt, InsuranceCapabilityState,
    InsuranceClaimAllocationState, InsuranceClaimHistoryState, InsuranceClaimReceipt,
    InsuranceContractState, InsuranceContractStatusState, InsuranceEligibilityReasonState,
    InsuranceEligibilityStatusState, InsuranceEnrollmentReceipt, InsuranceProductState,
    InsuranceQueryState, InsuranceReadResult, InsuranceState, InterviewDecision, IsaAccountState,
    LeaseArrearPaymentReceipt, LeaseArrearState, LeaseDepositLoanAffordabilityState,
    LeaseDepositLoanQuoteDecisionState, LeaseDepositLoanQuoteReasonState,
    LeaseDepositLoanQuoteReceipt, LeaseLifecycleTermsState, LeaseRenewalNoticeState,
    LeaseTerminationReviewState, LeaseTerminationReviewStatusState, LifeBudgetBandState,
    LifeBudgetSelectionState, LifeBudgetState, LifeEventCapabilityState, LifeEventChoiceReceipt,
    LifeEventChoiceState, LifeEventDecisionKindState, LifeEventEffectSummaryState,
    LifeEventHistoryItemState, LifeEventResolutionKindState, LifeEventsQueryState,
    LifeEventsReadResult, LifeEventsState, LifeFailureCode, LifeHouseholdState, LifeRateStatus,
    LifeResidenceState, LifeSnapshotState, LifeStore, LifeStoreResult, LivingCostMonthItemState,
    LivingCostMonthState, LoanDetailState, LoanExecutionReceipt, LoanInstallmentPageCursor,
    LoanInstallmentPageQuery, LoanInstallmentPageState, LoanInstallmentState,
    LoanInstallmentStatusState, LoanPaymentAllocationKindState, LoanPaymentAllocationState,
    LoanPaymentKindState, LoanPaymentState, LoanPrepaymentReceipt, LoanPrepaymentStatusState,
    LoanProductCatalogState, LoanProductState, LoanQuoteDecisionState, LoanQuoteDsrState,
    LoanQuoteFirstInstallmentState, LoanQuoteLtvState, LoanQuoteReasonState, LoanQuoteReceipt,
    LoanQuotedTermsState, LoanSummaryState, M2dAssetStore, ManualAdvanceCommand, MarketStore,
    MilitaryCompensationKind, MilitaryOptionIneligibilityReason, MilitaryOptionState,
    MilitaryOptionsState, MilitarySavingsClosureReason, MilitarySavingsCommandReceipt,
    MilitarySavingsContractStatus, MilitarySavingsDayCountConvention,
    MilitarySavingsHistoryItemState, MilitarySavingsIneligibilityReason,
    MilitarySavingsInstallmentState, MilitarySavingsInstallmentStatusState,
    MilitarySavingsInterestRounding, MilitarySavingsInterestTierState,
    MilitarySavingsMaturityProjectionState, MilitarySavingsPageState, MilitarySavingsProductState,
    MilitarySavingsProductsState, MilitarySavingsProjectionAssumption,
    MilitaryServiceCommandReceipt, MilitaryServiceHistoryState, MilitaryServiceSourceKind,
    MilitaryServiceState, MonthlyRentTerminationReviewTermsState, MonthlyRentTermsState,
    MortgageExecutionReceipt, MortgageLtvRegionClassState, MortgageQuoteDecisionState,
    MortgageQuoteReasonState, MortgageQuoteReceipt, MortgageStressTreatmentState,
    NextLoanInstallmentState, OfflineAttemptEvent, OfflineAttemptEventKind, OfflineAttemptIdentity,
    OfflineProgressFailure, OfflineProgressSettingStatus, OfflineProgressState,
    OfflineProgressStore, OfflineProgressUpdateResult, OfflineWorkClaim,
    OpenMilitarySavingsCommand, OpenTaxAccountCommand, OpenTaxAccountReceipt,
    PayCorporationDividendCommand, PayEssentialArrearCommand, PayLeaseArrearCommand,
    PendingInsuranceClaimState, PendingLifeEventState, PensionAccountState,
    PensionWithdrawalCommand, PensionWithdrawalReceipt, PrepareInsolvencyCaseCommand,
    PrepayLoanCommand, ProgressHolderKind, ProgressLeaseAcquireResult, ProgressStepContext,
    PropertyHoldingPurposeState, PropertyHoldingState, PropertyHoldingStatusState,
    PropertyPurchaseReceipt, PropertySaleExecutionState, PropertySaleOrderCancellationReceipt,
    PropertySaleOrderListingReceipt, PropertySaleOrderPageQuery, PropertySaleOrderPageState,
    PropertySaleOrderRejectionReasonState, PropertySaleOrderRevisionKindState,
    PropertySaleOrderStatusState, PropertySaleOrderSummaryState, PropertyTaxComponentState,
    PropertyTaxEventKindState, PropertyTaxEventPageQuery, PropertyTaxEventPageState,
    PropertyTaxEventState, PropertyTaxEventStatusState, PropertyTaxPaymentState,
    PropertyTaxPaymentStatusState, PublishCareerArtifactCommand, PurchasePropertyCommand,
    RealEstateDailyPreparationStore, RecruitmentPostingStore, RepaidDepositLoanReceipt,
    RepricePropertySaleOrderCommand, ResidenceTenureKind, ResolveLifeEventCommand, RunStore,
    SaveCursor, SaveState, SaveStore, StartCareerActivityCommand, StartGameCommand,
    StartGameManifestKind, StartGameReceipt, StartGameResult, StartHousingLeaseCommand,
    StartMilitaryServiceCommand, StartPensionCommand, StartPensionReceipt, StartingLoanCommand,
    TaxAccountStore, TaxAccountStoreResult, TradeStoreResult, TradingStore,
    UpdateCorporationSettingsCommand, UpdateLifeBudgetCommand, UpdateLifeBudgetReceipt, UserStore,
    VerifiedIncomeSourceState, WelfareApplicationReceipt, WelfareApplicationStatusState,
    WelfareApplicationSummaryState, WelfareConditionOutcomeState, WelfareConditionResultState,
    WelfareEvaluationStatusState, WelfarePaymentState, WelfarePaymentStatusState,
    WelfareProgramState, WelfareProgramsState, WithdrawCareerApplicationCommand,
};
#[cfg(test)]
pub use types::{
    ActiveMarketWorld, ActiveRunConfiguration, CareerActivityReceipt, CareerArtifactReceipt,
    CareerCatalogAssignment, CareerSnapshotState, ContentBundleAssignment,
    EmploymentPolicyAssignment, FocusCareerReceipt, MarketHistoryState, MarketWorldState,
    OfflinePolicyAssignment, OnlinePresenceRegistration, ProgressLeaseGuard,
    RunRuleBundleAssignment,
};
pub use user::create_mysql_user_store;
