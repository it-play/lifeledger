use std::collections::HashSet;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{IntoParams, Modify, OpenApi, ToSchema};
use utoipa_swagger_ui::{Config, SwaggerUi};

mod auth;

use crate::auth::{AuthUser, SESSION_COOKIE};
use crate::career::{
    ArtifactDraft, ArtifactKind, CareerFailureCode, Industry, LinkedinFields,
    MAX_MILITARY_MONEY_KRW,
};
use crate::character;
use crate::error::AppError;
use crate::finance::{
    AssetOrderSide, BondCatalog, BondOrderCommand, CashProductKind, CloseCashProductCommand,
    CloseCmaAccountCommand, CommandCursor, CommandId, FinanceFailureCode, FinancialAccountType,
    GoldCatalog, GoldOrderCommand, GoldWithdrawalCommand, IrpWithdrawalReason, M2dAccountType,
    OpenCashProductCommand, OpenCmaAccountCommand, OpenGoldAccountCommand,
    PensionWithdrawalRequestKind, ResourceId, TransferCommand, TransferDirection,
};
use crate::life::{
    HousingLeaseOfferKind, InsolvencyProcedureKind, LifeRegionKey, LivingCostCategory,
    LoanProductKind,
};
use crate::playtest::{
    AnalyticsCollection, ConsentAction, ConsentCommand, ConsentDisplayStatus,
    ConsentPolicy as DomainConsentPolicy, ConsentState as DomainConsentState,
    ConsentUpdate as DomainConsentUpdate, FeedbackCategory as DomainFeedbackCategory,
    FeedbackDeletion as DomainFeedbackDeletion, FeedbackDraft, FeedbackItem as DomainFeedbackItem,
    FeedbackSeverity as DomainFeedbackSeverity, PlaytestFailureCode,
    PlaytestFeedbackOverview as DomainPlaytestFeedbackOverview, PlaytestStoreResult,
};
use crate::runs::{
    CharacterPresetVersion, LeagueDefinition, LeagueRankingItem, LeagueRankingPage,
    PointBudgetCatalog, PointBudgetEvaluation, PointBudgetFailure, PointBudgetFailureCode,
    PointBudgetOption, PointCondition, PointCostKind, PointEffect, PointExclusiveGroup,
    PointFactComparison, PointFactValue, PointLedgerLine, PointSelection, PointTier,
    RunFinalization, RunMode, RunOptions, SeasonLeagues, SeasonStatus, SeasonSummary,
    parse_ranking_cursor,
};
use crate::state::{
    ActiveHousingLeaseSnapshot, ActiveLeaseTermSnapshot, ActiveMilitarySavingsStatusSnapshot,
    ActiveMilitarySavingsSummarySnapshot, ActiveMilitaryServiceStatusSnapshot,
    ActiveMilitaryServiceSummarySnapshot, ActiveWelfareApplicationSnapshot,
    ActiveWelfareApplicationStatusSnapshot, AdvanceCommandSnapshot, AdvanceResponse, AppState,
    AssetCommandResult, AutoSpeed, BondOrderResponse, BusinessContractSnapshot,
    BusinessContractStatusSnapshot, BusinessLoanProductSnapshot, BusinessMarketingBandSnapshot,
    BusinessMonthSnapshot, BusinessMonthlyPlanSnapshot, BusinessOperationResponse,
    BusinessOperationResultSnapshot, BusinessOperationsAvailabilitySnapshot,
    BusinessOperationsResponse, BusinessPositionSnapshot, BusinessPositionStatusSnapshot,
    BusinessWorkingCapitalLoanSnapshot, BusinessWorkingCapitalLoanStatusSnapshot,
    CareerActivitiesResponse, CareerActivityCatalogSnapshot, CareerActivityHistorySnapshot,
    CareerActivityResponse, CareerActivityResultSnapshot, CareerActivitySnapshot,
    CareerApplicationResponse, CareerApplicationResultSnapshot, CareerApplicationSnapshot,
    CareerApplicationsResponse, CareerArtifactResponse, CareerArtifactResultSnapshot,
    CareerArtifactSnapshot, CareerArtifactVersionSnapshot, CareerArtifactsResponse,
    CareerCommandResult, CareerEmploymentContractSnapshot, CareerEmploymentResponse,
    CareerEmploymentTaxYearSnapshot, CareerEmploymentTaxYearSourceSnapshot,
    CareerEmploymentTaxYearStatusSnapshot, CareerEvidenceSnapshot, CareerFocusResponse,
    CareerFocusResultSnapshot, CareerInvitationResponse, CareerInvitationResultSnapshot,
    CareerInvitationSnapshot, CareerJobSnapshot, CareerJobsResponse, CareerOfferResponse,
    CareerOfferResultSnapshot, CareerOfferSnapshot, CareerOpenApplicationSnapshot,
    CareerPayrollResponse, CareerPayrollSnapshot, CareerPendingScheduleItemSnapshot,
    CareerRewardPaymentSnapshot, CareerScheduledActionKindSnapshot,
    CareerScheduledSettlementKindSnapshot, CareerScoresSnapshot, CareerSnapshot,
    CareerSpecsResponse, CashContractSnapshot, CashProductCatalogResponse,
    CashProductCommandResult, CashProductVersionSnapshot, CharacterStartResponse,
    CharacterStartSnapshot, CmaAccountCloseResponse, CmaAccountCloseSnapshot,
    CmaAccountOpenResponse, CmaAccountOpenSnapshot, CmaAccountSnapshot,
    CorporationAvailabilitySnapshot, CorporationCreateResponse, CorporationDetailResponse,
    CorporationDividendResponse, CorporationDividendSnapshot, CorporationNextMonthSettingSnapshot,
    CorporationOperatingMonthPageResponse, CorporationOperatingMonthSnapshot,
    CorporationOperatingScaleSnapshot, CorporationOperatingSettingSnapshot,
    CorporationPayrollStatusSnapshot, CorporationSettingsResponse, CorporationSnapshot,
    CorporationStatusSnapshot, CorporationSummarySnapshot, CorporationTemplateSnapshot,
    CorporationTemplatesResponse, CreditBandSnapshot, CreditReasonSnapshot, CreditResponse,
    DepositCloseResponse, DepositCloseSnapshot, DepositKindSnapshot, DepositLoanExecutionSnapshot,
    DepositOpenResponse, DepositOpenSnapshot, DepositProtectionSnapshot,
    EssentialArrearPaymentResponse, EssentialArrearPaymentResultSnapshot, EssentialArrearSnapshot,
    FinanceAccountsResponse, FinanceCommandResult, FinanceSnapshot, FinanceTransferResponse,
    FinanceTransferSnapshot, FinancialAccountSnapshot, FinancialIncomeAssessmentSnapshot,
    FinancialIncomeSourceSnapshot, FinancialIncomeYearSnapshot, FinancialIncomeYearStatusSnapshot,
    FinancialInstitutionSnapshot, GameCommandCursorSnapshot, GameLoopError, GameSnapshot,
    GoldAccountOpenResponse, GoldOrderResponse, GoldWithdrawalResponse,
    HousingLeaseArrearRepaymentRuleSnapshot, HousingLeaseCapabilitySnapshot,
    HousingLeaseCurrentResponse, HousingLeaseMoveResponse, HousingLeaseMoveResultSnapshot,
    HousingLeaseOfferKindSnapshot, HousingLeaseRenewalRuleSnapshot, HousingLeaseRoleSnapshot,
    HousingLeaseTerminationReviewRuleSnapshot, HousingListingSnapshot, HousingListingsResponse,
    HousingMovingCostSnapshot, HousingOfferSnapshot, HousingPropertyHoldingsResponse,
    HousingPropertyTypeSnapshot, HousingPurchaseCapabilitySnapshot, HousingRateStatusSnapshot,
    HousingRegionKeySnapshot, HousingRegionSnapshot, HousingRentChargeRuleSnapshot,
    InsolvencyAvailabilitySnapshot, InsolvencyCaseCommandResponse, InsolvencyCaseDetailResponse,
    InsolvencyCaseStatusSnapshot, InsolvencyCaseSummarySnapshot, InsolvencyClaimPageResponse,
    InsolvencyClaimSnapshot, InsolvencyEligibilityReasonSnapshot,
    InsolvencyEligibilityStatusSnapshot, InsolvencyLiquidationPageResponse,
    InsolvencyLiquidationSnapshot, InsolvencyOverviewResponse, InsolvencyProcedureKindSnapshot,
    InsolvencySnapshot, InsolvencyTransitionSnapshot, InsolvencyWalletAssetSnapshot,
    InsuranceCancellationResponse, InsuranceCancellationResultSnapshot,
    InsuranceCapabilitySnapshot, InsuranceClaimAllocationSnapshot,
    InsuranceClaimHistoryItemSnapshot, InsuranceClaimResponse, InsuranceClaimResultSnapshot,
    InsuranceContractSnapshot, InsuranceContractStatusSnapshot, InsuranceContractsResponse,
    InsuranceEligibilityReasonSnapshot, InsuranceEligibilityStatusSnapshot,
    InsuranceEnrollmentResponse, InsuranceEnrollmentResultSnapshot, InsuranceProductSnapshot,
    IsaAccountSnapshot, IsaCloseResponse, IsaCloseSnapshot, JeonseHousingLeaseOfferKindSnapshot,
    LeaseArrearPaymentResponse, LeaseArrearPaymentResultSnapshot, LeaseArrearSnapshot,
    LeaseDepositLoanAffordabilitySnapshot, LeaseDepositLoanQuoteDecisionSnapshot,
    LeaseDepositLoanQuoteReasonSnapshot, LeaseDepositLoanQuoteResponse,
    LeaseDepositLoanQuoteResultSnapshot, LeaseLifecycleTermsSnapshot, LeaseRenewalNoticeSnapshot,
    LeaseTerminationReviewSnapshot, LeaseTerminationReviewStatusSnapshot, LedgerPageResponse,
    LedgerPostingSnapshot, LedgerTransactionSnapshot, LifeBudgetBandSnapshot, LifeBudgetResponse,
    LifeBudgetSelectionSnapshot, LifeBudgetUpdateResponse, LifeBudgetUpdateResultSnapshot,
    LifeCommandResult, LifeEventCapabilitySnapshot, LifeEventChoiceResponse,
    LifeEventChoiceResultSnapshot, LifeEventChoiceSnapshot, LifeEventDecisionKindSnapshot,
    LifeEventEffectSummarySnapshot, LifeEventHistoryItemSnapshot, LifeEventResolutionKindSnapshot,
    LifeEventsResponse, LifeHouseholdSnapshot, LifeRateStatusSnapshot, LifeResidenceSnapshot,
    LifeSnapshot, LivingCostCategorySnapshot, LivingCostMonthItemSnapshot, LivingCostMonthSnapshot,
    LoanContractStatusSnapshot, LoanDayCountRuleSnapshot, LoanDetailResponse,
    LoanExecutionResponse, LoanExecutionResultSnapshot, LoanInstallmentsResponse,
    LoanLenderSectorSnapshot, LoanPaymentCalendarSnapshot, LoanPrepaymentEffectSnapshot,
    LoanPrepaymentNextInstallmentSnapshot, LoanPrepaymentResponse, LoanPrepaymentResultSnapshot,
    LoanPrepaymentStatusSnapshot, LoanProductCatalogResponse, LoanProductKindSnapshot,
    LoanProductProvenanceSnapshot, LoanProductSnapshot, LoanQuoteDecisionSnapshot,
    LoanQuoteDsrSnapshot, LoanQuoteFirstInstallmentSnapshot, LoanQuoteReasonSnapshot,
    LoanQuoteResponse, LoanQuoteResultSnapshot, LoanQuotedTermsSnapshot, LoanRateReferenceSnapshot,
    LoanRateResetRuleSnapshot, LoanRateStatusSnapshot, LoanRateTypeSnapshot,
    LoanRepaymentMethodSnapshot, LoanSummarySnapshot, M2MarketFactorsSnapshot, MarketHistoryPoint,
    MarketHistoryResponse, MarketIndexSnapshot, MarketRatesSnapshot, MarketSnapshot,
    MilitaryCompensationKindSnapshot, MilitaryExperienceCreditSnapshot,
    MilitaryHardRequirementsSnapshot, MilitaryOptionIneligibilityReasonSnapshot,
    MilitaryOptionSnapshot, MilitaryOptionsResponse, MilitaryPayScheduleSnapshot,
    MilitaryPayStageSnapshot, MilitarySavingsClosureReasonSnapshot, MilitarySavingsCommandResponse,
    MilitarySavingsContractStatusSnapshot, MilitarySavingsDayCountConventionSnapshot,
    MilitarySavingsHistoryItemSnapshot, MilitarySavingsHistoryResponse,
    MilitarySavingsIneligibilityReasonSnapshot, MilitarySavingsInstallmentSnapshot,
    MilitarySavingsInstallmentStatusSnapshot, MilitarySavingsInterestRoundingSnapshot,
    MilitarySavingsInterestTierSnapshot, MilitarySavingsMaturityProjectionSnapshot,
    MilitarySavingsProductSnapshot, MilitarySavingsProductsResponse,
    MilitarySavingsProjectionAssumptionSnapshot, MilitarySavingsResultSnapshot,
    MilitaryServiceCommandResponse, MilitaryServiceHistorySnapshot, MilitaryServiceResponse,
    MilitaryServiceResultSnapshot, MilitaryServiceSourceKindSnapshot,
    MilitaryServiceStatusSnapshot, MilitaryServiceTypeSnapshot, MilitaryStatusSnapshot,
    MonthlyRentTerminationReviewTermsSnapshot, MonthlyRentTermsSnapshot, MortgageExecutionSnapshot,
    MortgageLtvRegionClassSnapshot, MortgageLtvSnapshot, MortgageQuoteDecisionSnapshot,
    MortgageQuoteReasonSnapshot, MortgageQuoteResponse, MortgageQuoteResultSnapshot,
    MortgageStressTreatmentSnapshot, NextLoanInstallmentSnapshot, PendingInsuranceClaimSnapshot,
    PendingLifeEventSnapshot, PendingSettlementSnapshot, PensionAccountSnapshot,
    PensionStartResponse, PensionStartSnapshot, PensionTaxLayersSnapshot,
    PensionWithdrawalResponse, PensionWithdrawalSnapshot, PlaceOrderResult, PolicySetSnapshot,
    PortfolioOrderResponse, PropertyHoldingPurposeSnapshot, PropertyHoldingSnapshot,
    PropertyHoldingStatusSnapshot, PropertyPurchaseResponse, PropertyPurchaseResultSnapshot,
    PropertySaleExecutionSnapshot, PropertySaleOrderCancellationResponse,
    PropertySaleOrderCancellationResultSnapshot, PropertySaleOrderListingResponse,
    PropertySaleOrderListingResultSnapshot, PropertySaleOrderRejectionReasonSnapshot,
    PropertySaleOrderRevisionKindSnapshot, PropertySaleOrderStatusSnapshot,
    PropertySaleOrderSummarySnapshot, PropertySaleOrdersResponse, PropertyTaxComponentSnapshot,
    PropertyTaxEventKindSnapshot, PropertyTaxEventSnapshot, PropertyTaxEventStatusSnapshot,
    PropertyTaxEventsResponse, PropertyTaxPaymentSnapshot, PropertyTaxPaymentStatusSnapshot,
    RegulatoryDsrAppliedSnapshot, RepaidDepositLoanSnapshot, ResidenceTenureKindSnapshot,
    StreamConnection, TaxAccountCommandResult, TaxAccountOpenResponse, TaxAccountOpenSnapshot,
    VerifiedIncomeSourceSnapshot, WelfareApplicationResponse, WelfareApplicationResultSnapshot,
    WelfareApplicationStatusSnapshot, WelfareApplicationSummarySnapshot,
    WelfareConditionOutcomeSnapshot, WelfareConditionResultSnapshot,
    WelfareEvaluationStatusSnapshot, WelfarePaymentSnapshot, WelfarePaymentStatusSnapshot,
    WelfareProgramSnapshot, WelfareProgramsResponse, YearMonthSnapshot,
};
use crate::store::{
    AcceptCareerInvitationCommand, AcceptCareerOfferCommand, ActOnInsolvencyCaseCommand,
    ApplyCareerCommand, ApplyWelfareProgramCommand, BusinessOperationAction,
    CancelCareerActivityCommand, CancelInsuranceContractCommand, CancelPropertySaleOrderCommand,
    CareerArtifactPageQuery, CareerJobsPageQuery, CareerPageQuery, CareerPlatform,
    CloseIsaAccountCommand, CloseMilitarySavingsCommand, ConfirmCareerInterviewCommand,
    CreateCorporationCommand, CreateLeaseDepositLoanQuoteCommand, CreateLoanQuoteCommand,
    CreateMortgageQuoteCommand, CreatePropertySaleOrderCommand, DeclineCareerInvitationCommand,
    DeclineCareerOfferCommand, EnrollInsuranceContractCommand, ExecuteLoanCommand,
    FileInsuranceClaimCommand, FocusCareerCommand, HousingListingsQueryState,
    InsolvencyActionState, InsuranceQueryState, InterviewDecision, LifeBudgetSelectionState,
    LifeEventsQueryState, LifeFailureCode, LoanInstallmentPageCursor, LoanInstallmentPageQuery,
    ManageBusinessOperationsCommand, ManualAdvanceCommand, OpenMilitarySavingsCommand,
    OpenTaxAccountCommand, PayCorporationDividendCommand, PayEssentialArrearCommand,
    PayLeaseArrearCommand, PensionWithdrawalCommand, PrepareInsolvencyCaseCommand,
    PrepayLoanCommand, PropertySaleOrderPageQuery, PropertyTaxEventPageQuery,
    PublishCareerArtifactCommand, PurchasePropertyCommand, RepricePropertySaleOrderCommand,
    ResolveLifeEventCommand, StartCareerActivityCommand, StartGameCommand, StartGameManifestKind,
    StartHousingLeaseCommand, StartMilitaryServiceCommand, StartPensionCommand,
    StartingLoanCommand, UpdateCorporationSettingsCommand, UpdateLifeBudgetCommand,
    WithdrawCareerApplicationCommand,
};
use crate::store::{
    OfflineProgressFailure, OfflineProgressSettingStatus, OfflineProgressState,
    OfflineProgressUpdateResult, ProgressHolderKind,
};
use crate::trading::{
    OrderSide, Portfolio, PortfolioPosition, TradeExecution, TradeFailure, TradeFailureCode,
    TradeOrder, TradeOrderRequest,
};

/// Reconnect delay the server suggests; the client uses it as its backoff baseline.
const RETRY_HINT: Duration = Duration::from_secs(1);
/// Keep-alive comment interval, so proxies do not drop an idle connection.
const KEEP_ALIVE: Duration = Duration::from_secs(15);
const DEFAULT_MARKET_HISTORY_DAYS: u32 = 365;
const MAX_MARKET_HISTORY_DAYS: u32 = 3_660;
const DEFAULT_LEDGER_PAGE_SIZE: u32 = 50;
const MAX_LEDGER_PAGE_SIZE: u32 = 200;
const DEFAULT_CAREER_PAGE_SIZE: u32 = 50;
const MAX_CAREER_PAGE_SIZE: u32 = 200;
const DEFAULT_LOAN_INSTALLMENT_PAGE_SIZE: u8 = 50;
const MAX_LOAN_INSTALLMENT_PAGE_SIZE: u8 = 50;
const DEFAULT_PROPERTY_HISTORY_PAGE_SIZE: u8 = 20;
const MAX_PROPERTY_HISTORY_PAGE_SIZE: u8 = 20;
const DEFAULT_RANKING_PAGE_SIZE: u32 = 20;
const MAX_RANKING_PAGE_SIZE: u32 = 100;
const MAX_JSON_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// API docs, mounted under `/api` because that is the prefix nginx forwards.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "LifeLedger API",
        description = "모의 자산관리 인생 시뮬레이션 서버"
    ),
    paths(
        health,
        presets,
        run_options,
        season_leagues,
        league_rankings,
        run_finalization,
        playtest_feedback_overview,
        set_playtest_consent,
        submit_playtest_feedback,
        delete_playtest_feedback,
        offline_progress_status,
        set_offline_progress,
        preview_point_budget,
        create_run,
        create_character,
        snapshot,
        advance,
        place_portfolio_order,
        finance_accounts,
        bond_catalog,
        place_bond_order,
        gold_product_catalog,
        place_gold_order,
        withdraw_gold,
        cash_product_catalog,
        open_financial_account,
        close_cma_account,
        close_isa_account,
        start_pension,
        withdraw_pension,
        open_deposit,
        close_deposit,
        finance_tax_year,
        finance_transfer,
        finance_ledger,
        welfare_programs,
        apply_welfare_program,
        life_events,
        resolve_life_event,
        insurance_contracts,
        enroll_insurance_contract,
        cancel_insurance_contract,
        file_insurance_claim,
        insolvency_overview,
        prepare_insolvency_case,
        act_on_insolvency_case,
        insolvency_case_detail,
        insolvency_claims,
        insolvency_liquidations,
        corporation_templates,
        create_corporation,
        corporation_detail,
        corporation_operations,
        manage_corporation_operations,
        update_corporation_settings,
        pay_corporation_dividend,
        corporation_operating_months,
        housing_listings,
        housing_lease_current,
        housing_property_holdings,
        property_sale_orders,
        create_property_sale_order,
        reprice_property_sale_order,
        cancel_property_sale_order,
        property_tax_events,
        start_housing_lease,
        quote_lease_deposit_loan,
        quote_mortgage,
        purchase_property,
        life_budget,
        loan_products,
        loan_detail,
        loan_installments,
        quote_loan,
        execute_loan,
        prepay_loan,
        credit,
        update_life_budget,
        pay_essential_arrear,
        pay_lease_arrear,
        career_specs,
        career_activities,
        career_artifacts,
        focus_career,
        start_career_activity,
        cancel_career_activity,
        publish_career_artifact,
        career_jobs,
        career_applications,
        career_employment,
        career_payroll,
        career_employment_tax_year,
        military_options,
        military_service,
        military_savings_products,
        military_savings,
        apply_career,
        confirm_career_interview,
        withdraw_career_application,
        accept_career_invitation,
        decline_career_invitation,
        accept_career_offer,
        decline_career_offer,
        start_military_service,
        open_military_savings,
        close_military_savings,
        market_history,
        clock,
        stream,
        auth::providers,
        auth::me,
        auth::logout,
        auth::delete_account,
    ),
    components(schemas(
        GameSnapshot,
        CareerSnapshot,
        CareerScoresSnapshot,
        CareerActivitySnapshot,
        CareerArtifactSnapshot,
        CareerEvidenceSnapshot,
        CareerSpecsResponse,
        CareerActivityCatalogSnapshot,
        CareerActivityHistorySnapshot,
        CareerActivitiesResponse,
        CareerArtifactVersionSnapshot,
        CareerArtifactsResponse,
        CareerFocusRequest,
        CareerActivityStartRequest,
        CareerCursorRequest,
        CareerArtifactPublishRequest,
        CareerFocusResultSnapshot,
        CareerActivityResultSnapshot,
        CareerArtifactResultSnapshot,
        CareerFocusResponse,
        CareerActivityResponse,
        CareerArtifactResponse,
        CareerJobSnapshot,
        CareerJobsResponse,
        CareerOfferSnapshot,
        CareerApplicationSnapshot,
        CareerOpenApplicationSnapshot,
        CareerInvitationSnapshot,
        CareerEmploymentContractSnapshot,
        CareerApplicationsResponse,
        CareerEmploymentResponse,
        CareerPayrollSnapshot,
        CareerRewardPaymentSnapshot,
        CareerPayrollResponse,
        CareerEmploymentTaxYearSnapshot,
        CareerEmploymentTaxYearStatusSnapshot,
        CareerEmploymentTaxYearSourceSnapshot,
        MilitaryStatusSnapshot,
        MilitaryServiceTypeSnapshot,
        MilitaryServiceStatusSnapshot,
        ActiveMilitaryServiceStatusSnapshot,
        MilitaryServiceSourceKindSnapshot,
        MilitaryCompensationKindSnapshot,
        MilitaryPayScheduleSnapshot,
        MilitaryOptionIneligibilityReasonSnapshot,
        MilitarySavingsIneligibilityReasonSnapshot,
        MilitarySavingsContractStatusSnapshot,
        ActiveMilitarySavingsStatusSnapshot,
        MilitarySavingsInstallmentStatusSnapshot,
        MilitarySavingsClosureReasonSnapshot,
        MilitarySavingsDayCountConventionSnapshot,
        MilitarySavingsInterestRoundingSnapshot,
        MilitarySavingsProjectionAssumptionSnapshot,
        MilitaryHardRequirementsSnapshot,
        MilitaryPayStageSnapshot,
        MilitaryExperienceCreditSnapshot,
        MilitaryOptionSnapshot,
        MilitaryOptionsResponse,
        ActiveMilitaryServiceSummarySnapshot,
        MilitaryServiceHistorySnapshot,
        MilitaryServiceResponse,
        MilitarySavingsInterestTierSnapshot,
        MilitarySavingsProductSnapshot,
        MilitarySavingsProductsResponse,
        ActiveMilitarySavingsSummarySnapshot,
        MilitarySavingsInstallmentSnapshot,
        MilitarySavingsMaturityProjectionSnapshot,
        MilitarySavingsHistoryItemSnapshot,
        MilitarySavingsHistoryResponse,
        MilitaryServiceResultSnapshot,
        MilitarySavingsResultSnapshot,
        MilitaryServiceCommandResponse,
        MilitarySavingsCommandResponse,
        CareerScheduledActionKindSnapshot,
        CareerScheduledSettlementKindSnapshot,
        CareerPendingScheduleItemSnapshot,
        MilitaryServiceStartRequest,
        MilitarySavingsEnrollmentRequest,
        CareerApplicationResultSnapshot,
        CareerInvitationResultSnapshot,
        CareerOfferResultSnapshot,
        CareerApplicationResponse,
        CareerInvitationResponse,
        CareerOfferResponse,
        CareerFailure,
        CareerArtifactKindRequest,
        CareerIndustryRequest,
        CareerPlatformRequest,
        CareerInterviewDecisionRequest,
        CareerApplicationRequest,
        CareerInterviewConfirmationRequest,
        MarketSnapshot,
        MarketIndexSnapshot,
        MarketRatesSnapshot,
        M2MarketFactorsSnapshot,
        crate::market::MarketRegime,
        AutoSpeed,
        Health,
        RunMode,
        RunOptions,
        SeasonStatus,
        SeasonSummary,
        LeagueDefinition,
        SeasonLeagues,
        LeagueRankingItem,
        LeagueRankingPage,
        CharacterPresetVersion,
        PointBudgetCatalog,
        PointBudgetOption,
        PointExclusiveGroup,
        PointCostKind,
        PointTier,
        PointEffect,
        PointCondition,
        PointFactComparison,
        PointFactValue,
        PointSelection,
        PointLedgerLine,
        PointBudgetFailureCode,
        PointBudgetFailure,
        PointBudgetEvaluation,
        PointBudgetPreviewRequest,
        PointSelectionRequest,
        RunRequestFailure,
        RunRequestFailureCode,
        RunStartRequest,
        RankedPresetRunStartRequest,
        RankedCustomRunStartRequest,
        SandboxRunStartRequest,
        RunStartResponse,
        CharacterStartRequest,
        CharacterStartResponse,
        CharacterStartSnapshot,
        GameCommandCursorSnapshot,
        AdvanceRequest,
        AdvanceResponse,
        AdvanceCommandSnapshot,
        ClockRequest,
        ClockSetting,
        GameCommandFailure,
        PortfolioOrderResponse,
        FinanceSnapshot,
        PolicySetSnapshot,
        FinancialAccountSnapshot,
        PendingSettlementSnapshot,
        FinanceAccountsResponse,
        CashProductCatalogResponse,
        CashProductVersionSnapshot,
        FinancialInstitutionSnapshot,
        FinanceAccountOpenRequest,
        FinanceAccountOpenResponse,
        GoldAccountOpenRequest,
        BondOrderRequest,
        BondOrderResponse,
        GoldOrderRequest,
        GoldOrderResponse,
        GoldWithdrawalRequest,
        GoldWithdrawalResponse,
        GoldAccountOpenResponse,
        BondCatalog,
        GoldCatalog,
        CmaAccountOpenRequest,
        CmaAccountOpenType,
        CmaAccountOpenResponse,
        CmaAccountOpenSnapshot,
        TaxAccountOpenRequest,
        TaxAccountOpenType,
        TaxAccountOpenResponse,
        TaxAccountOpenSnapshot,
        FinanceCursorCommandRequest,
        CmaAccountCloseResponse,
        CmaAccountCloseSnapshot,
        DepositOpenRequest,
        DepositKindRequest,
        DepositOpenResponse,
        DepositOpenSnapshot,
        DepositCloseResponse,
        DepositCloseSnapshot,
        DepositKindSnapshot,
        crate::finance::CashProductKind,
        crate::finance::CashRateReference,
        crate::finance::CashProductContractStatus,
        CmaAccountSnapshot,
        CashContractSnapshot,
        IsaAccountSnapshot,
        IsaCloseResponse,
        IsaCloseSnapshot,
        PensionAccountSnapshot,
        PensionTaxLayersSnapshot,
        PensionStartRequest,
        PensionStartResponse,
        PensionStartSnapshot,
        PensionWithdrawalRequest,
        PensionWithdrawalResponse,
        PensionWithdrawalSnapshot,
        crate::finance::IrpWithdrawalReason,
        crate::finance::PensionWithdrawalRequestKind,
        DepositProtectionSnapshot,
        FinancialIncomeYearSnapshot,
        FinancialIncomeAssessmentSnapshot,
        FinancialIncomeSourceSnapshot,
        FinancialIncomeYearStatusSnapshot,
        crate::finance::FinancialIncomeSource,
        FinanceTransferRequest,
        FinanceTransferResponse,
        FinanceTransferSnapshot,
        FinanceFailure,
        crate::finance::FinanceFailureCode,
        crate::finance::FinancialAccountStatus,
        crate::finance::FinancialAccountType,
        crate::finance::LedgerAccountCode,
        crate::finance::LedgerSourceKind,
        crate::finance::SettlementKind,
        crate::finance::TransferDirection,
        LedgerPageResponse,
        LedgerTransactionSnapshot,
        LedgerPostingSnapshot,
        LifeSnapshot,
        LifeRateStatusSnapshot,
        WelfareEvaluationStatusSnapshot,
        WelfareConditionOutcomeSnapshot,
        WelfareApplicationStatusSnapshot,
        ActiveWelfareApplicationStatusSnapshot,
        WelfarePaymentStatusSnapshot,
        WelfareConditionResultSnapshot,
        WelfarePaymentSnapshot,
        WelfareApplicationSummarySnapshot,
        WelfareProgramSnapshot,
        WelfareProgramsResponse,
        ActiveWelfareApplicationSnapshot,
        LifeEventCapabilitySnapshot,
        InsuranceCapabilitySnapshot,
        LifeEventDecisionKindSnapshot,
        LifeEventResolutionKindSnapshot,
        LifeEventEffectSummarySnapshot,
        LifeEventChoiceSnapshot,
        PendingLifeEventSnapshot,
        LifeEventHistoryItemSnapshot,
        LifeEventsResponse,
        LifeEventChoiceRequest,
        LifeEventChoiceResultSnapshot,
        LifeEventChoiceResponse,
        InsuranceEligibilityStatusSnapshot,
        InsuranceEligibilityReasonSnapshot,
        InsuranceContractStatusSnapshot,
        InsuranceProductSnapshot,
        InsuranceContractSnapshot,
        InsuranceClaimAllocationSnapshot,
        PendingInsuranceClaimSnapshot,
        InsuranceClaimHistoryItemSnapshot,
        InsuranceContractsResponse,
        InsuranceEnrollmentRequest,
        InsuranceEnrollmentResultSnapshot,
        InsuranceEnrollmentResponse,
        InsuranceCancellationRequest,
        InsuranceCancellationResultSnapshot,
        InsuranceCancellationResponse,
        InsuranceClaimRequest,
        InsuranceClaimResultSnapshot,
        InsuranceClaimResponse,
        InsolvencyAvailabilitySnapshot,
        InsolvencyEligibilityStatusSnapshot,
        InsolvencyEligibilityReasonSnapshot,
        InsolvencyProcedureKindSnapshot,
        InsolvencyCaseStatusSnapshot,
        InsolvencyCaseSummarySnapshot,
        InsolvencySnapshot,
        InsolvencyCaseCommandResponse,
        InsolvencyTransitionSnapshot,
        InsolvencyCaseDetailResponse,
        InsolvencyClaimSnapshot,
        InsolvencyClaimPageResponse,
        InsolvencyWalletAssetSnapshot,
        InsolvencyLiquidationSnapshot,
        InsolvencyLiquidationPageResponse,
        InsolvencyCasePrepareRequest,
        InsolvencyCaseActionRequest,
        InsolvencyActionRequestKind,
        InsolvencyProcedureRequestKind,
        CorporationAvailabilitySnapshot,
        CorporationStatusSnapshot,
        CorporationNextMonthSettingSnapshot,
        CorporationOperatingScaleSnapshot,
        CorporationOperatingSettingSnapshot,
        CorporationTemplateSnapshot,
        CorporationTemplatesResponse,
        CorporationSummarySnapshot,
        CorporationSnapshot,
        CorporationCreateRequest,
        CorporationCreateResponse,
        CorporationOperationRequest,
        BusinessOperationsAvailabilitySnapshot,
        BusinessContractStatusSnapshot,
        BusinessPositionStatusSnapshot,
        BusinessMarketingBandSnapshot,
        BusinessLoanProductSnapshot,
        BusinessWorkingCapitalLoanStatusSnapshot,
        BusinessWorkingCapitalLoanSnapshot,
        BusinessContractSnapshot,
        BusinessPositionSnapshot,
        BusinessMonthlyPlanSnapshot,
        BusinessMonthSnapshot,
        BusinessOperationsResponse,
        BusinessOperationResultSnapshot,
        BusinessOperationResponse,
        CorporationSettingsRequest,
        CorporationSettingsResponse,
        CorporationPayoutKindRequest,
        CorporationPayoutRequest,
        CorporationDividendSnapshot,
        CorporationDividendResponse,
        CorporationPayrollStatusSnapshot,
        CorporationOperatingMonthSnapshot,
        CorporationOperatingMonthPageResponse,
        WelfareApplicationRequest,
        WelfareApplicationResultSnapshot,
        WelfareApplicationResponse,
        HousingRegionKeySnapshot,
        HousingRateStatusSnapshot,
        HousingPropertyTypeSnapshot,
        HousingOfferSnapshot,
        HousingRegionSnapshot,
        HousingListingSnapshot,
        HousingListingsResponse,
        HousingLeaseCapabilitySnapshot,
        HousingLeaseRenewalRuleSnapshot,
        HousingLeaseTerminationReviewRuleSnapshot,
        HousingLeaseRoleSnapshot,
        HousingLeaseOfferKindSnapshot,
        JeonseHousingLeaseOfferKindSnapshot,
        HousingRentChargeRuleSnapshot,
        HousingLeaseArrearRepaymentRuleSnapshot,
        HousingMovingCostSnapshot,
        ActiveHousingLeaseSnapshot,
        ActiveLeaseTermSnapshot,
        MonthlyRentTermsSnapshot,
        MonthlyRentTerminationReviewTermsSnapshot,
        LeaseLifecycleTermsSnapshot,
        LeaseRenewalNoticeSnapshot,
        LeaseTerminationReviewStatusSnapshot,
        LeaseTerminationReviewSnapshot,
        LeaseArrearSnapshot,
        HousingLeaseCurrentResponse,
        HousingLeaseOfferKindRequest,
        JeonseHousingLeaseOfferKindRequest,
        StartHousingLeaseRequest,
        StartHousingLeaseCashRequest,
        StartHousingLeaseFinancedRequest,
        HousingLeaseMoveResultSnapshot,
        HousingLeaseMoveResponse,
        DepositLoanExecutionSnapshot,
        RepaidDepositLoanSnapshot,
        HousingPurchaseCapabilitySnapshot,
        PropertyHoldingStatusSnapshot,
        PropertyHoldingPurposeSnapshot,
        PropertyHoldingSnapshot,
        HousingPropertyHoldingsResponse,
        MortgageQuoteRequest,
        MortgageQuoteDecisionSnapshot,
        MortgageQuoteReasonSnapshot,
        MortgageLtvRegionClassSnapshot,
        MortgageStressTreatmentSnapshot,
        MortgageLtvSnapshot,
        MortgageQuoteResultSnapshot,
        MortgageQuoteResponse,
        PropertyPurchaseRequest,
        MortgageExecutionSnapshot,
        PropertyPurchaseResultSnapshot,
        PropertyPurchaseResponse,
        PropertySaleOrderCreateRequest,
        PropertySaleOrderRepriceRequest,
        PropertySaleOrderCancelRequest,
        PropertySaleOrderStatusSnapshot,
        PropertySaleOrderRevisionKindSnapshot,
        PropertySaleOrderRejectionReasonSnapshot,
        PropertySaleOrderListingResultSnapshot,
        PropertySaleOrderCancellationResultSnapshot,
        PropertySaleOrderListingResponse,
        PropertySaleOrderCancellationResponse,
        PropertySaleExecutionSnapshot,
        PropertySaleOrderSummarySnapshot,
        PropertySaleOrdersResponse,
        PropertyTaxEventKindSnapshot,
        PropertyTaxEventStatusSnapshot,
        PropertyTaxPaymentStatusSnapshot,
        PropertyTaxComponentSnapshot,
        PropertyTaxPaymentSnapshot,
        PropertyTaxEventSnapshot,
        PropertyTaxEventsResponse,
        LeaseDepositLoanQuoteRequest,
        LeaseDepositLoanQuoteDecisionSnapshot,
        LeaseDepositLoanQuoteReasonSnapshot,
        LeaseDepositLoanAffordabilitySnapshot,
        LeaseDepositLoanQuoteResultSnapshot,
        LeaseDepositLoanQuoteResponse,
        RegulatoryDsrAppliedSnapshot,
        LeaseArrearPaymentRequest,
        LeaseArrearPaymentResultSnapshot,
        LeaseArrearPaymentResponse,
        CreditBandSnapshot,
        CreditReasonSnapshot,
        CreditResponse,
        LoanProductCatalogResponse,
        LoanProductSnapshot,
        LoanLenderSectorSnapshot,
        LoanRateTypeSnapshot,
        LoanRateReferenceSnapshot,
        LoanRateResetRuleSnapshot,
        LoanDayCountRuleSnapshot,
        LoanRepaymentMethodSnapshot,
        LoanPaymentCalendarSnapshot,
        LoanPrepaymentEffectSnapshot,
        LoanProductProvenanceSnapshot,
        LoanProductKindSnapshot,
        LoanRateStatusSnapshot,
        LoanContractStatusSnapshot,
        LoanSummarySnapshot,
        NextLoanInstallmentSnapshot,
        LoanQuoteRequest,
        LoanQuoteDecisionSnapshot,
        LoanQuoteReasonSnapshot,
        VerifiedIncomeSourceSnapshot,
        LoanQuoteDsrSnapshot,
        LoanQuoteFirstInstallmentSnapshot,
        LoanQuotedTermsSnapshot,
        LoanQuoteResultSnapshot,
        LoanQuoteResponse,
        LoanExecutionRequest,
        LoanExecutionResultSnapshot,
        LoanExecutionResponse,
        LoanPrepaymentRequest,
        LoanPrepaymentStatusSnapshot,
        LoanPrepaymentNextInstallmentSnapshot,
        LoanPrepaymentResultSnapshot,
        LoanPrepaymentResponse,
        LifeHouseholdSnapshot,
        LifeResidenceSnapshot,
        ResidenceTenureKindSnapshot,
        LivingCostCategorySnapshot,
        YearMonthSnapshot,
        LifeBudgetBandSnapshot,
        LifeBudgetSelectionSnapshot,
        LivingCostMonthItemSnapshot,
        LivingCostMonthSnapshot,
        EssentialArrearSnapshot,
        LifeBudgetResponse,
        LifeBudgetSelectionRequest,
        LifeBudgetUpdateRequest,
        EssentialArrearPaymentRequest,
        LifeBudgetUpdateResultSnapshot,
        LifeBudgetUpdateResponse,
        EssentialArrearPaymentResultSnapshot,
        EssentialArrearPaymentResponse,
        LifeFailure,
        LifeFailureCodeSnapshot,
        MarketHistoryResponse,
        MarketHistoryPoint,
        TradeOrderRequest,
        TradeExecution,
        TradeFailure,
        TradeFailureCode,
        OrderSide,
        Portfolio,
        PortfolioPosition,
        ValidationFailure,
        character::CharacterDraft,
        character::Preset,
        character::ValidationError,
        character::Gender,
        character::MilitaryStatus,
        character::Education,
        character::Region,
        character::FamilyBackground,
        character::Health,
        auth::ProviderSummary,
        auth::MeResponse,
        auth::AccountDeletionConfirmation,
        auth::AccountDeletionRequest,
        auth::AccountDeletionFailure,
        crate::auth::ProviderKind,
    )),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "sessionCookie",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                    SESSION_COOKIE,
                    "로그인 세션 쿠키",
                ))),
            );
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/presets", get(presets))
        .route("/api/run-options", get(run_options))
        .route("/api/seasons/{id}/leagues", get(season_leagues))
        .route("/api/leagues/{id}/rankings", get(league_rankings))
        .route("/api/runs/{id}/finalization", get(run_finalization))
        .route(
            "/api/playtest/feedback",
            get(playtest_feedback_overview).post(submit_playtest_feedback),
        )
        .route("/api/playtest/consent", put(set_playtest_consent))
        .route(
            "/api/playtest/feedback/{id}",
            delete(delete_playtest_feedback),
        )
        .route("/api/offline-progress", put(set_offline_progress))
        .route("/api/offline-progress/status", get(offline_progress_status))
        .route("/api/runs/point-preview", post(preview_point_budget))
        .route("/api/runs", post(create_run))
        .route("/api/characters", post(create_character))
        .route("/api/state", get(snapshot))
        .route("/api/advance", post(advance))
        .route("/api/portfolio/orders", post(place_portfolio_order))
        .route(
            "/api/finance/accounts",
            get(finance_accounts).post(open_financial_account),
        )
        .route("/api/finance/bonds", get(bond_catalog))
        .route("/api/finance/bonds/orders", post(place_bond_order))
        .route("/api/finance/gold-products", get(gold_product_catalog))
        .route("/api/finance/gold/orders", post(place_gold_order))
        .route("/api/finance/gold/withdrawals", post(withdraw_gold))
        .route("/api/finance/accounts/{id}/close", post(close_cma_account))
        .route("/api/finance/isa/{id}/close", post(close_isa_account))
        .route("/api/finance/pensions/{id}/start", post(start_pension))
        .route(
            "/api/finance/pensions/{id}/withdrawals",
            post(withdraw_pension),
        )
        .route("/api/finance/cash-products", get(cash_product_catalog))
        .route("/api/finance/deposits", post(open_deposit))
        .route("/api/finance/deposits/{id}/close", post(close_deposit))
        .route("/api/finance/tax-years/{year}", get(finance_tax_year))
        .route("/api/finance/transfers", post(finance_transfer))
        .route("/api/finance/ledger", get(finance_ledger))
        .route("/api/welfare/programs", get(welfare_programs))
        .route("/api/welfare/applications", post(apply_welfare_program))
        .route("/api/life/events", get(life_events))
        .route(
            "/api/life/events/{eventId}/choices",
            post(resolve_life_event),
        )
        .route(
            "/api/insurance/contracts",
            get(insurance_contracts).post(enroll_insurance_contract),
        )
        .route(
            "/api/insurance/contracts/{contractId}/cancellations",
            post(cancel_insurance_contract),
        )
        .route("/api/insurance/claims", post(file_insurance_claim))
        .route("/api/insolvency", get(insolvency_overview))
        .route("/api/insolvency/cases", post(prepare_insolvency_case))
        .route(
            "/api/insolvency/{caseId}/actions",
            post(act_on_insolvency_case),
        )
        .route("/api/insolvency/{caseId}", get(insolvency_case_detail))
        .route("/api/insolvency/{caseId}/claims", get(insolvency_claims))
        .route(
            "/api/insolvency/{caseId}/liquidations",
            get(insolvency_liquidations),
        )
        .route("/api/corporations/templates", get(corporation_templates))
        .route("/api/corporations", post(create_corporation))
        .route("/api/corporations/{corporationId}", get(corporation_detail))
        .route(
            "/api/corporations/{corporationId}/operations",
            get(corporation_operations).post(manage_corporation_operations),
        )
        .route(
            "/api/corporations/{corporationId}/settings",
            put(update_corporation_settings),
        )
        .route(
            "/api/corporations/{corporationId}/payouts",
            post(pay_corporation_dividend),
        )
        .route(
            "/api/corporations/{corporationId}/months",
            get(corporation_operating_months),
        )
        .route("/api/housing/listings", get(housing_listings))
        .route("/api/housing/leases/current", get(housing_lease_current))
        .route("/api/housing/leases", post(start_housing_lease))
        .route("/api/housing/holdings", get(housing_property_holdings))
        .route(
            "/api/housing/sales",
            get(property_sale_orders).post(create_property_sale_order),
        )
        .route(
            "/api/housing/sales/{orderId}/reprice",
            post(reprice_property_sale_order),
        )
        .route(
            "/api/housing/sales/{orderId}/cancel",
            post(cancel_property_sale_order),
        )
        .route(
            "/api/housing/holdings/{holdingId}/tax-events",
            get(property_tax_events),
        )
        .route("/api/housing/mortgage-quotes", post(quote_mortgage))
        .route("/api/housing/purchases", post(purchase_property))
        .route(
            "/api/housing/lease-deposit-loan-quotes",
            post(quote_lease_deposit_loan),
        )
        .route(
            "/api/housing/lease-arrears/{id}/payments",
            post(pay_lease_arrear),
        )
        .route("/api/loans/products", get(loan_products))
        .route("/api/loans/quotes", post(quote_loan))
        .route("/api/loans", post(execute_loan))
        .route("/api/loans/{loanId}", get(loan_detail))
        .route("/api/loans/{loanId}/installments", get(loan_installments))
        .route("/api/loans/{loanId}/prepayments", post(prepay_loan))
        .route("/api/credit", get(credit))
        .route("/api/life/budget", get(life_budget).put(update_life_budget))
        .route(
            "/api/life/arrears/{id}/payments",
            post(pay_essential_arrear),
        )
        .route("/api/career/specs", get(career_specs))
        .route(
            "/api/career/activities",
            get(career_activities).post(start_career_activity),
        )
        .route(
            "/api/career/activities/{id}/cancel",
            post(cancel_career_activity),
        )
        .route(
            "/api/career/artifacts",
            get(career_artifacts).post(publish_career_artifact),
        )
        .route("/api/career/focus", post(focus_career))
        .route("/api/career/jobs", get(career_jobs))
        .route(
            "/api/career/applications",
            get(career_applications).post(apply_career),
        )
        .route(
            "/api/career/applications/{id}/interview-confirmation",
            post(confirm_career_interview),
        )
        .route(
            "/api/career/applications/{id}/withdraw",
            post(withdraw_career_application),
        )
        .route(
            "/api/career/invitations/{id}/accept",
            post(accept_career_invitation),
        )
        .route(
            "/api/career/invitations/{id}/decline",
            post(decline_career_invitation),
        )
        .route("/api/career/offers/{id}/accept", post(accept_career_offer))
        .route(
            "/api/career/offers/{id}/decline",
            post(decline_career_offer),
        )
        .route("/api/career/employment", get(career_employment))
        .route("/api/career/payroll", get(career_payroll))
        .route(
            "/api/career/tax-years/{year}",
            get(career_employment_tax_year),
        )
        .route("/api/military/options", get(military_options))
        .route(
            "/api/military/service",
            get(military_service).post(start_military_service),
        )
        .route(
            "/api/military/savings-products",
            get(military_savings_products),
        )
        .route(
            "/api/military/savings",
            get(military_savings).post(open_military_savings),
        )
        .route(
            "/api/military/savings/{id}/close",
            post(close_military_savings),
        )
        .route("/api/markets/LLX/history", get(market_history))
        .route("/api/clock", post(clock))
        .route("/api/stream", get(stream))
        .merge(auth::router())
        .with_state(state)
        .merge(
            SwaggerUi::new("/api/docs")
                .url("/api/docs/openapi.json", ApiDoc::openapi())
                // Relative, so the spec URL follows the prefix nginx adds; an absolute path
                // would skip `/lifeledger` and resolve against the domain root
                .config(Config::from("./openapi.json")),
        )
}

#[utoipa::path(
    get,
    path = "/api/presets",
    responses((status = 200, description = "선택 가능한 시작 프리셋", body = [character::Preset]))
)]
async fn presets() -> Json<&'static [character::Preset]> {
    Json(character::presets())
}

#[utoipa::path(
    get,
    path = "/api/run-options",
    responses(
        (status = 200, description = "사용 가능한 실행 모드와 versioned 시작 catalog", body = RunOptions),
        (status = 500, description = "조회 실패")
    )
)]
async fn run_options(State(state): State<Arc<AppState>>) -> Result<Json<RunOptions>, AppError> {
    Ok(Json(state.run_options().await?))
}

#[utoipa::path(
    get,
    path = "/api/seasons/{id}/leagues",
    params(("id" = String, Path, description = "시즌 ID")),
    responses(
        (status = 200, description = "시즌과 분리된 공개 리그", body = SeasonLeagues),
        (status = 400, description = "시즌 ID 형식 오류", body = RunRequestFailure),
        (status = 404, description = "시즌 없음", body = RunRequestFailure),
        (status = 500, description = "조회 실패")
    )
)]
async fn season_leagues(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SeasonLeagues>, PointBudgetPreviewError> {
    let season_id = parse_run_resource_id(&id)?;
    let response = state
        .season_leagues(season_id)
        .await?
        .ok_or(PointBudgetPreviewError::VersionNotFound)?;

    Ok(Json(response))
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[into_params(parameter_in = Query)]
struct RankingPageParams {
    cursor: Option<String>,
    limit: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/api/leagues/{id}/rankings",
    params(
        ("id" = String, Path, description = "리그 ID"),
        RankingPageParams
    ),
    responses(
        (status = 200, description = "완료된 결산만 포함한 공개 랭킹", body = LeagueRankingPage),
        (status = 400, description = "리그 ID, cursor 또는 limit 형식 오류", body = RunRequestFailure),
        (status = 404, description = "리그 없음", body = RunRequestFailure),
        (status = 500, description = "조회 실패")
    )
)]
async fn league_rankings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    query: Result<Query<RankingPageParams>, QueryRejection>,
) -> Result<Json<LeagueRankingPage>, PointBudgetPreviewError> {
    let league_id = parse_run_resource_id(&id)?;
    let Query(query) = query.map_err(|_| PointBudgetPreviewError::InvalidCommand)?;
    let limit = query.limit.unwrap_or(DEFAULT_RANKING_PAGE_SIZE);
    if !(1..=MAX_RANKING_PAGE_SIZE).contains(&limit) {
        return Err(PointBudgetPreviewError::InvalidCommand);
    }
    let cursor = query
        .cursor
        .as_deref()
        .map(parse_ranking_cursor)
        .transpose()
        .map_err(|_| PointBudgetPreviewError::InvalidCommand)?;
    let response = state
        .league_rankings(league_id, cursor, limit)
        .await?
        .ok_or(PointBudgetPreviewError::VersionNotFound)?;

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/runs/{id}/finalization",
    params(("id" = u32, Path, description = "현재 계정의 run revision")),
    responses(
        (status = 200, description = "인증 계정이 소유한 ranked run 결산", body = RunFinalization),
        (status = 400, description = "run revision 형식 오류", body = RunRequestFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "소유한 ranked run 없음", body = RunRequestFailure),
        (status = 500, description = "조회 실패")
    ),
    security(("sessionCookie" = []))
)]
async fn run_finalization(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<RunFinalization>, PointBudgetPreviewError> {
    let run_revision = id
        .parse::<u32>()
        .map_err(|_| PointBudgetPreviewError::InvalidCommand)?;
    let response = state
        .run_finalization(user.id, run_revision)
        .await?
        .ok_or(PointBudgetPreviewError::VersionNotFound)?;
    Ok(Json(response))
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum PlaytestConsentActionValue {
    Grant,
    Withdraw,
}

impl From<PlaytestConsentActionValue> for ConsentAction {
    fn from(value: PlaytestConsentActionValue) -> Self {
        match value {
            PlaytestConsentActionValue::Grant => Self::Grant,
            PlaytestConsentActionValue::Withdraw => Self::Withdraw,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum PlaytestFeedbackCategoryValue {
    Bug,
    Balance,
    Usability,
    Performance,
    Rules,
    Other,
}

impl From<PlaytestFeedbackCategoryValue> for DomainFeedbackCategory {
    fn from(value: PlaytestFeedbackCategoryValue) -> Self {
        match value {
            PlaytestFeedbackCategoryValue::Bug => Self::Bug,
            PlaytestFeedbackCategoryValue::Balance => Self::Balance,
            PlaytestFeedbackCategoryValue::Usability => Self::Usability,
            PlaytestFeedbackCategoryValue::Performance => Self::Performance,
            PlaytestFeedbackCategoryValue::Rules => Self::Rules,
            PlaytestFeedbackCategoryValue::Other => Self::Other,
        }
    }
}

impl From<DomainFeedbackCategory> for PlaytestFeedbackCategoryValue {
    fn from(value: DomainFeedbackCategory) -> Self {
        match value {
            DomainFeedbackCategory::Bug => Self::Bug,
            DomainFeedbackCategory::Balance => Self::Balance,
            DomainFeedbackCategory::Usability => Self::Usability,
            DomainFeedbackCategory::Performance => Self::Performance,
            DomainFeedbackCategory::Rules => Self::Rules,
            DomainFeedbackCategory::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum PlaytestFeedbackSeverityValue {
    Blocking,
    Major,
    Minor,
    Suggestion,
}

impl From<PlaytestFeedbackSeverityValue> for DomainFeedbackSeverity {
    fn from(value: PlaytestFeedbackSeverityValue) -> Self {
        match value {
            PlaytestFeedbackSeverityValue::Blocking => Self::Blocking,
            PlaytestFeedbackSeverityValue::Major => Self::Major,
            PlaytestFeedbackSeverityValue::Minor => Self::Minor,
            PlaytestFeedbackSeverityValue::Suggestion => Self::Suggestion,
        }
    }
}

impl From<DomainFeedbackSeverity> for PlaytestFeedbackSeverityValue {
    fn from(value: DomainFeedbackSeverity) -> Self {
        match value {
            DomainFeedbackSeverity::Blocking => Self::Blocking,
            DomainFeedbackSeverity::Major => Self::Major,
            DomainFeedbackSeverity::Minor => Self::Minor,
            DomainFeedbackSeverity::Suggestion => Self::Suggestion,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum PlaytestConsentStatusSnapshot {
    NotGranted,
    Granted,
    Withdrawn,
    PolicyChanged,
}

impl From<ConsentDisplayStatus> for PlaytestConsentStatusSnapshot {
    fn from(value: ConsentDisplayStatus) -> Self {
        match value {
            ConsentDisplayStatus::NotGranted => Self::NotGranted,
            ConsentDisplayStatus::Granted => Self::Granted,
            ConsentDisplayStatus::Withdrawn => Self::Withdrawn,
            ConsentDisplayStatus::PolicyChanged => Self::PolicyChanged,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum AnalyticsCollectionSnapshot {
    Disabled,
}

impl From<AnalyticsCollection> for AnalyticsCollectionSnapshot {
    fn from(value: AnalyticsCollection) -> Self {
        match value {
            AnalyticsCollection::Disabled => Self::Disabled,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlaytestConsentPolicySnapshot {
    id: String,
    scope: String,
    policy_key: String,
    version: u32,
    schema_version: u16,
    display_name: String,
    notice_text: String,
    canonical_sha256: String,
    analytics_collection: AnalyticsCollectionSnapshot,
    retention_maximum_days: u16,
    maximum_active_feedback: u64,
    message_maximum_characters: usize,
}

impl From<DomainConsentPolicy> for PlaytestConsentPolicySnapshot {
    fn from(value: DomainConsentPolicy) -> Self {
        Self {
            id: value.id.to_string(),
            scope: value.scope,
            policy_key: value.policy_key,
            version: value.version,
            schema_version: value.schema_version,
            display_name: value.display_name,
            notice_text: value.notice_text,
            canonical_sha256: value.canonical_sha256,
            analytics_collection: value.analytics_collection.into(),
            retention_maximum_days: value.retention_maximum_days,
            maximum_active_feedback: value.maximum_active_feedback,
            message_maximum_characters: value.message_maximum_characters,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlaytestConsentSnapshot {
    status: PlaytestConsentStatusSnapshot,
    revision: u64,
    policy_version_id: Option<String>,
    granted_at: Option<String>,
    withdrawn_at: Option<String>,
}

impl From<DomainConsentState> for PlaytestConsentSnapshot {
    fn from(value: DomainConsentState) -> Self {
        Self {
            status: value.status.into(),
            revision: value.revision,
            policy_version_id: value.policy_version_id.map(|id| id.to_string()),
            granted_at: value.granted_at,
            withdrawn_at: value.withdrawn_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlaytestFeedbackSnapshot {
    id: String,
    category: PlaytestFeedbackCategoryValue,
    severity: PlaytestFeedbackSeverityValue,
    message: String,
    run_revision: Option<u32>,
    run_manifest_sha256: Option<String>,
    finalization_sha256: Option<String>,
    created_at: String,
}

impl From<DomainFeedbackItem> for PlaytestFeedbackSnapshot {
    fn from(value: DomainFeedbackItem) -> Self {
        Self {
            id: value.id,
            category: value.category.into(),
            severity: value.severity.into(),
            message: value.message,
            run_revision: value.run_revision,
            run_manifest_sha256: value.run_manifest_sha256,
            finalization_sha256: value.finalization_sha256,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlaytestFeedbackOverviewResponse {
    policy: PlaytestConsentPolicySnapshot,
    consent: PlaytestConsentSnapshot,
    feedback: Vec<PlaytestFeedbackSnapshot>,
}

impl From<DomainPlaytestFeedbackOverview> for PlaytestFeedbackOverviewResponse {
    fn from(value: DomainPlaytestFeedbackOverview) -> Self {
        Self {
            policy: value.policy.into(),
            consent: value.consent.into(),
            feedback: value.feedback.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlaytestConsentRequest {
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]{0,19}$")]
    policy_version_id: String,
    #[schema(maximum = 9007199254740991_u64)]
    expected_revision: u64,
    action: PlaytestConsentActionValue,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlaytestConsentUpdateResponse {
    consent: PlaytestConsentSnapshot,
    purged_feedback_count: u64,
}

impl From<DomainConsentUpdate> for PlaytestConsentUpdateResponse {
    fn from(value: DomainConsentUpdate) -> Self {
        Self {
            consent: value.consent.into(),
            purged_feedback_count: value.purged_feedback_count,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlaytestFeedbackRequest {
    #[schema(minimum = 1, maximum = 9007199254740991_u64)]
    expected_consent_revision: u64,
    category: PlaytestFeedbackCategoryValue,
    severity: PlaytestFeedbackSeverityValue,
    #[schema(min_length = 1, max_length = 500)]
    message: String,
    privacy_confirmed: bool,
    run_revision: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct PlaytestFeedbackDeletionResponse {
    id: String,
    status: &'static str,
    withdrawn_at: String,
}

impl From<DomainFeedbackDeletion> for PlaytestFeedbackDeletionResponse {
    fn from(value: DomainFeedbackDeletion) -> Self {
        Self {
            id: value.id,
            status: "withdrawn",
            withdrawn_at: value.withdrawn_at,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum PlaytestFailureCodeSnapshot {
    InvalidCommand,
    PolicyUnavailable,
    RevisionConflict,
    ConsentRequired,
    PrivacyConfirmationRequired,
    FeedbackCapacityReached,
    RunReferenceNotFound,
    FeedbackNotFound,
}

impl From<PlaytestFailureCode> for PlaytestFailureCodeSnapshot {
    fn from(value: PlaytestFailureCode) -> Self {
        match value {
            PlaytestFailureCode::InvalidCommand => Self::InvalidCommand,
            PlaytestFailureCode::PolicyUnavailable => Self::PolicyUnavailable,
            PlaytestFailureCode::RevisionConflict => Self::RevisionConflict,
            PlaytestFailureCode::ConsentRequired => Self::ConsentRequired,
            PlaytestFailureCode::PrivacyConfirmationRequired => Self::PrivacyConfirmationRequired,
            PlaytestFailureCode::FeedbackCapacityReached => Self::FeedbackCapacityReached,
            PlaytestFailureCode::RunReferenceNotFound => Self::RunReferenceNotFound,
            PlaytestFailureCode::FeedbackNotFound => Self::FeedbackNotFound,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
struct PlaytestFailure {
    code: PlaytestFailureCodeSnapshot,
}

enum PlaytestRequestError {
    Domain(PlaytestFailureCode),
    Internal(AppError),
}

impl From<anyhow::Error> for PlaytestRequestError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for PlaytestRequestError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Domain(code) => {
                let status = match code {
                    PlaytestFailureCode::InvalidCommand
                    | PlaytestFailureCode::PrivacyConfirmationRequired => StatusCode::BAD_REQUEST,
                    PlaytestFailureCode::RunReferenceNotFound
                    | PlaytestFailureCode::FeedbackNotFound => StatusCode::NOT_FOUND,
                    PlaytestFailureCode::PolicyUnavailable
                    | PlaytestFailureCode::RevisionConflict
                    | PlaytestFailureCode::ConsentRequired
                    | PlaytestFailureCode::FeedbackCapacityReached => StatusCode::CONFLICT,
                };
                (status, Json(PlaytestFailure { code: code.into() })).into_response()
            }
            Self::Internal(error) => error.into_response(),
        }
    }
}

fn accepted_playtest<T>(result: PlaytestStoreResult<T>) -> Result<T, PlaytestRequestError> {
    match result {
        PlaytestStoreResult::Accepted(value) => Ok(value),
        PlaytestStoreResult::Rejected(code) => Err(PlaytestRequestError::Domain(code)),
    }
}

fn parse_playtest_policy_id(value: &str) -> Result<u64, PlaytestRequestError> {
    value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or(PlaytestRequestError::Domain(
            PlaytestFailureCode::InvalidCommand,
        ))
}

#[utoipa::path(
    get,
    path = "/api/playtest/feedback",
    responses(
        (status = 200, description = "현재 고지·동의와 소유한 활성 피드백", body = PlaytestFeedbackOverviewResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "조회 실패")
    ),
    security(("sessionCookie" = []))
)]
async fn playtest_feedback_overview(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<PlaytestFeedbackOverviewResponse>, PlaytestRequestError> {
    Ok(Json(
        state.playtest_feedback_overview(user.id).await?.into(),
    ))
}

#[utoipa::path(
    put,
    path = "/api/playtest/consent",
    request_body = PlaytestConsentRequest,
    responses(
        (status = 200, description = "동의 또는 철회 결과", body = PlaytestConsentUpdateResponse),
        (status = 400, description = "strict 요청 형식 오류", body = PlaytestFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "policy·revision·동의 충돌", body = PlaytestFailure),
        (status = 500, description = "저장 실패")
    ),
    security(("sessionCookie" = []))
)]
async fn set_playtest_consent(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<PlaytestConsentRequest>, JsonRejection>,
) -> Result<Json<PlaytestConsentUpdateResponse>, PlaytestRequestError> {
    let Json(request) =
        request.map_err(|_| PlaytestRequestError::Domain(PlaytestFailureCode::InvalidCommand))?;
    if request.expected_revision > MAX_JSON_SAFE_INTEGER {
        return Err(PlaytestRequestError::Domain(
            PlaytestFailureCode::InvalidCommand,
        ));
    }
    let result = state
        .set_playtest_consent(
            user.id,
            ConsentCommand {
                policy_version_id: parse_playtest_policy_id(&request.policy_version_id)?,
                expected_revision: request.expected_revision,
                action: request.action.into(),
            },
        )
        .await?;

    Ok(Json(accepted_playtest(result)?.into()))
}

#[utoipa::path(
    post,
    path = "/api/playtest/feedback",
    request_body = PlaytestFeedbackRequest,
    responses(
        (status = 200, description = "서버가 hash를 해석해 저장한 피드백", body = PlaytestFeedbackSnapshot),
        (status = 400, description = "strict 요청·본문·개인정보 확인 오류", body = PlaytestFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "소유한 run 없음", body = PlaytestFailure),
        (status = 409, description = "동의·revision·용량 충돌", body = PlaytestFailure),
        (status = 500, description = "저장 실패")
    ),
    security(("sessionCookie" = []))
)]
async fn submit_playtest_feedback(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<PlaytestFeedbackRequest>, JsonRejection>,
) -> Result<Json<PlaytestFeedbackSnapshot>, PlaytestRequestError> {
    let Json(request) =
        request.map_err(|_| PlaytestRequestError::Domain(PlaytestFailureCode::InvalidCommand))?;
    if request.expected_consent_revision > MAX_JSON_SAFE_INTEGER {
        return Err(PlaytestRequestError::Domain(
            PlaytestFailureCode::InvalidCommand,
        ));
    }
    let result = state
        .submit_playtest_feedback(
            user.id,
            FeedbackDraft {
                expected_consent_revision: request.expected_consent_revision,
                category: request.category.into(),
                severity: request.severity.into(),
                message: request.message,
                privacy_confirmed: request.privacy_confirmed,
                run_revision: request.run_revision,
            },
        )
        .await?;

    Ok(Json(accepted_playtest(result)?.into()))
}

#[utoipa::path(
    delete,
    path = "/api/playtest/feedback/{id}",
    params(("id" = String, Path, description = "서버가 발급한 feedback UUID")),
    responses(
        (status = 200, description = "피드백 tombstone 결과", body = PlaytestFeedbackDeletionResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "소유한 피드백 없음", body = PlaytestFailure),
        (status = 500, description = "삭제 실패")
    ),
    security(("sessionCookie" = []))
)]
async fn delete_playtest_feedback(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<PlaytestFeedbackDeletionResponse>, PlaytestRequestError> {
    let result = state.delete_playtest_feedback(user.id, &id).await?;
    Ok(Json(accepted_playtest(result)?.into()))
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum OfflineSettingStatusSnapshot {
    Active,
    PausedBySystem,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum ProgressHolderKindSnapshot {
    Online,
    Worker,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct OfflinePolicySnapshot {
    id: String,
    canonical_sha256: String,
    engine_version: String,
    cadence_seconds: u32,
    absence_window_cap_days: u32,
    max_worker_batch_days: u16,
    lease_seconds: u16,
    presence_ttl_seconds: u16,
    heartbeat_seconds: u16,
    online_intent_ttl_seconds: u16,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ProgressLeaseSnapshot {
    holder_kind: ProgressHolderKindSnapshot,
    generation: String,
    expires_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct OfflineProgressResponse {
    run_revision: u32,
    available: bool,
    #[schema(required = true, nullable)]
    policy: Option<OfflinePolicySnapshot>,
    enabled: bool,
    status: OfflineSettingStatusSnapshot,
    #[schema(required = true, nullable)]
    absence_started_at: Option<String>,
    #[schema(required = true, nullable)]
    accrued_through: Option<String>,
    #[schema(required = true, nullable)]
    accrual_limit_at: Option<String>,
    window_accrued_days: u32,
    pending_days: u32,
    processed_days: String,
    cancelled_pending_days: String,
    revision: String,
    #[schema(required = true, nullable)]
    last_error_code: Option<String>,
    online: bool,
    #[schema(required = true, nullable)]
    lease: Option<ProgressLeaseSnapshot>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OfflineProgressUpdateRequest {
    #[schema(pattern = "^(0|[1-9][0-9]{0,19})$")]
    expected_revision: String,
    enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum OfflineProgressFailureCode {
    InvalidCommand,
    CharacterRequired,
    PolicyUnavailable,
    RevisionConflict,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct OfflineProgressFailureResponse {
    code: OfflineProgressFailureCode,
    message: &'static str,
}

enum OfflineProgressApiError {
    InvalidCommand,
    Rejected(OfflineProgressFailure),
    Internal(AppError),
}

impl From<anyhow::Error> for OfflineProgressApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for OfflineProgressApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = match self {
            Self::InvalidCommand => (
                StatusCode::BAD_REQUEST,
                OfflineProgressFailureCode::InvalidCommand,
                "오프라인 진행 요청 형식이 올바르지 않습니다",
            ),
            Self::Rejected(OfflineProgressFailure::CharacterRequired) => (
                StatusCode::CONFLICT,
                OfflineProgressFailureCode::CharacterRequired,
                "먼저 실행을 시작해야 합니다",
            ),
            Self::Rejected(OfflineProgressFailure::PolicyUnavailable) => (
                StatusCode::CONFLICT,
                OfflineProgressFailureCode::PolicyUnavailable,
                "현재 실행은 오프라인 진행 정책을 허용하지 않습니다",
            ),
            Self::Rejected(OfflineProgressFailure::RevisionConflict) => (
                StatusCode::CONFLICT,
                OfflineProgressFailureCode::RevisionConflict,
                "오프라인 진행 설정이 이미 변경되었습니다",
            ),
            Self::Internal(error) => return error.into_response(),
        };
        (
            status,
            Json(OfflineProgressFailureResponse { code, message }),
        )
            .into_response()
    }
}

#[utoipa::path(
    get,
    path = "/api/offline-progress/status",
    responses(
        (status = 200, description = "현재 실행의 오프라인 진행·presence·lease 상태", body = OfflineProgressResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "조회 실패")
    ),
    security(("sessionCookie" = []))
)]
async fn offline_progress_status(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<OfflineProgressResponse>, OfflineProgressApiError> {
    Ok(Json(to_offline_progress_response(
        state.offline_progress_status(user.id).await?,
    )?))
}

#[utoipa::path(
    put,
    path = "/api/offline-progress",
    request_body = OfflineProgressUpdateRequest,
    responses(
        (status = 200, description = "revision 조건으로 갱신한 opt-in/out 상태", body = OfflineProgressResponse),
        (status = 400, description = "strict 요청 형식 오류", body = OfflineProgressFailureResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "실행·정책·revision 충돌", body = OfflineProgressFailureResponse),
        (status = 500, description = "갱신 실패")
    ),
    security(("sessionCookie" = []))
)]
async fn set_offline_progress(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<OfflineProgressUpdateRequest>, JsonRejection>,
) -> Result<Json<OfflineProgressResponse>, OfflineProgressApiError> {
    let request = request
        .map_err(|_| OfflineProgressApiError::InvalidCommand)?
        .0;
    let expected_revision = parse_revision(&request.expected_revision)
        .ok_or(OfflineProgressApiError::InvalidCommand)?;
    match state
        .set_offline_progress(user.id, expected_revision, request.enabled)
        .await?
    {
        OfflineProgressUpdateResult::Updated(status) => {
            Ok(Json(to_offline_progress_response(*status)?))
        }
        OfflineProgressUpdateResult::Rejected(failure) => {
            Err(OfflineProgressApiError::Rejected(failure))
        }
    }
}

fn to_offline_progress_response(
    state: OfflineProgressState,
) -> Result<OfflineProgressResponse, OfflineProgressApiError> {
    let available = state.policy.is_some();
    let policy = state.policy.map(|policy| OfflinePolicySnapshot {
        id: policy.id.get().to_string(),
        canonical_sha256: policy.canonical_sha256,
        engine_version: policy.engine_version,
        cadence_seconds: policy.cadence_seconds,
        absence_window_cap_days: policy.absence_window_cap_days,
        max_worker_batch_days: policy.max_worker_batch_days,
        lease_seconds: policy.lease_seconds,
        presence_ttl_seconds: policy.presence_ttl_seconds,
        heartbeat_seconds: policy.heartbeat_seconds,
        online_intent_ttl_seconds: policy.online_intent_ttl_seconds,
    });
    let status = match state.setting_status {
        OfflineProgressSettingStatus::Active => OfflineSettingStatusSnapshot::Active,
        OfflineProgressSettingStatus::PausedBySystem => {
            OfflineSettingStatusSnapshot::PausedBySystem
        }
    };
    let lease = state.lease.map(|lease| ProgressLeaseSnapshot {
        holder_kind: match lease.holder_kind {
            ProgressHolderKind::Online => ProgressHolderKindSnapshot::Online,
            ProgressHolderKind::Worker => ProgressHolderKindSnapshot::Worker,
        },
        generation: lease.generation.to_string(),
        expires_at: lease.expires_at,
    });
    Ok(OfflineProgressResponse {
        run_revision: state.run_revision,
        available,
        policy,
        enabled: state.enabled,
        status,
        absence_started_at: state.absence_started_at,
        accrued_through: state.accrued_through,
        accrual_limit_at: state.accrual_limit_at,
        window_accrued_days: state.window_accrued_days,
        pending_days: state.pending_days,
        processed_days: state.processed_days.to_string(),
        cancelled_pending_days: state.cancelled_pending_days.to_string(),
        revision: state.revision.to_string(),
        last_error_code: state.last_error_code,
        online: state.online,
        lease,
    })
}

fn parse_revision(raw: &str) -> Option<u64> {
    if raw == "0" {
        return Some(0);
    }
    if raw.is_empty()
        || raw.len() > 20
        || raw.starts_with('0')
        || !raw.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    raw.parse().ok()
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PointBudgetPreviewRequest {
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]{0,19}$")]
    point_budget_version_id: String,
    #[schema(max_items = 64)]
    selections: Vec<PointSelectionRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PointSelectionRequest {
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]{0,19}$")]
    option_id: String,
    #[schema(minimum = 1, maximum = 1000000)]
    quantity: u32,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum RunRequestFailureCode {
    InvalidCommand,
    VersionNotFound,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunRequestFailure {
    code: RunRequestFailureCode,
}

enum PointBudgetPreviewError {
    InvalidCommand,
    VersionNotFound,
    Internal(AppError),
}

impl From<anyhow::Error> for PointBudgetPreviewError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for PointBudgetPreviewError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::InvalidCommand => (
                StatusCode::BAD_REQUEST,
                Json(RunRequestFailure {
                    code: RunRequestFailureCode::InvalidCommand,
                }),
            )
                .into_response(),
            Self::VersionNotFound => (
                StatusCode::NOT_FOUND,
                Json(RunRequestFailure {
                    code: RunRequestFailureCode::VersionNotFound,
                }),
            )
                .into_response(),
            Self::Internal(error) => error.into_response(),
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/runs/point-preview",
    request_body = PointBudgetPreviewRequest,
    responses(
        (status = 200, description = "서버가 계산한 point ledger", body = PointBudgetEvaluation),
        (status = 400, description = "strict 요청 형식 또는 범위 오류", body = RunRequestFailure),
        (status = 404, description = "사용할 수 없는 budget version", body = RunRequestFailure),
        (status = 500, description = "조회 실패")
    )
)]
async fn preview_point_budget(
    State(state): State<Arc<AppState>>,
    request: Result<Json<PointBudgetPreviewRequest>, JsonRejection>,
) -> Result<Json<PointBudgetEvaluation>, PointBudgetPreviewError> {
    let Json(request) = request.map_err(|_| PointBudgetPreviewError::InvalidCommand)?;
    if request.selections.len() > 64 {
        return Err(PointBudgetPreviewError::InvalidCommand);
    }
    let version_id = parse_run_resource_id(&request.point_budget_version_id)?;
    let selections = request
        .selections
        .into_iter()
        .map(|selection| {
            if selection.quantity == 0 || selection.quantity > 1_000_000 {
                return Err(PointBudgetPreviewError::InvalidCommand);
            }
            Ok(PointSelection {
                option_id: parse_run_resource_id(&selection.option_id)?,
                quantity: selection.quantity,
            })
        })
        .collect::<Result<Vec<_>, PointBudgetPreviewError>>()?;
    let evaluation = state
        .preview_point_budget(version_id, &selections)
        .await?
        .ok_or(PointBudgetPreviewError::VersionNotFound)?;

    Ok(Json(evaluation))
}

fn parse_run_resource_id(raw: &str) -> Result<ResourceId, PointBudgetPreviewError> {
    raw.parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .map(ResourceId::from_u64)
        .ok_or(PointBudgetPreviewError::InvalidCommand)
}

#[derive(Deserialize, ToSchema)]
#[serde(tag = "mode", rename_all = "camelCase")]
enum RunStartRequest {
    RankedPreset(RankedPresetRunStartRequest),
    RankedCustom(RankedCustomRunStartRequest),
    Sandbox(SandboxRunStartRequest),
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RankedPresetRunStartRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]{0,19}$")]
    character_preset_version_id: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RankedCustomRunStartRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]{0,19}$")]
    point_budget_version_id: String,
    #[schema(max_items = 64)]
    selections: Vec<PointSelectionRequest>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SandboxRunStartRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    character: CharacterStartProfile,
    #[schema(max_items = 2)]
    starting_loans: Vec<CharacterStartingLoan>,
}

enum RunStartAction {
    Start(Box<StartGameCommand>),
    RankedPreset {
        command_id: CommandId,
        cursor: CommandCursor,
        preset_version_id: ResourceId,
    },
    RankedCustom {
        command_id: CommandId,
        cursor: CommandCursor,
        budget_version_id: ResourceId,
        selections: Vec<PointSelection>,
    },
}

impl RunStartRequest {
    fn into_action(self) -> Result<RunStartAction, GameLoopError> {
        match self {
            Self::RankedPreset(request) => request.into_action(),
            Self::RankedCustom(request) => request.into_action(),
            Self::Sandbox(request) => request.into_action(),
        }
    }
}

impl RankedPresetRunStartRequest {
    fn into_action(self) -> Result<RunStartAction, GameLoopError> {
        Ok(RunStartAction::RankedPreset {
            command_id: parse_start_command_id(self.command_id)?,
            cursor: start_cursor(
                self.expected_run_revision,
                self.expected_state_revision,
                self.expected_game_day,
            ),
            preset_version_id: parse_start_resource_id(&self.character_preset_version_id)?,
        })
    }
}

impl RankedCustomRunStartRequest {
    fn into_action(self) -> Result<RunStartAction, GameLoopError> {
        if self.selections.len() > 64 {
            return Err(GameLoopError::InvalidCommand);
        }
        let selections = self
            .selections
            .into_iter()
            .map(|selection| {
                if selection.quantity == 0 || selection.quantity > 1_000_000 {
                    return Err(GameLoopError::InvalidCommand);
                }
                Ok(PointSelection {
                    option_id: parse_start_resource_id(&selection.option_id)?,
                    quantity: selection.quantity,
                })
            })
            .collect::<Result<Vec<_>, GameLoopError>>()?;

        Ok(RunStartAction::RankedCustom {
            command_id: parse_start_command_id(self.command_id)?,
            cursor: start_cursor(
                self.expected_run_revision,
                self.expected_state_revision,
                self.expected_game_day,
            ),
            budget_version_id: parse_start_resource_id(&self.point_budget_version_id)?,
            selections,
        })
    }
}

impl SandboxRunStartRequest {
    fn into_action(self) -> Result<RunStartAction, GameLoopError> {
        let mut command = CharacterStartV2Request {
            command_id: self.command_id,
            expected_run_revision: self.expected_run_revision,
            expected_state_revision: self.expected_state_revision,
            expected_game_day: self.expected_game_day,
            character: self.character,
            starting_loans: self.starting_loans,
        }
        .into_command()?;
        command.manifest_kind = StartGameManifestKind::Sandbox;

        Ok(RunStartAction::Start(Box::new(command)))
    }
}

fn parse_start_resource_id(raw: &str) -> Result<ResourceId, GameLoopError> {
    raw.parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .map(ResourceId::from_u64)
        .ok_or(GameLoopError::InvalidCommand)
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RunStartResponse {
    mode: RunMode,
    manifest_sha256: String,
    start: CharacterStartSnapshot,
    snapshot: GameSnapshot,
}

#[utoipa::path(
    post,
    path = "/api/runs",
    request_body = RunStartRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "immutable manifest와 함께 생성된 실행", body = RunStartResponse),
        (status = 400, description = "strict 요청 형식 또는 범위 오류", body = GameCommandFailure),
        (status = 409, description = "mode, command ID 또는 cursor 충돌", body = GameCommandFailure),
        (status = 422, description = "sandbox 시작 조건이 서로 모순됨", body = ValidationFailure),
        (status = 500, description = "저장 또는 manifest 조회 실패")
    )
)]
async fn create_run(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<RunStartRequest>, JsonRejection>,
) -> Result<Json<RunStartResponse>, CreateRunError> {
    let Json(request) =
        request.map_err(|_| CreateRunError::Command(GameLoopError::InvalidCommand))?;
    let command = match request.into_action().map_err(CreateRunError::Command)? {
        RunStartAction::Start(command) => *command,
        RunStartAction::RankedPreset {
            command_id,
            cursor,
            preset_version_id,
        } => {
            let preparation = state
                .prepare_ranked_preset(preset_version_id)
                .await?
                .ok_or(CreateRunError::ModeUnavailable)?;
            StartGameCommand {
                command_id,
                cursor,
                draft: preparation.draft,
                starting_loans: None,
                manifest_kind: StartGameManifestKind::Ranked(preparation.context),
            }
        }
        RunStartAction::RankedCustom {
            command_id,
            cursor,
            budget_version_id,
            selections,
        } => {
            let preparation = state
                .prepare_ranked_custom(budget_version_id, &selections)
                .await?
                .ok_or(CreateRunError::ModeUnavailable)?;
            StartGameCommand {
                command_id,
                cursor,
                draft: preparation.draft,
                starting_loans: None,
                manifest_kind: StartGameManifestKind::Ranked(preparation.context),
            }
        }
    };
    let expected_mode = command.manifest_kind.run_mode();
    let response = state
        .start_game(user.id, &command)
        .await
        .map_err(CreateRunError::Command)?;
    let run_revision = response.start.committed_cursor.run_revision;
    let manifest = state
        .run_manifest(user.id, run_revision)
        .await?
        .ok_or_else(|| anyhow::anyhow!("committed run has no manifest"))?;
    if manifest.run_revision != run_revision || manifest.mode != expected_mode {
        return Err(anyhow::anyhow!("committed run manifest disagrees with response").into());
    }

    Ok(Json(RunStartResponse {
        mode: manifest.mode,
        manifest_sha256: manifest.manifest_sha256,
        start: response.start,
        snapshot: response.snapshot,
    }))
}

enum CreateRunError {
    ModeUnavailable,
    Command(GameLoopError),
    Internal(AppError),
}

impl From<anyhow::Error> for CreateRunError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for CreateRunError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::ModeUnavailable => (
                StatusCode::CONFLICT,
                Json(GameCommandFailure {
                    code: GameCommandFailureCode::ModeUnavailable,
                    message: "현재 게시된 ranked season이 없습니다",
                }),
            )
                .into_response(),
            Self::Command(error) => GameCommandError(error).into_response(),
            Self::Internal(error) => error.into_response(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(untagged)]
enum CharacterStartRequest {
    V2(CharacterStartV2Request),
    V1(CharacterStartV1Request),
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CharacterStartV1Request {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    character: character::CharacterDraft,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CharacterStartV2Request {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    character: CharacterStartProfile,
    #[schema(max_items = 2)]
    starting_loans: Vec<CharacterStartingLoan>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CharacterStartProfile {
    name: String,
    age: u32,
    gender: character::Gender,
    military: character::MilitaryStatus,
    region: character::Region,
    background: character::FamilyBackground,
    education: character::Education,
    career_years: u32,
    certifications: u32,
    starting_cash_krw: i64,
    health: character::Health,
    dependents: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum CharacterStartingLoanKind {
    StudentLoan,
    UnsecuredLoan,
}

impl CharacterStartingLoanKind {
    const fn order(self) -> u8 {
        match self {
            Self::StudentLoan => 0,
            Self::UnsecuredLoan => 1,
        }
    }

    const fn product_kind(self) -> LoanProductKind {
        match self {
            Self::StudentLoan => LoanProductKind::StudentLoan,
            Self::UnsecuredLoan => LoanProductKind::UnsecuredLoan,
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CharacterStartingLoan {
    kind: CharacterStartingLoanKind,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]{0,19}$")]
    product_version_id: String,
    #[schema(minimum = 1)]
    principal_krw: i64,
}

impl CharacterStartRequest {
    fn into_command(self) -> Result<StartGameCommand, GameLoopError> {
        match self {
            Self::V1(request) => request.into_command(),
            Self::V2(request) => request.into_command(),
        }
    }
}

impl CharacterStartV1Request {
    fn into_command(self) -> Result<StartGameCommand, GameLoopError> {
        Ok(StartGameCommand {
            command_id: parse_start_command_id(self.command_id)?,
            cursor: start_cursor(
                self.expected_run_revision,
                self.expected_state_revision,
                self.expected_game_day,
            ),
            draft: self.character,
            starting_loans: None,
            manifest_kind: StartGameManifestKind::LegacySandbox,
        })
    }
}

impl CharacterStartV2Request {
    fn into_command(self) -> Result<StartGameCommand, GameLoopError> {
        if self.starting_loans.len() > 2 {
            return Err(GameLoopError::InvalidCommand);
        }
        let mut prior_order = None;
        let mut student_loan_krw = 0_i64;
        let mut credit_loan_krw = 0_i64;
        let mut starting_loans = Vec::with_capacity(self.starting_loans.len());
        for loan in self.starting_loans {
            let order = loan.kind.order();
            if loan.principal_krw <= 0 || prior_order.is_some_and(|prior| order <= prior) {
                return Err(GameLoopError::InvalidCommand);
            }
            let product_version_id = loan
                .product_version_id
                .parse::<u64>()
                .ok()
                .filter(|id| *id > 0)
                .map(ResourceId::from_u64)
                .ok_or(GameLoopError::InvalidCommand)?;
            match loan.kind {
                CharacterStartingLoanKind::StudentLoan => {
                    student_loan_krw = loan.principal_krw;
                }
                CharacterStartingLoanKind::UnsecuredLoan => {
                    credit_loan_krw = loan.principal_krw;
                }
            }
            starting_loans.push(StartingLoanCommand {
                product_version_id,
                product_kind: loan.kind.product_kind(),
                principal_krw: loan.principal_krw,
            });
            prior_order = Some(order);
        }
        Ok(StartGameCommand {
            command_id: parse_start_command_id(self.command_id)?,
            cursor: start_cursor(
                self.expected_run_revision,
                self.expected_state_revision,
                self.expected_game_day,
            ),
            draft: self.character.into_draft(student_loan_krw, credit_loan_krw),
            starting_loans: Some(starting_loans),
            manifest_kind: StartGameManifestKind::LegacySandbox,
        })
    }
}

impl CharacterStartProfile {
    fn into_draft(self, student_loan_krw: i64, credit_loan_krw: i64) -> character::CharacterDraft {
        character::CharacterDraft {
            name: self.name,
            age: self.age,
            gender: self.gender,
            military: self.military,
            region: self.region,
            background: self.background,
            education: self.education,
            career_years: self.career_years,
            certifications: self.certifications,
            starting_cash_krw: self.starting_cash_krw,
            student_loan_krw,
            credit_loan_krw,
            health: self.health,
            dependents: self.dependents,
        }
    }
}

fn parse_start_command_id(raw: String) -> Result<CommandId, GameLoopError> {
    CommandId::parse(raw).map_err(|_| GameLoopError::InvalidCommand)
}

const fn start_cursor(
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
) -> CommandCursor {
    CommandCursor {
        expected_run_revision,
        expected_state_revision,
        expected_game_day,
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ValidationFailure {
    errors: Vec<character::ValidationError>,
}

/// Character creation. The domain validates (§3.5); this only picks a status code.
#[utoipa::path(
    post,
    path = "/api/characters",
    request_body = CharacterStartRequest,
    responses(
        (status = 200, description = "캐릭터 시작 명령 결과와 최신 스냅샷", body = CharacterStartResponse),
        (status = 400, description = "명령 형식이 잘못됨", body = GameCommandFailure),
        (status = 409, description = "명령 충돌 또는 오래된 커서", body = GameCommandFailure),
        (status = 422, description = "시작 조건이 서로 모순됨", body = ValidationFailure),
        (status = 500, description = "저장 실패"),
    )
)]
async fn create_character(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<CharacterStartRequest>, JsonRejection>,
) -> Result<Json<CharacterStartResponse>, CreateCharacterError> {
    let Json(request) =
        request.map_err(|_| CreateCharacterError::Command(GameLoopError::InvalidCommand))?;
    let command = request
        .into_command()
        .map_err(CreateCharacterError::Command)?;

    Ok(Json(state.start_game(user.id, &command).await?))
}

/// 422 and 500 have different causes, so they have different response shapes.
enum CreateCharacterError {
    Invalid(Vec<character::ValidationError>),
    Command(GameLoopError),
    Internal(AppError),
}

impl From<GameLoopError> for CreateCharacterError {
    fn from(error: GameLoopError) -> Self {
        match error {
            GameLoopError::InvalidCharacter(errors) => Self::Invalid(errors),
            error => Self::Command(error),
        }
    }
}

impl From<anyhow::Error> for CreateCharacterError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for CreateCharacterError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Invalid(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ValidationFailure { errors }),
            )
                .into_response(),
            Self::Command(error) => GameCommandError(error).into_response(),
            Self::Internal(error) => error.into_response(),
        }
    }
}

#[derive(Serialize, ToSchema)]
struct Health {
    status: &'static str,
    version: &'static str,
}

#[utoipa::path(
    get,
    path = "/api/health",
    responses((status = 200, description = "서버가 살아 있음", body = Health))
)]
async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[utoipa::path(
    get,
    path = "/api/state",
    responses(
        (status = 200, description = "현재 게임 상태", body = GameSnapshot),
        (status = 500, description = "조회 실패"),
    )
)]
async fn snapshot(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<GameSnapshot>, AppError> {
    Ok(Json(state.snapshot(user.id).await?))
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdvanceRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(minimum = 1, maximum = 30)]
    days: u32,
}

/// A non-optional field whose JSON value itself may be null.
#[derive(Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = Option<AutoSpeed>)]
struct ClockSetting(Option<AutoSpeed>);

#[derive(ToSchema)]
struct ClockRequest {
    speed: ClockSetting,
}

impl<'de> Deserialize<'de> for ClockRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("게임 시계 요청은 객체여야 합니다"))?;
        let speed = object
            .get("speed")
            .ok_or_else(|| serde::de::Error::missing_field("speed"))?;
        let speed = serde_json::from_value(speed.clone()).map_err(serde::de::Error::custom)?;

        Ok(Self { speed })
    }
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum GameCommandFailureCode {
    InvalidCommand,
    IdempotencyConflict,
    Busy,
    ProgressBusy,
    CharacterRequired,
    ModeUnavailable,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct GameCommandFailure {
    code: GameCommandFailureCode,
    message: &'static str,
}

struct GameCommandError(GameLoopError);

impl From<GameLoopError> for GameCommandError {
    fn from(error: GameLoopError) -> Self {
        Self(error)
    }
}

impl axum::response::IntoResponse for GameCommandError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = match self.0 {
            GameLoopError::InvalidCommand => (
                StatusCode::BAD_REQUEST,
                GameCommandFailureCode::InvalidCommand,
                "명령 형식 또는 진행 일수가 올바르지 않습니다",
            ),
            GameLoopError::InvalidCharacter(errors) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ValidationFailure { errors }),
                )
                    .into_response();
            }
            GameLoopError::IdempotencyConflict => (
                StatusCode::CONFLICT,
                GameCommandFailureCode::IdempotencyConflict,
                "같은 명령 ID가 다른 요청에 이미 사용되었습니다",
            ),
            GameLoopError::Busy => (
                StatusCode::CONFLICT,
                GameCommandFailureCode::Busy,
                "게임 상태가 요청의 최초 커서와 다릅니다",
            ),
            GameLoopError::ProgressBusy => (
                StatusCode::CONFLICT,
                GameCommandFailureCode::ProgressBusy,
                "다른 진행 주체가 처리 중입니다. 같은 명령으로 다시 시도하세요",
            ),
            GameLoopError::CharacterRequired => (
                StatusCode::CONFLICT,
                GameCommandFailureCode::CharacterRequired,
                "먼저 캐릭터를 생성해야 합니다",
            ),
            GameLoopError::ModeUnavailable => (
                StatusCode::CONFLICT,
                GameCommandFailureCode::ModeUnavailable,
                "현재 게시된 ranked season에서 이 실행을 시작할 수 없습니다",
            ),
            GameLoopError::ActiveStreamRequired => (
                StatusCode::CONFLICT,
                GameCommandFailureCode::Busy,
                "배속 실행에는 활성 게임 연결이 필요합니다",
            ),
            GameLoopError::Internal(error) => return AppError::from(error).into_response(),
        };

        (status, Json(GameCommandFailure { code, message })).into_response()
    }
}

#[utoipa::path(
    post,
    path = "/api/advance",
    request_body = AdvanceRequest,
    responses(
        (status = 200, description = "수동 전진 명령 결과와 최신 스냅샷", body = AdvanceResponse),
        (status = 400, description = "명령 형식 또는 진행 일수가 잘못됨", body = GameCommandFailure),
        (status = 409, description = "명령 충돌, 오래된 커서 또는 캐릭터 없음", body = GameCommandFailure),
        (status = 500, description = "전진 실패"),
    )
)]
async fn advance(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<AdvanceRequest>, JsonRejection>,
) -> Result<Json<AdvanceResponse>, GameCommandError> {
    let Json(request) = request.map_err(|_| GameCommandError(GameLoopError::InvalidCommand))?;
    let command_id = CommandId::parse(request.command_id)
        .map_err(|_| GameCommandError(GameLoopError::InvalidCommand))?;
    let command = ManualAdvanceCommand {
        command_id,
        cursor: CommandCursor {
            expected_run_revision: request.expected_run_revision,
            expected_state_revision: request.expected_state_revision,
            expected_game_day: request.expected_game_day,
        },
        days: request.days,
    };

    Ok(Json(state.advance(user.id, &command).await?))
}

enum PortfolioOrderRouteError {
    Rejected(TradeFailure),
    Internal(AppError),
}

impl From<anyhow::Error> for PortfolioOrderRouteError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl From<TradeFailure> for PortfolioOrderRouteError {
    fn from(failure: TradeFailure) -> Self {
        Self::Rejected(failure)
    }
}

impl axum::response::IntoResponse for PortfolioOrderRouteError {
    fn into_response(self) -> axum::response::Response {
        let failure = match self {
            Self::Internal(error) => return error.into_response(),
            Self::Rejected(failure) => failure,
        };
        let status = if failure.code == TradeFailureCode::InvalidOrder {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::CONFLICT
        };

        (status, Json(failure)).into_response()
    }
}

#[utoipa::path(
    post,
    path = "/api/portfolio/orders",
    request_body = TradeOrderRequest,
    responses(
        (status = 200, description = "체결 또는 멱등 재조회 결과", body = PortfolioOrderResponse),
        (status = 400, description = "주문 형식이나 지원 상품이 잘못됨", body = TradeFailure),
        (status = 409, description = "현재 게임 상태에서 주문을 체결할 수 없음", body = TradeFailure),
        (status = 422, description = "JSON 요청 형태가 잘못됨"),
        (status = 500, description = "주문 저장 또는 스냅샷 조립 실패"),
    )
)]
async fn place_portfolio_order(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Json(request): Json<TradeOrderRequest>,
) -> Result<Json<PortfolioOrderResponse>, PortfolioOrderRouteError> {
    let order = TradeOrder::try_from(request)?;

    match state.place_order(user.id, &order).await? {
        PlaceOrderResult::Executed(response) => Ok(Json(*response)),
        PlaceOrderResult::Rejected(failure) => Err(failure.into()),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FinanceTransferRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    account_id: String,
    direction: TransferDirection,
    #[schema(minimum = 1)]
    amount_krw: i64,
}

impl TryFrom<FinanceTransferRequest> for TransferCommand {
    type Error = FinanceFailureCode;

    fn try_from(request: FinanceTransferRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            account_id: ResourceId::parse(&request.account_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            direction: request.direction,
            amount_krw: request.amount_krw,
        })
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct FinanceFailure {
    code: FinanceFailureCode,
    message: &'static str,
}

enum FinanceRouteError {
    Rejected(FinanceFailureCode),
    Internal(AppError),
}

impl From<anyhow::Error> for FinanceRouteError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl From<FinanceFailureCode> for FinanceRouteError {
    fn from(code: FinanceFailureCode) -> Self {
        Self::Rejected(code)
    }
}

impl axum::response::IntoResponse for FinanceRouteError {
    fn into_response(self) -> axum::response::Response {
        let code = match self {
            Self::Rejected(code) => code,
            Self::Internal(error) => return error.into_response(),
        };
        let status = if code == FinanceFailureCode::InvalidCommand {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::CONFLICT
        };

        (
            status,
            Json(FinanceFailure {
                code,
                message: finance_failure_message(code),
            }),
        )
            .into_response()
    }
}

const fn finance_failure_message(code: FinanceFailureCode) -> &'static str {
    match code {
        FinanceFailureCode::InvalidCommand => "금융 요청 형식이 올바르지 않습니다",
        FinanceFailureCode::CharacterRequired => "먼저 캐릭터를 생성해야 합니다",
        FinanceFailureCode::AccountNotFound => "계좌를 찾을 수 없습니다",
        FinanceFailureCode::AccountAlreadyExists => "같은 종류의 계좌가 이미 열려 있습니다",
        FinanceFailureCode::AccountClosed => "닫힌 계좌에서는 처리할 수 없습니다",
        FinanceFailureCode::AccountTypeNotAllowed => "이 계좌에서는 요청한 거래를 할 수 없습니다",
        FinanceFailureCode::AccountNotEmpty => "계좌 잔액을 먼저 비워야 합니다",
        FinanceFailureCode::InsufficientWalletCash => "지갑 현금이 부족합니다",
        FinanceFailureCode::InsufficientAccountCash => "계좌 현금이 부족합니다",
        FinanceFailureCode::PolicyNotEligible => "현재 제도 조건을 충족하지 않습니다",
        FinanceFailureCode::LimitExceeded => "허용 한도를 초과했습니다",
        FinanceFailureCode::ProductNotFound => "금융상품을 찾을 수 없습니다",
        FinanceFailureCode::ContractNotFound => "금융상품 계약을 찾을 수 없습니다",
        FinanceFailureCode::ContractClosed => "이미 종료된 금융상품 계약입니다",
        FinanceFailureCode::RateUnavailable => "현재 시장 금리로는 상품을 시작할 수 없습니다",
        FinanceFailureCode::MarketClosed => "휴장일에는 주문할 수 없습니다",
        FinanceFailureCode::InsufficientQuantity => "보유 수량이 부족합니다",
        FinanceFailureCode::PositionLimit => "상품 보유 한도를 초과했습니다",
        FinanceFailureCode::SettlementConflict => "이미 처리 중이거나 완료된 정산입니다",
        FinanceFailureCode::IdempotencyConflict => "같은 명령 ID가 다른 요청에 사용되었습니다",
        FinanceFailureCode::Busy => "게임 상태가 변경되었습니다. 최신 상태에서 다시 시도하세요",
    }
}

#[utoipa::path(
    get,
    path = "/api/finance/accounts",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 런의 금융계좌와 제도 버전", body = FinanceAccountsResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "계좌 조회 실패"),
    )
)]
async fn finance_accounts(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<FinanceAccountsResponse>, AppError> {
    Ok(Json(state.finance_accounts(user.id).await?))
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum CmaAccountOpenType {
    Cma,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum TaxAccountOpenType {
    IsaGeneral,
    IsaLowIncome,
    PensionSavings,
    Irp,
}

impl From<TaxAccountOpenType> for FinancialAccountType {
    fn from(account_type: TaxAccountOpenType) -> Self {
        match account_type {
            TaxAccountOpenType::IsaGeneral => Self::IsaGeneral,
            TaxAccountOpenType::IsaLowIncome => Self::IsaLowIncome,
            TaxAccountOpenType::PensionSavings => Self::PensionSavings,
            TaxAccountOpenType::Irp => Self::Irp,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum DepositKindRequest {
    TermDeposit,
    InstallmentSavings,
}

impl From<DepositKindRequest> for CashProductKind {
    fn from(kind: DepositKindRequest) -> Self {
        match kind {
            DepositKindRequest::TermDeposit => Self::TermDeposit,
            DepositKindRequest::InstallmentSavings => Self::InstallmentSavings,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FinanceCursorCommandRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
}

impl FinanceCursorCommandRequest {
    fn into_command(self) -> Result<(CommandId, CommandCursor), FinanceFailureCode> {
        Ok((
            CommandId::parse(self.command_id).map_err(|_| FinanceFailureCode::InvalidCommand)?,
            CommandCursor {
                expected_run_revision: self.expected_run_revision,
                expected_state_revision: self.expected_state_revision,
                expected_game_day: self.expected_game_day,
            },
        ))
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CmaAccountOpenRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[serde(rename = "type")]
    account_type: CmaAccountOpenType,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    product_version_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaxAccountOpenRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[serde(rename = "type")]
    account_type: TaxAccountOpenType,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GoldAccountOpenRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[serde(rename = "type")]
    account_type: M2dAccountType,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    product_version_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(untagged)]
enum FinanceAccountOpenRequest {
    Cma(CmaAccountOpenRequest),
    Gold(GoldAccountOpenRequest),
    Tax(TaxAccountOpenRequest),
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
enum FinanceAccountOpenResponse {
    Cma(CmaAccountOpenResponse),
    Gold(GoldAccountOpenResponse),
    Tax(TaxAccountOpenResponse),
}

impl TryFrom<CmaAccountOpenRequest> for OpenCmaAccountCommand {
    type Error = FinanceFailureCode;

    fn try_from(request: CmaAccountOpenRequest) -> Result<Self, Self::Error> {
        let CmaAccountOpenRequest {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            account_type: CmaAccountOpenType::Cma,
            product_version_id,
        } = request;
        Ok(Self {
            command_id: CommandId::parse(command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
            },
            product_version_id: ResourceId::parse(&product_version_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
        })
    }
}

impl TryFrom<TaxAccountOpenRequest> for OpenTaxAccountCommand {
    type Error = FinanceFailureCode;

    fn try_from(request: TaxAccountOpenRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            account_type: request.account_type.into(),
        })
    }
}

impl TryFrom<GoldAccountOpenRequest> for OpenGoldAccountCommand {
    type Error = FinanceFailureCode;

    fn try_from(request: GoldAccountOpenRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            account_type: request.account_type,
            product_version_id: ResourceId::parse(&request.product_version_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BondOrderRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    account_id: String,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    series_id: String,
    side: AssetOrderSide,
    #[schema(minimum = 1, maximum = 100000)]
    bond_units: u32,
}

impl TryFrom<BondOrderRequest> for BondOrderCommand {
    type Error = FinanceFailureCode;

    fn try_from(request: BondOrderRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            account_id: ResourceId::parse(&request.account_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            series_id: ResourceId::parse(&request.series_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            side: request.side,
            bond_units: request.bond_units,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GoldOrderRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    account_id: String,
    side: AssetOrderSide,
    #[schema(minimum = 1)]
    quantity_gram: u32,
}

impl TryFrom<GoldOrderRequest> for GoldOrderCommand {
    type Error = FinanceFailureCode;

    fn try_from(request: GoldOrderRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            account_id: ResourceId::parse(&request.account_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            side: request.side,
            quantity_gram: request.quantity_gram,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GoldWithdrawalRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    account_id: String,
    #[schema(minimum = 100, maximum = 1000)]
    bar_size_gram: u32,
    #[schema(minimum = 1)]
    bar_count: u32,
}

impl TryFrom<GoldWithdrawalRequest> for GoldWithdrawalCommand {
    type Error = FinanceFailureCode;

    fn try_from(request: GoldWithdrawalRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            account_id: ResourceId::parse(&request.account_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            bar_size_gram: request.bar_size_gram,
            bar_count: request.bar_count,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PensionStartRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(minimum = 5, maximum = 100)]
    payment_years: u16,
    lifetime: bool,
}

impl PensionStartRequest {
    fn into_command(
        self,
        account_id: ResourceId,
    ) -> Result<StartPensionCommand, FinanceFailureCode> {
        Ok(StartPensionCommand {
            command_id: CommandId::parse(self.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: self.expected_run_revision,
                expected_state_revision: self.expected_state_revision,
                expected_game_day: self.expected_game_day,
            },
            account_id,
            payment_years: self.payment_years,
            lifetime: self.lifetime,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PensionWithdrawalRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(minimum = 1)]
    amount_krw: i64,
    #[serde(rename = "type")]
    kind: PensionWithdrawalRequestKind,
    #[serde(deserialize_with = "deserialize_nullable_irp_withdrawal_reason")]
    #[schema(required = true, nullable)]
    reason: Option<IrpWithdrawalReason>,
}

fn deserialize_nullable_irp_withdrawal_reason<'de, D>(
    deserializer: D,
) -> Result<Option<IrpWithdrawalReason>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<IrpWithdrawalReason>::deserialize(deserializer)
}

impl PensionWithdrawalRequest {
    fn into_command(
        self,
        account_id: ResourceId,
    ) -> Result<PensionWithdrawalCommand, FinanceFailureCode> {
        Ok(PensionWithdrawalCommand {
            command_id: CommandId::parse(self.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: self.expected_run_revision,
                expected_state_revision: self.expected_state_revision,
                expected_game_day: self.expected_game_day,
            },
            account_id,
            amount_krw: self.amount_krw,
            kind: self.kind,
            reason: self.reason,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DepositOpenRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    kind: DepositKindRequest,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    product_version_id: String,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    settlement_account_id: String,
    #[schema(minimum = 1)]
    amount_krw: i64,
}

impl TryFrom<DepositOpenRequest> for OpenCashProductCommand {
    type Error = FinanceFailureCode;

    fn try_from(request: DepositOpenRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            kind: request.kind.into(),
            product_version_id: ResourceId::parse(&request.product_version_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            settlement_account_id: ResourceId::parse(&request.settlement_account_id)
                .map_err(|_| FinanceFailureCode::InvalidCommand)?,
            amount_krw: request.amount_krw,
        })
    }
}

#[utoipa::path(
    get,
    path = "/api/finance/bonds",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 런의 국채 상품과 유통 시리즈", body = BondCatalog),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "국채 카탈로그 조회 실패"),
    )
)]
async fn bond_catalog(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<BondCatalog>, AppError> {
    Ok(Json(state.bond_catalog(user.id).await?))
}

#[utoipa::path(
    post,
    path = "/api/finance/bonds/orders",
    request_body = BondOrderRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "국채 체결 또는 멱등 재조회", body = BondOrderResponse),
        (status = 400, description = "국채 주문 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 주문할 수 없음", body = FinanceFailure),
        (status = 500, description = "국채 주문 또는 스냅샷 조립 실패"),
    )
)]
async fn place_bond_order(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<BondOrderRequest>, JsonRejection>,
) -> Result<Json<BondOrderResponse>, FinanceRouteError> {
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let command = BondOrderCommand::try_from(request)?;
    match state.place_bond_order(user.id, &command).await? {
        AssetCommandResult::Applied(response) => Ok(Json(*response)),
        AssetCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/finance/gold-products",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 런의 KRX 금 상품", body = GoldCatalog),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "금 상품 조회 실패"),
    )
)]
async fn gold_product_catalog(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<GoldCatalog>, AppError> {
    Ok(Json(state.gold_catalog(user.id).await?))
}

#[utoipa::path(
    post,
    path = "/api/finance/gold/orders",
    request_body = GoldOrderRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "금 체결 또는 멱등 재조회", body = GoldOrderResponse),
        (status = 400, description = "금 주문 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 주문할 수 없음", body = FinanceFailure),
        (status = 500, description = "금 주문 또는 스냅샷 조립 실패"),
    )
)]
async fn place_gold_order(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<GoldOrderRequest>, JsonRejection>,
) -> Result<Json<GoldOrderResponse>, FinanceRouteError> {
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let command = GoldOrderCommand::try_from(request)?;
    match state.place_gold_order(user.id, &command).await? {
        AssetCommandResult::Applied(response) => Ok(Json(*response)),
        AssetCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/finance/gold/withdrawals",
    request_body = GoldWithdrawalRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "금 실물 인출 또는 멱등 재조회", body = GoldWithdrawalResponse),
        (status = 400, description = "금 인출 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 인출할 수 없음", body = FinanceFailure),
        (status = 500, description = "금 인출 또는 스냅샷 조립 실패"),
    )
)]
async fn withdraw_gold(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<GoldWithdrawalRequest>, JsonRejection>,
) -> Result<Json<GoldWithdrawalResponse>, FinanceRouteError> {
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let command = GoldWithdrawalCommand::try_from(request)?;
    match state.withdraw_gold(user.id, &command).await? {
        AssetCommandResult::Applied(response) => Ok(Json(*response)),
        AssetCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/finance/cash-products",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "게시된 CMA·예금·적금 상품", body = CashProductCatalogResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "현금상품 목록 조회 실패"),
    )
)]
async fn cash_product_catalog(
    State(state): State<Arc<AppState>>,
    AuthUser(_user): AuthUser,
) -> Result<Json<CashProductCatalogResponse>, AppError> {
    Ok(Json(state.cash_product_catalog().await?))
}

#[utoipa::path(
    post,
    path = "/api/finance/accounts",
    request_body = FinanceAccountOpenRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "금융계좌 개설 또는 멱등 재조회", body = FinanceAccountOpenResponse),
        (status = 400, description = "금융계좌 개설 요청 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 계좌를 열 수 없음", body = FinanceFailure),
        (status = 500, description = "금융계좌 개설 또는 스냅샷 조립 실패"),
    )
)]
async fn open_financial_account(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<FinanceAccountOpenRequest>, JsonRejection>,
) -> Result<Json<FinanceAccountOpenResponse>, FinanceRouteError> {
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    match request {
        FinanceAccountOpenRequest::Cma(request) => {
            let command = OpenCmaAccountCommand::try_from(request)?;
            match state.open_cma_account(user.id, &command).await? {
                CashProductCommandResult::Applied(response) => {
                    Ok(Json(FinanceAccountOpenResponse::Cma(*response)))
                }
                CashProductCommandResult::Rejected(code) => Err(code.into()),
            }
        }
        FinanceAccountOpenRequest::Gold(request) => {
            let command = OpenGoldAccountCommand::try_from(request)?;
            match state.open_gold_account(user.id, &command).await? {
                AssetCommandResult::Applied(response) => {
                    Ok(Json(FinanceAccountOpenResponse::Gold(*response)))
                }
                AssetCommandResult::Rejected(code) => Err(code.into()),
            }
        }
        FinanceAccountOpenRequest::Tax(request) => {
            let command = OpenTaxAccountCommand::try_from(request)?;
            match state.open_tax_account(user.id, &command).await? {
                TaxAccountCommandResult::Applied(response) => {
                    Ok(Json(FinanceAccountOpenResponse::Tax(*response)))
                }
                TaxAccountCommandResult::Rejected(code) => Err(code.into()),
            }
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/finance/accounts/{id}/close",
    params((
        "id" = String,
        Path,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$",
        description = "닫을 CMA 계좌 ID"
    )),
    request_body = FinanceCursorCommandRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "CMA 종료 또는 멱등 재조회", body = CmaAccountCloseResponse),
        (status = 400, description = "CMA 종료 요청 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 CMA를 닫을 수 없음", body = FinanceFailure),
        (status = 500, description = "CMA 종료 또는 스냅샷 조립 실패"),
    )
)]
async fn close_cma_account(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(account_id): Path<String>,
    request: Result<Json<FinanceCursorCommandRequest>, JsonRejection>,
) -> Result<Json<CmaAccountCloseResponse>, FinanceRouteError> {
    let account_id =
        ResourceId::parse(&account_id).map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let (command_id, cursor) = request.into_command()?;
    let command = CloseCmaAccountCommand {
        command_id,
        cursor,
        account_id,
    };
    match state.close_cma_account(user.id, &command).await? {
        CashProductCommandResult::Applied(response) => Ok(Json(*response)),
        CashProductCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/finance/isa/{id}/close",
    params((
        "id" = String,
        Path,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$",
        description = "닫을 ISA 계좌 ID"
    )),
    request_body = FinanceCursorCommandRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "ISA 해지 또는 멱등 재조회", body = IsaCloseResponse),
        (status = 400, description = "ISA 해지 요청 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 ISA를 해지할 수 없음", body = FinanceFailure),
        (status = 500, description = "ISA 해지 또는 스냅샷 조립 실패"),
    )
)]
async fn close_isa_account(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(account_id): Path<String>,
    request: Result<Json<FinanceCursorCommandRequest>, JsonRejection>,
) -> Result<Json<IsaCloseResponse>, FinanceRouteError> {
    let account_id =
        ResourceId::parse(&account_id).map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let (command_id, cursor) = request.into_command()?;
    let command = CloseIsaAccountCommand {
        command_id,
        cursor,
        account_id,
    };
    match state.close_isa_account(user.id, &command).await? {
        TaxAccountCommandResult::Applied(response) => Ok(Json(*response)),
        TaxAccountCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/finance/pensions/{id}/start",
    params((
        "id" = String,
        Path,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$",
        description = "연금 수령을 개시할 계좌 ID"
    )),
    request_body = PensionStartRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "연금 개시 또는 멱등 재조회", body = PensionStartResponse),
        (status = 400, description = "연금 개시 요청 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 연금을 개시할 수 없음", body = FinanceFailure),
        (status = 500, description = "연금 개시 또는 스냅샷 조립 실패"),
    )
)]
async fn start_pension(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(account_id): Path<String>,
    request: Result<Json<PensionStartRequest>, JsonRejection>,
) -> Result<Json<PensionStartResponse>, FinanceRouteError> {
    let account_id =
        ResourceId::parse(&account_id).map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let command = request.into_command(account_id)?;
    match state.start_pension(user.id, &command).await? {
        TaxAccountCommandResult::Applied(response) => Ok(Json(*response)),
        TaxAccountCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/finance/pensions/{id}/withdrawals",
    params((
        "id" = String,
        Path,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$",
        description = "인출할 연금계좌 ID"
    )),
    request_body = PensionWithdrawalRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "연금계좌 인출 또는 멱등 재조회", body = PensionWithdrawalResponse),
        (status = 400, description = "연금계좌 인출 요청 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 연금계좌에서 인출할 수 없음", body = FinanceFailure),
        (status = 500, description = "연금계좌 인출 또는 스냅샷 조립 실패"),
    )
)]
async fn withdraw_pension(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(account_id): Path<String>,
    request: Result<Json<PensionWithdrawalRequest>, JsonRejection>,
) -> Result<Json<PensionWithdrawalResponse>, FinanceRouteError> {
    let account_id =
        ResourceId::parse(&account_id).map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let command = request.into_command(account_id)?;
    match state.withdraw_pension(user.id, &command).await? {
        TaxAccountCommandResult::Applied(response) => Ok(Json(*response)),
        TaxAccountCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/finance/deposits",
    request_body = DepositOpenRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "예금·적금 가입 또는 멱등 재조회", body = DepositOpenResponse),
        (status = 400, description = "예금·적금 가입 요청 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 상품에 가입할 수 없음", body = FinanceFailure),
        (status = 500, description = "예금·적금 가입 또는 스냅샷 조립 실패"),
    )
)]
async fn open_deposit(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<DepositOpenRequest>, JsonRejection>,
) -> Result<Json<DepositOpenResponse>, FinanceRouteError> {
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let command = OpenCashProductCommand::try_from(request)?;
    match state.open_deposit(user.id, &command).await? {
        CashProductCommandResult::Applied(response) => Ok(Json(*response)),
        CashProductCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/finance/deposits/{id}/close",
    params((
        "id" = String,
        Path,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$",
        description = "중도해지할 계약 ID"
    )),
    request_body = FinanceCursorCommandRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "예금·적금 중도해지 또는 멱등 재조회", body = DepositCloseResponse),
        (status = 400, description = "중도해지 요청 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 계약을 해지할 수 없음", body = FinanceFailure),
        (status = 500, description = "중도해지 또는 스냅샷 조립 실패"),
    )
)]
async fn close_deposit(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(contract_id): Path<String>,
    request: Result<Json<FinanceCursorCommandRequest>, JsonRejection>,
) -> Result<Json<DepositCloseResponse>, FinanceRouteError> {
    let contract_id =
        ResourceId::parse(&contract_id).map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let (command_id, cursor) = request.into_command()?;
    let command = CloseCashProductCommand {
        command_id,
        cursor,
        contract_id,
    };
    match state.close_deposit(user.id, &command).await? {
        CashProductCommandResult::Applied(response) => Ok(Json(*response)),
        CashProductCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/finance/tax-years/{year}",
    params(("year" = u16, Path, minimum = 1, maximum = 9999, description = "조회할 달력 연도")),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "금융소득과 원천징수 누계", body = FinancialIncomeYearSnapshot),
        (status = 400, description = "연도 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "금융소득 연도 조회 실패"),
    )
)]
async fn finance_tax_year(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(year): Path<String>,
) -> Result<Json<FinancialIncomeYearSnapshot>, FinanceRouteError> {
    let year = year
        .parse::<u16>()
        .ok()
        .filter(|year| *year > 0)
        .ok_or(FinanceFailureCode::InvalidCommand)?;
    Ok(Json(state.finance_tax_year(user.id, year).await?))
}

#[utoipa::path(
    post,
    path = "/api/finance/transfers",
    request_body = FinanceTransferRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "이체 또는 멱등 재조회 결과", body = FinanceTransferResponse),
        (status = 400, description = "이체 요청 형식이 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 이체할 수 없음", body = FinanceFailure),
        (status = 500, description = "이체 저장 또는 스냅샷 조립 실패"),
    )
)]
async fn finance_transfer(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<FinanceTransferRequest>, JsonRejection>,
) -> Result<Json<FinanceTransferResponse>, FinanceRouteError> {
    let Json(request) = request.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let command = TransferCommand::try_from(request)?;

    match state.transfer_finance(user.id, &command).await? {
        FinanceCommandResult::Transferred(response) => Ok(Json(*response)),
        FinanceCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
struct FinanceLedgerQuery {
    #[param(
        value_type = String,
        required = false,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    before: Option<String>,
    #[param(
        value_type = u32,
        required = false,
        default = 50,
        minimum = 1,
        maximum = 200
    )]
    limit: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/finance/ledger",
    params(FinanceLedgerQuery),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 런의 최신순 원장 페이지", body = LedgerPageResponse),
        (status = 400, description = "페이지 커서나 크기가 잘못됨", body = FinanceFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "원장 조회 실패"),
    )
)]
async fn finance_ledger(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<FinanceLedgerQuery>, QueryRejection>,
) -> Result<Json<LedgerPageResponse>, FinanceRouteError> {
    let Query(query) = query.map_err(|_| FinanceFailureCode::InvalidCommand)?;
    let before = query
        .before
        .as_deref()
        .map(ResourceId::parse)
        .transpose()
        .map_err(|_| FinanceFailureCode::InvalidCommand)?
        .map(ResourceId::get);
    let limit = query
        .limit
        .as_deref()
        .map(str::parse::<u32>)
        .transpose()
        .map_err(|_| FinanceFailureCode::InvalidCommand)?
        .unwrap_or(DEFAULT_LEDGER_PAGE_SIZE);
    if !(1..=MAX_LEDGER_PAGE_SIZE).contains(&limit) {
        return Err(FinanceFailureCode::InvalidCommand.into());
    }

    Ok(Json(state.finance_ledger(user.id, before, limit).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoanQuoteRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    product_version_id: String,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    principal_krw: i64,
}

impl TryFrom<LoanQuoteRequest> for CreateLoanQuoteCommand {
    type Error = LifeFailureCode;

    fn try_from(request: LoanQuoteRequest) -> Result<Self, Self::Error> {
        if request.principal_krw <= 0 || request.principal_krw > MAX_JSON_SAFE_INTEGER as i64 {
            return Err(LifeFailureCode::InvalidCommand);
        }
        let (command_id, cursor) = life_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        let product_version_id = ResourceId::parse(&request.product_version_id)
            .map_err(|_| LifeFailureCode::InvalidCommand)?;

        Ok(Self {
            command_id,
            cursor,
            product_version_id,
            principal_krw: request.principal_krw,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoanExecutionRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    quote_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoanPrepaymentRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    principal_krw: i64,
}

fn prepay_loan_command(
    loan_id: &str,
    request: LoanPrepaymentRequest,
) -> Result<PrepayLoanCommand, LifeFailureCode> {
    if request.principal_krw <= 0 || request.principal_krw > MAX_JSON_SAFE_INTEGER as i64 {
        return Err(LifeFailureCode::InvalidCommand);
    }
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let loan_id = ResourceId::parse(loan_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    Ok(PrepayLoanCommand {
        command_id,
        cursor,
        loan_id,
        principal_krw: request.principal_krw,
    })
}

impl TryFrom<LoanExecutionRequest> for ExecuteLoanCommand {
    type Error = LifeFailureCode;

    fn try_from(request: LoanExecutionRequest) -> Result<Self, Self::Error> {
        let (command_id, cursor) = life_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        let quote_id =
            ResourceId::parse(&request.quote_id).map_err(|_| LifeFailureCode::InvalidCommand)?;

        Ok(Self {
            command_id,
            cursor,
            quote_id,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum LivingCostCategoryRequest {
    Housing,
    Food,
    Transport,
    Communication,
    Utilities,
    Healthcare,
    Education,
    DependentCare,
    Discretionary,
}

impl From<LivingCostCategoryRequest> for LivingCostCategory {
    fn from(category: LivingCostCategoryRequest) -> Self {
        match category {
            LivingCostCategoryRequest::Housing => Self::Housing,
            LivingCostCategoryRequest::Food => Self::Food,
            LivingCostCategoryRequest::Transport => Self::Transport,
            LivingCostCategoryRequest::Communication => Self::Communication,
            LivingCostCategoryRequest::Utilities => Self::Utilities,
            LivingCostCategoryRequest::Healthcare => Self::Healthcare,
            LivingCostCategoryRequest::Education => Self::Education,
            LivingCostCategoryRequest::DependentCare => Self::DependentCare,
            LivingCostCategoryRequest::Discretionary => Self::Discretionary,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LifeBudgetSelectionRequest {
    category: LivingCostCategoryRequest,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    band_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LifeBudgetUpdateRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_items = 9, max_items = 9)]
    selections: Vec<LifeBudgetSelectionRequest>,
}

impl TryFrom<LifeBudgetUpdateRequest> for UpdateLifeBudgetCommand {
    type Error = LifeFailureCode;

    fn try_from(request: LifeBudgetUpdateRequest) -> Result<Self, Self::Error> {
        if request.selections.len() != LivingCostCategory::ALL.len()
            || request
                .selections
                .iter()
                .map(|selection| selection.category)
                .collect::<HashSet<_>>()
                .len()
                != LivingCostCategory::ALL.len()
        {
            return Err(LifeFailureCode::InvalidCommand);
        }
        let (command_id, cursor) = life_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        let mut selections = request
            .selections
            .into_iter()
            .map(|selection| {
                Ok(LifeBudgetSelectionState {
                    category: selection.category.into(),
                    band_id: ResourceId::parse(&selection.band_id)
                        .map_err(|_| LifeFailureCode::InvalidCommand)?,
                })
            })
            .collect::<Result<Vec<_>, LifeFailureCode>>()?;
        selections.sort_by_key(|selection| selection.category.order());

        Ok(Self {
            command_id,
            cursor,
            selections,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EssentialArrearPaymentRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    amount_krw: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LeaseArrearPaymentRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    amount_krw: i64,
}

fn life_command_parts(
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
) -> Result<(CommandId, CommandCursor), LifeFailureCode> {
    if expected_state_revision > MAX_JSON_SAFE_INTEGER {
        return Err(LifeFailureCode::InvalidCommand);
    }
    Ok((
        CommandId::parse(command_id).map_err(|_| LifeFailureCode::InvalidCommand)?,
        CommandCursor {
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
        },
    ))
}

fn essential_arrear_payment_command(
    arrear_id: ResourceId,
    request: EssentialArrearPaymentRequest,
) -> Result<PayEssentialArrearCommand, LifeFailureCode> {
    if request.amount_krw <= 0 || request.amount_krw > MAX_JSON_SAFE_INTEGER as i64 {
        return Err(LifeFailureCode::InvalidCommand);
    }
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    Ok(PayEssentialArrearCommand {
        command_id,
        cursor,
        arrear_id,
        amount_krw: request.amount_krw,
    })
}

fn lease_arrear_payment_command(
    arrear_id: ResourceId,
    request: LeaseArrearPaymentRequest,
) -> Result<PayLeaseArrearCommand, LifeFailureCode> {
    if request.amount_krw <= 0 || request.amount_krw > MAX_JSON_SAFE_INTEGER as i64 {
        return Err(LifeFailureCode::InvalidCommand);
    }
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    Ok(PayLeaseArrearCommand {
        command_id,
        cursor,
        arrear_id,
        amount_krw: request.amount_krw,
    })
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct LifeFailure {
    code: LifeFailureCodeSnapshot,
    message: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum LifeFailureCodeSnapshot {
    InvalidCommand,
    CharacterRequired,
    InsufficientWalletCash,
    RateUnavailable,
    CreditRestricted,
    IncomeUnavailable,
    DebtServiceLimit,
    CollateralLimit,
    AffordabilityLimit,
    ContractConflict,
    IdempotencyConflict,
    SettlementConflict,
    HousingResourceNotFound,
    WelfareResourceNotFound,
    EventNotFound,
    EventExpired,
    InsuranceResourceNotFound,
    InsolvencyResourceNotFound,
    InsolvencyCompositionUnsupported,
    InsolvencyCompositionChanged,
    InsolvencyStateConflict,
    CorporationResourceNotFound,
    CorporationStateConflict,
    ClaimNotCovered,
    Ineligible,
    ValuationUnavailable,
    PolicyUnsupported,
    Busy,
}

impl From<LifeFailureCode> for LifeFailureCodeSnapshot {
    fn from(code: LifeFailureCode) -> Self {
        match code {
            LifeFailureCode::InvalidCommand => Self::InvalidCommand,
            LifeFailureCode::CharacterRequired => Self::CharacterRequired,
            LifeFailureCode::InsufficientWalletCash => Self::InsufficientWalletCash,
            LifeFailureCode::RateUnavailable => Self::RateUnavailable,
            LifeFailureCode::CreditRestricted => Self::CreditRestricted,
            LifeFailureCode::IncomeUnavailable => Self::IncomeUnavailable,
            LifeFailureCode::DebtServiceLimit => Self::DebtServiceLimit,
            LifeFailureCode::CollateralLimit => Self::CollateralLimit,
            LifeFailureCode::AffordabilityLimit => Self::AffordabilityLimit,
            LifeFailureCode::ContractConflict => Self::ContractConflict,
            LifeFailureCode::IdempotencyConflict => Self::IdempotencyConflict,
            LifeFailureCode::SettlementConflict => Self::SettlementConflict,
            LifeFailureCode::HousingResourceNotFound => Self::HousingResourceNotFound,
            LifeFailureCode::WelfareResourceNotFound => Self::WelfareResourceNotFound,
            LifeFailureCode::EventNotFound => Self::EventNotFound,
            LifeFailureCode::EventExpired => Self::EventExpired,
            LifeFailureCode::InsuranceResourceNotFound => Self::InsuranceResourceNotFound,
            LifeFailureCode::InsolvencyResourceNotFound => Self::InsolvencyResourceNotFound,
            LifeFailureCode::InsolvencyCompositionUnsupported => {
                Self::InsolvencyCompositionUnsupported
            }
            LifeFailureCode::InsolvencyCompositionChanged => Self::InsolvencyCompositionChanged,
            LifeFailureCode::InsolvencyStateConflict => Self::InsolvencyStateConflict,
            LifeFailureCode::CorporationResourceNotFound => Self::CorporationResourceNotFound,
            LifeFailureCode::CorporationStateConflict => Self::CorporationStateConflict,
            LifeFailureCode::ClaimNotCovered => Self::ClaimNotCovered,
            LifeFailureCode::Ineligible => Self::Ineligible,
            LifeFailureCode::ValuationUnavailable => Self::ValuationUnavailable,
            LifeFailureCode::PolicyUnsupported => Self::PolicyUnsupported,
            LifeFailureCode::Busy => Self::Busy,
        }
    }
}

enum LifeRouteError {
    Rejected(LifeFailureCode),
    Internal(AppError),
}

impl From<LifeFailureCode> for LifeRouteError {
    fn from(code: LifeFailureCode) -> Self {
        Self::Rejected(code)
    }
}

impl From<anyhow::Error> for LifeRouteError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for LifeRouteError {
    fn into_response(self) -> axum::response::Response {
        let code = match self {
            Self::Rejected(code) => code,
            Self::Internal(error) => return error.into_response(),
        };
        let status = match code {
            LifeFailureCode::InvalidCommand => StatusCode::BAD_REQUEST,
            LifeFailureCode::HousingResourceNotFound
            | LifeFailureCode::WelfareResourceNotFound
            | LifeFailureCode::EventNotFound
            | LifeFailureCode::InsuranceResourceNotFound
            | LifeFailureCode::InsolvencyResourceNotFound
            | LifeFailureCode::CorporationResourceNotFound => StatusCode::NOT_FOUND,
            LifeFailureCode::InsolvencyCompositionUnsupported => StatusCode::UNPROCESSABLE_ENTITY,
            _ => StatusCode::CONFLICT,
        };
        (
            status,
            Json(LifeFailure {
                code: code.into(),
                message: life_failure_message(code),
            }),
        )
            .into_response()
    }
}

const fn life_failure_message(code: LifeFailureCode) -> &'static str {
    match code {
        LifeFailureCode::InvalidCommand => "요청 형식이 올바르지 않습니다",
        LifeFailureCode::CharacterRequired => "먼저 캐릭터를 생성해야 합니다",
        LifeFailureCode::InsufficientWalletCash => "지갑 현금이 부족합니다",
        LifeFailureCode::RateUnavailable => "현재 월드에는 필요한 생활·대출·주거 규칙이 없습니다",
        LifeFailureCode::CreditRestricted => "현재 신용 상태로는 대출을 실행할 수 없습니다",
        LifeFailureCode::IncomeUnavailable => "검증된 소득이 없어 대출을 실행할 수 없습니다",
        LifeFailureCode::DebtServiceLimit => "총부채원리금상환비율 한도를 초과했습니다",
        LifeFailureCode::CollateralLimit => "요청한 대출 원금이 보증금 한도를 초과했습니다",
        LifeFailureCode::AffordabilityLimit => "전세자금대출 상환여력 한도를 초과했습니다",
        LifeFailureCode::ContractConflict => "현재 상태에서 이 계약 요청을 처리할 수 없습니다",
        LifeFailureCode::IdempotencyConflict => "같은 명령 ID가 다른 요청에 사용되었습니다",
        LifeFailureCode::SettlementConflict => "이미 처리 중이거나 완료된 생활비 정산입니다",
        LifeFailureCode::HousingResourceNotFound => {
            "현재 run의 주택 또는 매도 주문을 찾을 수 없습니다"
        }
        LifeFailureCode::WelfareResourceNotFound => {
            "현재 run에서 사용할 수 있는 복지 프로그램을 찾을 수 없습니다"
        }
        LifeFailureCode::EventNotFound => "현재 run의 생애 사건을 찾을 수 없습니다",
        LifeFailureCode::EventExpired => "생애 사건의 선택 기한이 지났거나 이미 해결되었습니다",
        LifeFailureCode::InsuranceResourceNotFound => {
            "현재 run의 보험 상품, 계약 또는 청구를 찾을 수 없습니다"
        }
        LifeFailureCode::InsolvencyResourceNotFound => "현재 run의 도산 사건을 찾을 수 없습니다",
        LifeFailureCode::InsolvencyCompositionUnsupported => {
            "현재 자산과 채무 구성은 현금 전용 청산 절차가 지원하지 않습니다"
        }
        LifeFailureCode::InsolvencyCompositionChanged => {
            "준비 후 자산 또는 채무 구성이 변경되었습니다. 사건을 다시 준비하세요"
        }
        LifeFailureCode::InsolvencyStateConflict => {
            "현재 cursor 또는 도산 사건 상태에서 이 요청을 처리할 수 없습니다"
        }
        LifeFailureCode::CorporationResourceNotFound => {
            "현재 run의 법인 또는 업종 템플릿을 찾을 수 없습니다"
        }
        LifeFailureCode::CorporationStateConflict => {
            "현재 cursor, 법인 또는 도산 상태에서 이 요청을 처리할 수 없습니다"
        }
        LifeFailureCode::ClaimNotCovered => {
            "이 청구는 보장되지 않거나 지급 기한 또는 상태가 유효하지 않습니다"
        }
        LifeFailureCode::Ineligible => "현재 조건으로는 이 복지 프로그램을 신청할 수 없습니다",
        LifeFailureCode::ValuationUnavailable => {
            "현재 자산 평가 정보로는 복지 자격을 판정할 수 없습니다"
        }
        LifeFailureCode::PolicyUnsupported => "현재 부동산 세금 정책이 이 상태를 지원하지 않습니다",
        LifeFailureCode::Busy => "게임 상태가 변경되었습니다. 최신 상태에서 다시 시도하세요",
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
struct LifeEventsQuery {
    #[param(required = false, max_length = 512)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LifeEventChoiceRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    choice_id: String,
}

fn life_event_choice_command(
    event_id: String,
    request: LifeEventChoiceRequest,
) -> Result<ResolveLifeEventCommand, LifeFailureCode> {
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let event_id = ResourceId::parse(&event_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let choice_id =
        ResourceId::parse(&request.choice_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    Ok(ResolveLifeEventCommand {
        command_id,
        cursor,
        event_id,
        choice_id,
    })
}

#[utoipa::path(
    get,
    path = "/api/life/events",
    params(LifeEventsQuery),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run의 생애 사건 기능, 선택 대기 사건과 최근 기록", body = LifeEventsResponse),
        (status = 400, description = "생애 사건 cursor 또는 query 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터와 현재 run이 필요함", body = LifeFailure),
        (status = 500, description = "생애 사건 조회 또는 invariant 검증 실패"),
    )
)]
async fn life_events(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<LifeEventsQuery>, QueryRejection>,
) -> Result<Json<LifeEventsResponse>, LifeRouteError> {
    let Query(query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    if query
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.is_empty() || cursor.len() > 512 || !cursor.is_ascii())
    {
        return Err(LifeFailureCode::InvalidCommand.into());
    }
    match state
        .life_events(
            user.id,
            LifeEventsQueryState {
                cursor: query.cursor,
            },
        )
        .await?
    {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/life/events/{eventId}/choices",
    params(
        ("eventId" = String, Path, description = "현재 run 생애 사건 ID", pattern = "^[1-9][0-9]*$")
    ),
    request_body = LifeEventChoiceRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "생애 사건 선택 결과 또는 멱등 재조회", body = LifeEventChoiceResponse),
        (status = 400, description = "사건 ID, 선택 또는 cursor 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 사건을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "기한·선택·현금·cursor 또는 계약 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "생애 사건 선택 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn resolve_life_event(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(event_id): Path<String>,
    request: Result<Json<LifeEventChoiceRequest>, JsonRejection>,
) -> Result<Json<LifeEventChoiceResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = life_event_choice_command(event_id, request)?;
    match state.resolve_life_event(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
struct InsuranceContractsQuery {
    #[param(required = false, max_length = 512)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InsuranceEnrollmentRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    product_version_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InsuranceCancellationRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InsuranceClaimRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    claim_id: String,
}

#[utoipa::path(
    get,
    path = "/api/insurance/contracts",
    params(InsuranceContractsQuery),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run의 보험 상품, 계약, pending claim과 최근 결과", body = InsuranceContractsResponse),
        (status = 400, description = "보험 cursor 또는 query 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터와 현재 run 또는 지원 보험 component가 필요함", body = LifeFailure),
        (status = 500, description = "보험 조회 또는 invariant 검증 실패"),
    )
)]
async fn insurance_contracts(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<InsuranceContractsQuery>, QueryRejection>,
) -> Result<Json<InsuranceContractsResponse>, LifeRouteError> {
    let Query(query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    if query
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.is_empty() || cursor.len() > 512 || !cursor.is_ascii())
    {
        return Err(LifeFailureCode::InvalidCommand.into());
    }
    match state
        .insurance(
            user.id,
            InsuranceQueryState {
                cursor: query.cursor,
            },
        )
        .await?
    {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/insurance/contracts",
    request_body = InsuranceEnrollmentRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "보험 가입과 초회 보험료 결제 또는 멱등 재조회", body = InsuranceEnrollmentResponse),
        (status = 400, description = "보험 가입 body 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 보험 상품을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "자격·잔액·cursor 또는 계약 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "보험 가입 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn enroll_insurance_contract(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<InsuranceEnrollmentRequest>, JsonRejection>,
) -> Result<Json<InsuranceEnrollmentResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let product_version_id = ResourceId::parse(&request.product_version_id)
        .map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = EnrollInsuranceContractCommand {
        command_id,
        cursor,
        product_version_id,
    };
    match state.enroll_insurance_contract(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/insurance/contracts/{contractId}/cancellations",
    params(
        ("contractId" = String, Path, description = "현재 run 보험 계약 ID", pattern = "^[1-9][0-9]*$")
    ),
    request_body = InsuranceCancellationRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "active 보험 계약 중도 취소 또는 멱등 재조회", body = InsuranceCancellationResponse),
        (status = 400, description = "계약 ID 또는 취소 body 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 보험 계약을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "cursor 또는 계약 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "보험 취소 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn cancel_insurance_contract(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(contract_id): Path<String>,
    request: Result<Json<InsuranceCancellationRequest>, JsonRejection>,
) -> Result<Json<InsuranceCancellationResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let contract_id =
        ResourceId::parse(&contract_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = CancelInsuranceContractCommand {
        command_id,
        cursor,
        contract_id,
    };
    match state.cancel_insurance_contract(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/insurance/claims",
    request_body = InsuranceClaimRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "ready 보험 claim 지급 또는 멱등 재조회", body = InsuranceClaimResponse),
        (status = 400, description = "claim body 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 claim을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "보장·기한·cursor 또는 claim 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "claim 지급 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn file_insurance_claim(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<InsuranceClaimRequest>, JsonRejection>,
) -> Result<Json<InsuranceClaimResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let claim_id =
        ResourceId::parse(&request.claim_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = FileInsuranceClaimCommand {
        command_id,
        cursor,
        claim_id,
    };
    match state.file_insurance_claim(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorporationCreateRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    industry_template_id: String,
    #[schema(min_length = 2, max_length = 40)]
    name: String,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    capital_krw: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum CorporationOperationRequest {
    AcceptContract {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
        contract_id: String,
    },
    CancelContract {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
        contract_id: String,
    },
    HirePosition {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
        position_id: String,
    },
    TerminatePosition {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
        position_id: String,
    },
    SetMonthlyPlan {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
        marketing_band_id: String,
        cash_buffer_krw: i64,
        #[schema(max_items = 50, value_type = Vec<String>)]
        contract_priority_ids: Vec<String>,
    },
    CapitalContribution {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
        amount_krw: i64,
    },
    DrawWorkingCapitalLoan {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
        loan_product_id: String,
        principal_krw: i64,
    },
    RepayWorkingCapitalLoan {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
        loan_id: String,
        principal_krw: i64,
    },
    Dissolve {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        expected_revision: u64,
    },
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorporationSettingsRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    operating_scale_id: String,
    #[schema(minimum = 0, maximum = 100000000)]
    officer_gross_salary_krw: i64,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum CorporationPayoutKindRequest {
    Dividend,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorporationPayoutRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    kind: CorporationPayoutKindRequest,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    gross_dividend_krw: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorporationMonthsQuery {
    cursor: Option<String>,
}

fn corporation_create_command(
    request: CorporationCreateRequest,
) -> Result<CreateCorporationCommand, LifeFailureCode> {
    if request.capital_krw <= 0 || request.capital_krw > MAX_JSON_SAFE_INTEGER as i64 {
        return Err(LifeFailureCode::InvalidCommand);
    }
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    Ok(CreateCorporationCommand {
        command_id,
        cursor,
        industry_template_id: ResourceId::parse(&request.industry_template_id)
            .map_err(|_| LifeFailureCode::InvalidCommand)?,
        name: request.name,
        capital_krw: request.capital_krw,
    })
}

fn corporation_operation_command(
    corporation_id: ResourceId,
    request: CorporationOperationRequest,
) -> Result<ManageBusinessOperationsCommand, LifeFailureCode> {
    let (
        command_id,
        expected_run_revision,
        expected_state_revision,
        expected_game_day,
        revision,
        action,
    ) = match request {
        CorporationOperationRequest::AcceptContract {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            contract_id,
        } => (
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            BusinessOperationAction::AcceptContract {
                contract_id: ResourceId::parse(&contract_id)
                    .map_err(|_| LifeFailureCode::InvalidCommand)?,
            },
        ),
        CorporationOperationRequest::CancelContract {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            contract_id,
        } => (
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            BusinessOperationAction::CancelContract {
                contract_id: ResourceId::parse(&contract_id)
                    .map_err(|_| LifeFailureCode::InvalidCommand)?,
            },
        ),
        CorporationOperationRequest::HirePosition {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            position_id,
        } => (
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            BusinessOperationAction::HirePosition {
                position_id: ResourceId::parse(&position_id)
                    .map_err(|_| LifeFailureCode::InvalidCommand)?,
            },
        ),
        CorporationOperationRequest::TerminatePosition {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            position_id,
        } => (
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            BusinessOperationAction::TerminatePosition {
                position_id: ResourceId::parse(&position_id)
                    .map_err(|_| LifeFailureCode::InvalidCommand)?,
            },
        ),
        CorporationOperationRequest::SetMonthlyPlan {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            marketing_band_id,
            cash_buffer_krw,
            contract_priority_ids,
        } => {
            if !(0..=MAX_JSON_SAFE_INTEGER as i64).contains(&cash_buffer_krw)
                || contract_priority_ids.len() > 50
            {
                return Err(LifeFailureCode::InvalidCommand);
            }
            let contract_priority_ids = contract_priority_ids
                .into_iter()
                .map(|id| ResourceId::parse(&id).map_err(|_| LifeFailureCode::InvalidCommand))
                .collect::<Result<Vec<_>, _>>()?;
            (
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                expected_revision,
                BusinessOperationAction::SetMonthlyPlan {
                    marketing_band_id: ResourceId::parse(&marketing_band_id)
                        .map_err(|_| LifeFailureCode::InvalidCommand)?,
                    cash_buffer_krw,
                    contract_priority_ids,
                },
            )
        }
        CorporationOperationRequest::CapitalContribution {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            amount_krw,
        } => {
            if !(1..=MAX_JSON_SAFE_INTEGER as i64).contains(&amount_krw) {
                return Err(LifeFailureCode::InvalidCommand);
            }
            (
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                expected_revision,
                BusinessOperationAction::CapitalContribution { amount_krw },
            )
        }
        CorporationOperationRequest::DrawWorkingCapitalLoan {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            loan_product_id,
            principal_krw,
        } => {
            if !(1..=MAX_JSON_SAFE_INTEGER as i64).contains(&principal_krw) {
                return Err(LifeFailureCode::InvalidCommand);
            }
            (
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                expected_revision,
                BusinessOperationAction::DrawWorkingCapitalLoan {
                    loan_product_id: ResourceId::parse(&loan_product_id)
                        .map_err(|_| LifeFailureCode::InvalidCommand)?,
                    principal_krw,
                },
            )
        }
        CorporationOperationRequest::RepayWorkingCapitalLoan {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            loan_id,
            principal_krw,
        } => {
            if !(1..=MAX_JSON_SAFE_INTEGER as i64).contains(&principal_krw) {
                return Err(LifeFailureCode::InvalidCommand);
            }
            (
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                expected_revision,
                BusinessOperationAction::RepayWorkingCapitalLoan {
                    loan_id: ResourceId::parse(&loan_id)
                        .map_err(|_| LifeFailureCode::InvalidCommand)?,
                    principal_krw,
                },
            )
        }
        CorporationOperationRequest::Dissolve {
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
        } => (
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            expected_revision,
            BusinessOperationAction::Dissolve,
        ),
    };
    if revision == 0 || revision > MAX_JSON_SAFE_INTEGER {
        return Err(LifeFailureCode::InvalidCommand);
    }
    let (command_id, cursor) = life_command_parts(
        command_id,
        expected_run_revision,
        expected_state_revision,
        expected_game_day,
    )?;
    Ok(ManageBusinessOperationsCommand {
        command_id,
        cursor,
        corporation_id,
        expected_revision: revision,
        action,
    })
}

#[utoipa::path(
    get,
    path = "/api/corporations/templates",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run에 고정된 법인 설립 조건과 업종 템플릿", body = CorporationTemplatesResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터 또는 현재 run이 필요함", body = LifeFailure),
        (status = 500, description = "법인 템플릿 조회 또는 invariant 검증 실패"),
    )
)]
async fn corporation_templates(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<CorporationTemplatesResponse>, LifeRouteError> {
    match state.corporation_templates(user.id).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/corporations",
    request_body = CorporationCreateRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "법인 설립 결과 또는 멱등 재조회", body = CorporationCreateResponse),
        (status = 400, description = "법인 설립 body 형식 또는 값이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 업종 템플릿을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "현금, cursor, 기존 법인 또는 도산 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "법인 설립 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn create_corporation(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<CorporationCreateRequest>, JsonRejection>,
) -> Result<Json<CorporationCreateResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = corporation_create_command(request)?;
    match state.create_corporation(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/corporations/{corporationId}",
    params(("corporationId" = String, Path, description = "현재 run 법인 ID", pattern = "^[1-9][0-9]*$")),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run 법인 상세", body = CorporationDetailResponse),
        (status = 400, description = "법인 ID 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 법인을 찾을 수 없음", body = LifeFailure),
        (status = 500, description = "법인 상세 조회 또는 invariant 검증 실패"),
    )
)]
async fn corporation_detail(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(corporation_id): Path<String>,
) -> Result<Json<CorporationDetailResponse>, LifeRouteError> {
    let corporation_id =
        ResourceId::parse(&corporation_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    match state.corporation_detail(user.id, corporation_id).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/corporations/{corporationId}/operations",
    params(("corporationId" = String, Path, description = "현재 run 법인 ID", pattern = "^[1-9][0-9]*$")),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "고정된 카탈로그와 다음 달 계약·인력·운영 계획", body = BusinessOperationsResponse),
        (status = 400, description = "법인 ID 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 법인을 찾을 수 없음", body = LifeFailure),
        (status = 500, description = "법인 운영 상태 조회 또는 invariant 검증 실패"),
    )
)]
async fn corporation_operations(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(corporation_id): Path<String>,
) -> Result<Json<BusinessOperationsResponse>, LifeRouteError> {
    let corporation_id =
        ResourceId::parse(&corporation_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    match state
        .corporation_operations(user.id, corporation_id)
        .await?
    {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/corporations/{corporationId}/operations",
    params(("corporationId" = String, Path, description = "현재 run 법인 ID", pattern = "^[1-9][0-9]*$")),
    request_body = CorporationOperationRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "계약·인력·월 계획 명령 결과 또는 멱등 재조회", body = BusinessOperationResponse),
        (status = 400, description = "명령 tag·필드·식별자 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 법인을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "cursor·revision·멱등성 또는 대상 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "법인 운영 명령 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn manage_corporation_operations(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(corporation_id): Path<String>,
    request: Result<Json<CorporationOperationRequest>, JsonRejection>,
) -> Result<Json<BusinessOperationResponse>, LifeRouteError> {
    let corporation_id =
        ResourceId::parse(&corporation_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = corporation_operation_command(corporation_id, request)?;
    match state
        .manage_corporation_operations(user.id, &command)
        .await?
    {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    put,
    path = "/api/corporations/{corporationId}/settings",
    params(("corporationId" = String, Path, description = "현재 run 법인 ID", pattern = "^[1-9][0-9]*$")),
    request_body = CorporationSettingsRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "다음 operating month부터 적용할 운영 설정", body = CorporationSettingsResponse),
        (status = 400, description = "명령 형식 또는 설정 범위가 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "법인 또는 운영 규모를 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "cursor·멱등성 또는 법인 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "법인 설정 저장 또는 스냅샷 조립 실패"),
    )
)]
async fn update_corporation_settings(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(corporation_id): Path<String>,
    request: Result<Json<CorporationSettingsRequest>, JsonRejection>,
) -> Result<Json<CorporationSettingsResponse>, LifeRouteError> {
    let corporation_id =
        ResourceId::parse(&corporation_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    if !(0..=100_000_000).contains(&request.officer_gross_salary_krw) {
        return Err(LifeFailureCode::InvalidCommand.into());
    }
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let command = UpdateCorporationSettingsCommand {
        command_id,
        cursor,
        corporation_id,
        operating_scale_id: ResourceId::parse(&request.operating_scale_id)
            .map_err(|_| LifeFailureCode::InvalidCommand)?,
        officer_gross_salary_krw: request.officer_gross_salary_krw,
    };
    match state.update_corporation_settings(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/corporations/{corporationId}/payouts",
    params(("corporationId" = String, Path, description = "현재 run 법인 ID", pattern = "^[1-9][0-9]*$")),
    request_body = CorporationPayoutRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "결산 이익 범위의 법인 배당 지급 또는 멱등 재조회", body = CorporationDividendResponse),
        (status = 400, description = "명령 형식 또는 배당액이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run 법인을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "cursor·결산·배당가능이익 또는 법인 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "법인·개인 원장과 금융소득 반영 실패"),
    )
)]
async fn pay_corporation_dividend(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(corporation_id): Path<String>,
    request: Result<Json<CorporationPayoutRequest>, JsonRejection>,
) -> Result<Json<CorporationDividendResponse>, LifeRouteError> {
    let corporation_id =
        ResourceId::parse(&corporation_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    if request.gross_dividend_krw <= 0
        || request.gross_dividend_krw > MAX_JSON_SAFE_INTEGER as i64
        || !matches!(request.kind, CorporationPayoutKindRequest::Dividend)
    {
        return Err(LifeFailureCode::InvalidCommand.into());
    }
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let command = PayCorporationDividendCommand {
        command_id,
        cursor,
        corporation_id,
        gross_dividend_krw: request.gross_dividend_krw,
    };
    match state.pay_corporation_dividend(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/corporations/{corporationId}/months",
    params(
        ("corporationId" = String, Path, description = "현재 run 법인 ID", pattern = "^[1-9][0-9]*$"),
        ("cursor" = Option<String>, Query, description = "서명된 다음 페이지 cursor", max_length = 512)
    ),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "오름차순 법인 월 손익 최대 20건", body = CorporationOperatingMonthPageResponse),
        (status = 400, description = "법인 ID 또는 cursor가 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run 법인을 찾을 수 없음", body = LifeFailure),
        (status = 500, description = "법인 월 history 조회 또는 invariant 검증 실패"),
    )
)]
async fn corporation_operating_months(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(corporation_id): Path<String>,
    query: Result<Query<CorporationMonthsQuery>, QueryRejection>,
) -> Result<Json<CorporationOperatingMonthPageResponse>, LifeRouteError> {
    let corporation_id =
        ResourceId::parse(&corporation_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Query(query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    if query
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.is_empty() || cursor.len() > 512 || !cursor.is_ascii())
    {
        return Err(LifeFailureCode::InvalidCommand.into());
    }
    match state
        .corporation_operating_months(user.id, corporation_id, query.cursor)
        .await?
    {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum InsolvencyProcedureRequestKind {
    CashOnlyLiquidation,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InsolvencyCasePrepareRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    procedure_kind: InsolvencyProcedureRequestKind,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum InsolvencyActionRequestKind {
    Submit,
    Withdraw,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InsolvencyCaseActionRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    action: InsolvencyActionRequestKind,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
struct InsolvencyPageQuery {
    #[param(required = false, max_length = 512)]
    cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/insolvency",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run의 도산 기능, 자격과 사건 요약", body = InsolvencyOverviewResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터 또는 현재 run이 필요함", body = LifeFailure),
        (status = 500, description = "도산 상태 조회 또는 invariant 검증 실패"),
    )
)]
async fn insolvency_overview(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<InsolvencyOverviewResponse>, LifeRouteError> {
    match state.insolvency(user.id).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/insolvency/cases",
    request_body = InsolvencyCasePrepareRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "cash-only 청산 사건 준비 또는 멱등 재조회", body = InsolvencyCaseCommandResponse),
        (status = 400, description = "사건 준비 body 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "자격, cursor 또는 기존 사건이 충돌함", body = LifeFailure),
        (status = 422, description = "현재 자산·채무 구성을 안전하게 판정할 수 없음", body = LifeFailure),
        (status = 500, description = "사건 준비 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn prepare_insolvency_case(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<InsolvencyCasePrepareRequest>, JsonRejection>,
) -> Result<Json<InsolvencyCaseCommandResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let command = PrepareInsolvencyCaseCommand {
        command_id,
        cursor,
        procedure_kind: match request.procedure_kind {
            InsolvencyProcedureRequestKind::CashOnlyLiquidation => {
                InsolvencyProcedureKind::CashOnlyLiquidation
            }
        },
    };
    match state.prepare_insolvency_case(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/insolvency/{caseId}/actions",
    params(("caseId" = String, Path, description = "현재 run 도산 사건 ID", pattern = "^[1-9][0-9]*$")),
    request_body = InsolvencyCaseActionRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "사건 제출·철회 또는 멱등 재조회", body = InsolvencyCaseCommandResponse),
        (status = 400, description = "사건 ID 또는 action body 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 사건을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "사건 구성, 상태 또는 cursor가 충돌함", body = LifeFailure),
        (status = 422, description = "현재 자산·채무 구성을 안전하게 판정할 수 없음", body = LifeFailure),
        (status = 500, description = "사건 action transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn act_on_insolvency_case(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(case_id): Path<String>,
    request: Result<Json<InsolvencyCaseActionRequest>, JsonRejection>,
) -> Result<Json<InsolvencyCaseCommandResponse>, LifeRouteError> {
    let case_id = ResourceId::parse(&case_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let action = match request.action {
        InsolvencyActionRequestKind::Submit => InsolvencyActionState::Submit,
        InsolvencyActionRequestKind::Withdraw => InsolvencyActionState::Withdraw,
    };
    let command = ActOnInsolvencyCaseCommand {
        command_id,
        cursor,
        case_id,
        action,
    };
    match state.act_on_insolvency_case(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/insolvency/{caseId}",
    params(("caseId" = String, Path, description = "현재 run 도산 사건 ID", pattern = "^[1-9][0-9]*$")),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "사건 총계, 정책 provenance와 전이 이력", body = InsolvencyCaseDetailResponse),
        (status = 400, description = "사건 ID 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 사건을 찾을 수 없음", body = LifeFailure),
        (status = 500, description = "사건 상세 조회 또는 invariant 검증 실패"),
    )
)]
async fn insolvency_case_detail(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(case_id): Path<String>,
) -> Result<Json<InsolvencyCaseDetailResponse>, LifeRouteError> {
    let case_id = ResourceId::parse(&case_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    match state.insolvency_case_detail(user.id, case_id).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

fn insolvency_page_cursor(query: InsolvencyPageQuery) -> Result<Option<String>, LifeFailureCode> {
    if query
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.is_empty() || cursor.len() > 512 || !cursor.is_ascii())
    {
        return Err(LifeFailureCode::InvalidCommand);
    }
    Ok(query.cursor)
}

#[utoipa::path(
    get,
    path = "/api/insolvency/{caseId}/claims",
    params(
        ("caseId" = String, Path, description = "현재 run 도산 사건 ID", pattern = "^[1-9][0-9]*$"),
        InsolvencyPageQuery
    ),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "ID 오름차순 채권 page", body = InsolvencyClaimPageResponse),
        (status = 400, description = "사건 ID 또는 cursor 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 사건을 찾을 수 없음", body = LifeFailure),
        (status = 500, description = "채권 page 조회 또는 invariant 검증 실패"),
    )
)]
async fn insolvency_claims(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(case_id): Path<String>,
    query: Result<Query<InsolvencyPageQuery>, QueryRejection>,
) -> Result<Json<InsolvencyClaimPageResponse>, LifeRouteError> {
    let case_id = ResourceId::parse(&case_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Query(query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let cursor = insolvency_page_cursor(query)?;
    match state.insolvency_claims(user.id, case_id, cursor).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/insolvency/{caseId}/liquidations",
    params(
        ("caseId" = String, Path, description = "현재 run 도산 사건 ID", pattern = "^[1-9][0-9]*$"),
        InsolvencyPageQuery
    ),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "지갑 자산과 청산 배분 page", body = InsolvencyLiquidationPageResponse),
        (status = 400, description = "사건 ID 또는 cursor 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 사건을 찾을 수 없음", body = LifeFailure),
        (status = 500, description = "청산 page 조회 또는 invariant 검증 실패"),
    )
)]
async fn insolvency_liquidations(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(case_id): Path<String>,
    query: Result<Query<InsolvencyPageQuery>, QueryRejection>,
) -> Result<Json<InsolvencyLiquidationPageResponse>, LifeRouteError> {
    let case_id = ResourceId::parse(&case_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Query(query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let cursor = insolvency_page_cursor(query)?;
    match state
        .insolvency_liquidations(user.id, case_id, cursor)
        .await?
    {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WelfareProgramsQuery {}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WelfareApplicationRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    program_version_id: String,
}

impl TryFrom<WelfareApplicationRequest> for ApplyWelfareProgramCommand {
    type Error = LifeFailureCode;

    fn try_from(request: WelfareApplicationRequest) -> Result<Self, Self::Error> {
        let (command_id, cursor) = life_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        let program_version_id = ResourceId::parse(&request.program_version_id)
            .map_err(|_| LifeFailureCode::InvalidCommand)?;
        Ok(Self {
            command_id,
            cursor,
            program_version_id,
        })
    }
}

#[utoipa::path(
    get,
    path = "/api/welfare/programs",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run의 복지 프로그램과 서버 권위 자격 판정", body = WelfareProgramsResponse),
        (status = 400, description = "복지 프로그램 query 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터와 현재 run이 필요하거나 자산 평가를 완료할 수 없음", body = LifeFailure),
        (status = 500, description = "복지 프로그램 평가 또는 조회 실패"),
    )
)]
async fn welfare_programs(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<WelfareProgramsQuery>, QueryRejection>,
) -> Result<Json<WelfareProgramsResponse>, LifeRouteError> {
    let Query(_query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    state
        .welfare_programs(user.id)
        .await?
        .map(Json)
        .ok_or_else(|| LifeFailureCode::CharacterRequired.into())
}

#[utoipa::path(
    post,
    path = "/api/welfare/applications",
    request_body = WelfareApplicationRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "복지 신청 승인과 D+1 지급 예약 또는 멱등 재조회", body = WelfareApplicationResponse),
        (status = 400, description = "복지 신청 요청 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 복지 프로그램을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "신청 자격·평가·cursor 또는 계약 상태가 충돌함", body = LifeFailure),
        (status = 500, description = "복지 신청 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn apply_welfare_program(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<WelfareApplicationRequest>, JsonRejection>,
) -> Result<Json<WelfareApplicationResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = ApplyWelfareProgramCommand::try_from(request)?;
    match state.apply_welfare_program(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum HousingRegionRequest {
    CapitalArea,
    Metropolitan,
    SmallCity,
    Rural,
}

impl From<HousingRegionRequest> for LifeRegionKey {
    fn from(region: HousingRegionRequest) -> Self {
        match region {
            HousingRegionRequest::CapitalArea => Self::CapitalArea,
            HousingRegionRequest::Metropolitan => Self::Metropolitan,
            HousingRegionRequest::SmallCity => Self::SmallCity,
            HousingRegionRequest::Rural => Self::Rural,
        }
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
struct HousingListingsQuery {
    #[param(required = false)]
    region: Option<HousingRegionRequest>,
}

#[utoipa::path(
    get,
    path = "/api/housing/listings",
    params(HousingListingsQuery),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 월과 지역의 부동산 지수·유한 매물", body = HousingListingsResponse),
        (status = 400, description = "지역 또는 query 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터와 현재 run이 필요함", body = LifeFailure),
        (status = 500, description = "부동산 지수·매물 조회 실패"),
    )
)]
async fn housing_listings(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<HousingListingsQuery>, QueryRejection>,
) -> Result<Json<HousingListingsResponse>, LifeRouteError> {
    let Query(query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let query = HousingListingsQueryState {
        region: query.region.map(Into::into),
    };
    state
        .housing_listings(user.id, query)
        .await?
        .map(Json)
        .ok_or_else(|| LifeFailureCode::CharacterRequired.into())
}

#[utoipa::path(
    get,
    path = "/api/housing/holdings",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run의 주택 매수 capability와 활성 보유주택", body = HousingPropertyHoldingsResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터와 현재 run이 필요함", body = LifeFailure),
        (status = 500, description = "보유주택 조회 실패"),
    )
)]
async fn housing_property_holdings(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<HousingPropertyHoldingsResponse>, LifeRouteError> {
    state
        .housing_property_holdings(user.id)
        .await?
        .map(Json)
        .ok_or_else(|| LifeFailureCode::CharacterRequired.into())
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum HousingLeaseOfferKindRequest {
    Jeonse,
    MonthlyRent,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum JeonseHousingLeaseOfferKindRequest {
    Jeonse,
}

impl From<HousingLeaseOfferKindRequest> for HousingLeaseOfferKind {
    fn from(offer_kind: HousingLeaseOfferKindRequest) -> Self {
        match offer_kind {
            HousingLeaseOfferKindRequest::Jeonse => Self::Jeonse,
            HousingLeaseOfferKindRequest::MonthlyRent => Self::MonthlyRent,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LeaseDepositLoanQuoteRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    listing_id: String,
    offer_kind: JeonseHousingLeaseOfferKindRequest,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    product_version_id: String,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    principal_krw: i64,
}

impl TryFrom<LeaseDepositLoanQuoteRequest> for CreateLeaseDepositLoanQuoteCommand {
    type Error = LifeFailureCode;

    fn try_from(request: LeaseDepositLoanQuoteRequest) -> Result<Self, Self::Error> {
        let JeonseHousingLeaseOfferKindRequest::Jeonse = request.offer_kind;
        if request.principal_krw <= 0 || request.principal_krw > MAX_JSON_SAFE_INTEGER as i64 {
            return Err(LifeFailureCode::InvalidCommand);
        }
        let (command_id, cursor) = life_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        let listing_id =
            ResourceId::parse(&request.listing_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
        let product_version_id = ResourceId::parse(&request.product_version_id)
            .map_err(|_| LifeFailureCode::InvalidCommand)?;

        Ok(Self {
            command_id,
            cursor,
            listing_id,
            offer_kind: HousingLeaseOfferKind::Jeonse,
            product_version_id,
            principal_krw: request.principal_krw,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MortgageQuoteRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    listing_id: String,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    product_version_id: String,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    principal_krw: i64,
}

impl TryFrom<MortgageQuoteRequest> for CreateMortgageQuoteCommand {
    type Error = LifeFailureCode;

    fn try_from(request: MortgageQuoteRequest) -> Result<Self, Self::Error> {
        if request.principal_krw <= 0 || request.principal_krw > MAX_JSON_SAFE_INTEGER as i64 {
            return Err(LifeFailureCode::InvalidCommand);
        }
        let (command_id, cursor) = life_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        let listing_id =
            ResourceId::parse(&request.listing_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
        let product_version_id = ResourceId::parse(&request.product_version_id)
            .map_err(|_| LifeFailureCode::InvalidCommand)?;

        Ok(Self {
            command_id,
            cursor,
            listing_id,
            product_version_id,
            principal_krw: request.principal_krw,
        })
    }
}

fn deserialize_required_nullable_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PropertyPurchaseRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    listing_id: String,
    #[serde(deserialize_with = "deserialize_required_nullable_string")]
    #[schema(
        required = true,
        nullable,
        value_type = Option<String>,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    mortgage_quote_id: Option<String>,
}

impl TryFrom<PropertyPurchaseRequest> for PurchasePropertyCommand {
    type Error = LifeFailureCode;

    fn try_from(request: PropertyPurchaseRequest) -> Result<Self, Self::Error> {
        let (command_id, cursor) = life_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        let listing_id =
            ResourceId::parse(&request.listing_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
        let mortgage_quote_id = request
            .mortgage_quote_id
            .as_deref()
            .map(ResourceId::parse)
            .transpose()
            .map_err(|_| LifeFailureCode::InvalidCommand)?;

        Ok(Self {
            command_id,
            cursor,
            listing_id,
            mortgage_quote_id,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PropertySaleOrderCreateRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    holding_id: String,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    asking_price_krw: i64,
}

impl TryFrom<PropertySaleOrderCreateRequest> for CreatePropertySaleOrderCommand {
    type Error = LifeFailureCode;

    fn try_from(request: PropertySaleOrderCreateRequest) -> Result<Self, Self::Error> {
        if request.asking_price_krw <= 0 || request.asking_price_krw > MAX_JSON_SAFE_INTEGER as i64
        {
            return Err(LifeFailureCode::InvalidCommand);
        }
        let (command_id, cursor) = life_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        let holding_id =
            ResourceId::parse(&request.holding_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
        Ok(Self {
            command_id,
            cursor,
            holding_id,
            asking_price_krw: request.asking_price_krw,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PropertySaleOrderRepriceRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    asking_price_krw: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PropertySaleOrderCancelRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
}

fn reprice_property_sale_order_command(
    order_id: ResourceId,
    request: PropertySaleOrderRepriceRequest,
) -> Result<RepricePropertySaleOrderCommand, LifeFailureCode> {
    if request.asking_price_krw <= 0 || request.asking_price_krw > MAX_JSON_SAFE_INTEGER as i64 {
        return Err(LifeFailureCode::InvalidCommand);
    }
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    Ok(RepricePropertySaleOrderCommand {
        command_id,
        cursor,
        order_id,
        asking_price_krw: request.asking_price_krw,
    })
}

fn cancel_property_sale_order_command(
    order_id: ResourceId,
    request: PropertySaleOrderCancelRequest,
) -> Result<CancelPropertySaleOrderCommand, LifeFailureCode> {
    let (command_id, cursor) = life_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    Ok(CancelPropertySaleOrderCommand {
        command_id,
        cursor,
        order_id,
    })
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
struct PropertyHistoryQuery {
    #[param(required = false, value_type = Option<String>, pattern = "^[1-9][0-9]*$")]
    before: Option<String>,
    #[param(required = false, value_type = Option<u8>, minimum = 1, maximum = 20)]
    limit: Option<String>,
}

fn property_history_page(
    query: PropertyHistoryQuery,
) -> Result<(Option<ResourceId>, u8), LifeFailureCode> {
    let before = query
        .before
        .as_deref()
        .map(ResourceId::parse)
        .transpose()
        .map_err(|_| LifeFailureCode::InvalidCommand)?;
    let limit = query
        .limit
        .as_deref()
        .map(str::parse::<u8>)
        .transpose()
        .map_err(|_| LifeFailureCode::InvalidCommand)?
        .unwrap_or(DEFAULT_PROPERTY_HISTORY_PAGE_SIZE);
    if !(1..=MAX_PROPERTY_HISTORY_PAGE_SIZE).contains(&limit) {
        return Err(LifeFailureCode::InvalidCommand);
    }
    Ok((before, limit))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(untagged)]
enum StartHousingLeaseRequest {
    Cash(StartHousingLeaseCashRequest),
    Financed(StartHousingLeaseFinancedRequest),
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartHousingLeaseCashRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    listing_id: String,
    offer_kind: HousingLeaseOfferKindRequest,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartHousingLeaseFinancedRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    listing_id: String,
    offer_kind: JeonseHousingLeaseOfferKindRequest,
    #[schema(
        value_type = String,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    loan_quote_id: String,
}

impl TryFrom<StartHousingLeaseRequest> for StartHousingLeaseCommand {
    type Error = LifeFailureCode;

    fn try_from(request: StartHousingLeaseRequest) -> Result<Self, Self::Error> {
        let (
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            listing_id,
            offer_kind,
            loan_quote_id,
        ) = match request {
            StartHousingLeaseRequest::Cash(request) => (
                request.command_id,
                request.expected_run_revision,
                request.expected_state_revision,
                request.expected_game_day,
                request.listing_id,
                request.offer_kind,
                None,
            ),
            StartHousingLeaseRequest::Financed(request) => {
                let JeonseHousingLeaseOfferKindRequest::Jeonse = request.offer_kind;
                let loan_quote_id = ResourceId::parse(&request.loan_quote_id)
                    .map_err(|_| LifeFailureCode::InvalidCommand)?;
                (
                    request.command_id,
                    request.expected_run_revision,
                    request.expected_state_revision,
                    request.expected_game_day,
                    request.listing_id,
                    HousingLeaseOfferKindRequest::Jeonse,
                    Some(loan_quote_id),
                )
            }
        };
        let (command_id, cursor) = life_command_parts(
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
        )?;
        let listing_id =
            ResourceId::parse(&listing_id).map_err(|_| LifeFailureCode::InvalidCommand)?;

        Ok(Self {
            command_id,
            cursor,
            listing_id,
            offer_kind: offer_kind.into(),
            loan_quote_id,
        })
    }
}

#[utoipa::path(
    get,
    path = "/api/housing/leases/current",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run의 임대차 capability와 활성 계약", body = HousingLeaseCurrentResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터와 현재 run이 필요함", body = LifeFailure),
        (status = 500, description = "현재 임대차 계약 조회 실패"),
    )
)]
async fn housing_lease_current(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<HousingLeaseCurrentResponse>, LifeRouteError> {
    state
        .housing_lease_current(user.id)
        .await?
        .map(Json)
        .ok_or_else(|| LifeFailureCode::CharacterRequired.into())
}

#[utoipa::path(
    post,
    path = "/api/housing/mortgage-quotes",
    request_body = MortgageQuoteRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "주택담보대출 견적 생성 또는 멱등 재조회", body = MortgageQuoteResponse),
        (status = 400, description = "주택담보대출 견적 요청 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 주택담보대출 견적을 만들 수 없음", body = LifeFailure),
        (status = 500, description = "주택담보대출 견적 저장 실패"),
    )
)]
async fn quote_mortgage(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<MortgageQuoteRequest>, JsonRejection>,
) -> Result<Json<MortgageQuoteResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = CreateMortgageQuoteCommand::try_from(request)?;
    match state.quote_mortgage(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/housing/purchases",
    request_body = PropertyPurchaseRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현금 또는 주택담보대출 매수와 owner 이사", body = PropertyPurchaseResponse),
        (status = 400, description = "주택 매수 요청 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 주택을 매수할 수 없음", body = LifeFailure),
        (status = 500, description = "주택 매수 transaction 또는 스냅샷 조립 실패"),
    )
)]
async fn purchase_property(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<PropertyPurchaseRequest>, JsonRejection>,
) -> Result<Json<PropertyPurchaseResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = PurchasePropertyCommand::try_from(request)?;
    match state.purchase_property(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/housing/sales",
    params(PropertyHistoryQuery),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run의 부동산 매도 주문 이력", body = PropertySaleOrdersResponse),
        (status = 400, description = "매도 주문 cursor 또는 limit이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "캐릭터와 현재 run이 필요함", body = LifeFailure),
        (status = 500, description = "매도 주문 이력 조회 실패"),
    )
)]
async fn property_sale_orders(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<PropertyHistoryQuery>, QueryRejection>,
) -> Result<Json<PropertySaleOrdersResponse>, LifeRouteError> {
    let Query(query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let (before, limit) = property_history_page(query)?;
    match state
        .property_sale_orders(user.id, PropertySaleOrderPageQuery { before, limit })
        .await?
    {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/housing/sales",
    request_body = PropertySaleOrderCreateRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "보유주택 매도 주문 생성 또는 멱등 재조회", body = PropertySaleOrderListingResponse),
        (status = 400, description = "매도 주문 요청 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 보유주택을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "현재 상태에서 매도 주문을 만들 수 없음", body = LifeFailure),
        (status = 500, description = "매도 주문 저장 실패"),
    )
)]
async fn create_property_sale_order(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<PropertySaleOrderCreateRequest>, JsonRejection>,
) -> Result<Json<PropertySaleOrderListingResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = CreatePropertySaleOrderCommand::try_from(request)?;
    match state.create_property_sale_order(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/housing/sales/{orderId}/reprice",
    params(("orderId" = String, Path, pattern = "^[1-9][0-9]*$")),
    request_body = PropertySaleOrderRepriceRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "매도 주문 가격 변경 또는 멱등 재조회", body = PropertySaleOrderListingResponse),
        (status = 400, description = "주문 ID 또는 가격 변경 요청이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 매도 주문을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "현재 상태에서 주문 가격을 바꿀 수 없음", body = LifeFailure),
        (status = 500, description = "매도 주문 가격 변경 실패"),
    )
)]
async fn reprice_property_sale_order(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(order_id): Path<String>,
    request: Result<Json<PropertySaleOrderRepriceRequest>, JsonRejection>,
) -> Result<Json<PropertySaleOrderListingResponse>, LifeRouteError> {
    let order_id = ResourceId::parse(&order_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = reprice_property_sale_order_command(order_id, request)?;
    match state.reprice_property_sale_order(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/housing/sales/{orderId}/cancel",
    params(("orderId" = String, Path, pattern = "^[1-9][0-9]*$")),
    request_body = PropertySaleOrderCancelRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "매도 주문 취소 또는 멱등 재조회", body = PropertySaleOrderCancellationResponse),
        (status = 400, description = "주문 ID 또는 취소 요청이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 매도 주문을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "현재 상태에서 주문을 취소할 수 없음", body = LifeFailure),
        (status = 500, description = "매도 주문 취소 실패"),
    )
)]
async fn cancel_property_sale_order(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(order_id): Path<String>,
    request: Result<Json<PropertySaleOrderCancelRequest>, JsonRejection>,
) -> Result<Json<PropertySaleOrderCancellationResponse>, LifeRouteError> {
    let order_id = ResourceId::parse(&order_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = cancel_property_sale_order_command(order_id, request)?;
    match state.cancel_property_sale_order(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/housing/holdings/{holdingId}/tax-events",
    params(
        ("holdingId" = String, Path, pattern = "^[1-9][0-9]*$"),
        PropertyHistoryQuery
    ),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "보유주택의 취득·보유·양도세 이력", body = PropertyTaxEventsResponse),
        (status = 400, description = "보유주택 ID나 cursor 또는 limit이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run의 보유주택을 찾을 수 없음", body = LifeFailure),
        (status = 409, description = "캐릭터와 현재 run이 필요함", body = LifeFailure),
        (status = 500, description = "부동산 세금 이력 조회 실패"),
    )
)]
async fn property_tax_events(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(holding_id): Path<String>,
    query: Result<Query<PropertyHistoryQuery>, QueryRejection>,
) -> Result<Json<PropertyTaxEventsResponse>, LifeRouteError> {
    let holding_id = ResourceId::parse(&holding_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Query(query) = query.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let (before, limit) = property_history_page(query)?;
    match state
        .property_tax_events(
            user.id,
            holding_id,
            PropertyTaxEventPageQuery { before, limit },
        )
        .await?
    {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/housing/lease-deposit-loan-quotes",
    request_body = LeaseDepositLoanQuoteRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "전세자금대출 견적 생성 또는 멱등 재조회", body = LeaseDepositLoanQuoteResponse),
        (status = 400, description = "전세자금대출 견적 요청 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 전세자금대출 견적을 만들 수 없음", body = LifeFailure),
        (status = 500, description = "전세자금대출 견적 저장 실패"),
    )
)]
async fn quote_lease_deposit_loan(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<LeaseDepositLoanQuoteRequest>, JsonRejection>,
) -> Result<Json<LeaseDepositLoanQuoteResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = CreateLeaseDepositLoanQuoteCommand::try_from(request)?;
    match state.quote_lease_deposit_loan(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/housing/leases",
    request_body = StartHousingLeaseRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현금 임대차 계약과 원자적 이사 또는 멱등 재조회", body = HousingLeaseMoveResponse),
        (status = 400, description = "임대차 계약 요청 형식이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 게임 상태에서 임대차 계약을 실행할 수 없음", body = LifeFailure),
        (status = 500, description = "임대차 계약 또는 스냅샷 조립 실패"),
    )
)]
async fn start_housing_lease(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<StartHousingLeaseRequest>, JsonRejection>,
) -> Result<Json<HousingLeaseMoveResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = StartHousingLeaseCommand::try_from(request)?;
    match state.start_housing_lease(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/housing/lease-arrears/{id}/payments",
    params(("id" = String, Path, pattern = "^[1-9][0-9]*$")),
    request_body = LeaseArrearPaymentRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "월세 연체 일부 또는 전액 상환", body = LeaseArrearPaymentResponse),
        (status = 400, description = "월세 연체 상환 요청이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 월세 연체를 상환할 수 없음", body = LifeFailure),
        (status = 500, description = "월세 연체 상환 저장 실패"),
    )
)]
async fn pay_lease_arrear(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<LeaseArrearPaymentRequest>, JsonRejection>,
) -> Result<Json<LeaseArrearPaymentResponse>, LifeRouteError> {
    let arrear_id = ResourceId::parse(&id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = lease_arrear_payment_command(arrear_id, request)?;
    match state.pay_lease_arrear(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoanDetailQuery {}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
struct LoanInstallmentsQuery {
    #[param(
        value_type = String,
        required = false,
        min_length = 11,
        max_length = 43,
        pattern = "^v1\\.l[1-9][0-9]*\\.i(?:0|[1-9][0-9]*)\\.p(?:0|[1-9][0-9]*)$"
    )]
    before: Option<String>,
    #[param(
        value_type = u8,
        required = false,
        default = 50,
        minimum = 1,
        maximum = 50
    )]
    limit: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct LoanNotFoundFailure {
    code: LoanNotFoundCodeSnapshot,
    message: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum LoanNotFoundCodeSnapshot {
    LoanNotFound,
}

enum LoanReadRouteError {
    InvalidCommand,
    LoanNotFound,
    Internal(AppError),
}

impl From<anyhow::Error> for LoanReadRouteError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for LoanReadRouteError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::InvalidCommand => (
                StatusCode::BAD_REQUEST,
                Json(LifeFailure {
                    code: LifeFailureCodeSnapshot::InvalidCommand,
                    message: life_failure_message(LifeFailureCode::InvalidCommand),
                }),
            )
                .into_response(),
            Self::LoanNotFound => (
                StatusCode::NOT_FOUND,
                Json(LoanNotFoundFailure {
                    code: LoanNotFoundCodeSnapshot::LoanNotFound,
                    message: "대출을 찾을 수 없습니다",
                }),
            )
                .into_response(),
            Self::Internal(error) => error.into_response(),
        }
    }
}

fn parse_loan_installment_cursor(
    value: &str,
    expected_loan_id: ResourceId,
) -> Result<LoanInstallmentPageCursor, LifeFailureCode> {
    if !(11..=43).contains(&value.len()) {
        return Err(LifeFailureCode::InvalidCommand);
    }
    let remainder = value
        .strip_prefix("v1.l")
        .ok_or(LifeFailureCode::InvalidCommand)?;
    let (loan_id, remainder) = remainder
        .split_once(".i")
        .ok_or(LifeFailureCode::InvalidCommand)?;
    let (installment_before, payment_before) = remainder
        .split_once(".p")
        .ok_or(LifeFailureCode::InvalidCommand)?;
    let loan_id = ResourceId::parse(loan_id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    if loan_id != expected_loan_id {
        return Err(LifeFailureCode::InvalidCommand);
    }
    let installment_before = parse_loan_cursor_u16(installment_before)?;
    let payment_before = parse_loan_cursor_u32(payment_before)?;
    let cursor = LoanInstallmentPageCursor {
        loan_id,
        installment_before,
        payment_before,
    };
    let canonical = format!(
        "v1.l{}.i{}.p{}",
        cursor.loan_id,
        cursor.installment_before.unwrap_or(0),
        cursor.payment_before.unwrap_or(0)
    );
    if canonical != value {
        return Err(LifeFailureCode::InvalidCommand);
    }
    Ok(cursor)
}

fn parse_loan_cursor_u16(value: &str) -> Result<Option<u16>, LifeFailureCode> {
    if value == "0" {
        return Ok(None);
    }
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(LifeFailureCode::InvalidCommand);
    }
    value
        .parse::<u16>()
        .map(Some)
        .map_err(|_| LifeFailureCode::InvalidCommand)
}

fn parse_loan_cursor_u32(value: &str) -> Result<Option<u32>, LifeFailureCode> {
    if value == "0" {
        return Ok(None);
    }
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(LifeFailureCode::InvalidCommand);
    }
    value
        .parse::<u32>()
        .map(Some)
        .map_err(|_| LifeFailureCode::InvalidCommand)
}

fn loan_installment_page_query(
    loan_id: ResourceId,
    query: LoanInstallmentsQuery,
) -> Result<LoanInstallmentPageQuery, LifeFailureCode> {
    let before = query
        .before
        .as_deref()
        .map(|value| parse_loan_installment_cursor(value, loan_id))
        .transpose()?;
    let limit = query
        .limit
        .as_deref()
        .map(str::parse::<u8>)
        .transpose()
        .map_err(|_| LifeFailureCode::InvalidCommand)?
        .unwrap_or(DEFAULT_LOAN_INSTALLMENT_PAGE_SIZE);
    if !(1..=MAX_LOAN_INSTALLMENT_PAGE_SIZE).contains(&limit) {
        return Err(LifeFailureCode::InvalidCommand);
    }
    Ok(LoanInstallmentPageQuery { before, limit })
}

#[utoipa::path(
    get,
    path = "/api/loans/{loanId}",
    params((
        "loanId" = String,
        Path,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$",
        description = "조회할 대출 계약 ID"
    )),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 run의 대출 계약 상세", body = LoanDetailResponse),
        (status = 400, description = "대출 계약 ID나 query가 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run에서 소유한 대출을 찾을 수 없음", body = LoanNotFoundFailure),
        (status = 500, description = "대출 계약 상세 조회 실패"),
    )
)]
async fn loan_detail(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(loan_id): Path<String>,
    query: Result<Query<LoanDetailQuery>, QueryRejection>,
) -> Result<Json<LoanDetailResponse>, LoanReadRouteError> {
    let Query(_) = query.map_err(|_| LoanReadRouteError::InvalidCommand)?;
    let loan_id = ResourceId::parse(&loan_id).map_err(|_| LoanReadRouteError::InvalidCommand)?;
    state
        .loan_detail(user.id, loan_id)
        .await?
        .map(Json)
        .ok_or(LoanReadRouteError::LoanNotFound)
}

#[utoipa::path(
    get,
    path = "/api/loans/{loanId}/installments",
    params(
        (
            "loanId" = String,
            Path,
            min_length = 1,
            max_length = 20,
            pattern = "^[1-9][0-9]*$",
            description = "조회할 대출 계약 ID"
        ),
        LoanInstallmentsQuery
    ),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "상환 회차와 납부 이력 dual window", body = LoanInstallmentsResponse),
        (status = 400, description = "대출 ID, cursor 또는 limit이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 404, description = "현재 run에서 소유한 대출을 찾을 수 없음", body = LoanNotFoundFailure),
        (status = 500, description = "대출 상환 이력 조회 실패"),
    )
)]
async fn loan_installments(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(loan_id): Path<String>,
    query: Result<Query<LoanInstallmentsQuery>, QueryRejection>,
) -> Result<Json<LoanInstallmentsResponse>, LoanReadRouteError> {
    let loan_id = ResourceId::parse(&loan_id).map_err(|_| LoanReadRouteError::InvalidCommand)?;
    let Query(query) = query.map_err(|_| LoanReadRouteError::InvalidCommand)?;
    let query = loan_installment_page_query(loan_id, query)
        .map_err(|_| LoanReadRouteError::InvalidCommand)?;
    state
        .loan_installments(user.id, loan_id, query)
        .await?
        .map(Json)
        .ok_or(LoanReadRouteError::LoanNotFound)
}

#[utoipa::path(
    get,
    path = "/api/loans/products",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 시작 또는 run에 pin된 대출 상품", body = LoanProductCatalogResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "대출 상품 조회 실패"),
    )
)]
async fn loan_products(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<LoanProductCatalogResponse>, AppError> {
    Ok(Json(state.loan_products(user.id).await?))
}

#[utoipa::path(
    post,
    path = "/api/loans/quotes",
    request_body = LoanQuoteRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "대출 견적 생성 또는 멱등 재조회", body = LoanQuoteResponse),
        (status = 400, description = "대출 견적 요청이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 대출 견적을 만들 수 없음", body = LifeFailure),
        (status = 500, description = "대출 견적 저장 실패"),
    )
)]
async fn quote_loan(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<LoanQuoteRequest>, JsonRejection>,
) -> Result<Json<LoanQuoteResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = CreateLoanQuoteCommand::try_from(request)?;
    match state.quote_loan(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/loans",
    request_body = LoanExecutionRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "대출 실행 또는 멱등 재조회", body = LoanExecutionResponse),
        (status = 400, description = "대출 실행 요청이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 대출을 실행할 수 없음", body = LifeFailure),
        (status = 500, description = "대출 실행 저장 실패"),
    )
)]
async fn execute_loan(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<LoanExecutionRequest>, JsonRejection>,
) -> Result<Json<LoanExecutionResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = ExecuteLoanCommand::try_from(request)?;
    match state.execute_loan(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/loans/{loanId}/prepayments",
    params((
        "loanId" = String,
        Path,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$",
        description = "중도상환할 대출 계약 ID"
    )),
    request_body = LoanPrepaymentRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "대출 중도상환 또는 멱등 재조회", body = LoanPrepaymentResponse),
        (status = 400, description = "대출 중도상환 요청이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 대출을 중도상환할 수 없음", body = LifeFailure),
        (status = 500, description = "대출 중도상환 저장 실패"),
    )
)]
async fn prepay_loan(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(loan_id): Path<String>,
    request: Result<Json<LoanPrepaymentRequest>, JsonRejection>,
) -> Result<Json<LoanPrepaymentResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = prepay_loan_command(&loan_id, request)?;
    match state.prepay_loan(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    get,
    path = "/api/credit",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 공개 신용 사유와 활성 대출 요약", body = CreditResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "신용 요약 조회 실패"),
    )
)]
async fn credit(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<CreditResponse>, AppError> {
    Ok(Json(state.credit(user.id).await?))
}

#[utoipa::path(
    get,
    path = "/api/life/budget",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 생활비 산정 근거와 예산 선택", body = LifeBudgetResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "생활비 예산 조회 실패"),
    )
)]
async fn life_budget(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<LifeBudgetResponse>, AppError> {
    Ok(Json(state.life_budget(user.id).await?))
}

#[utoipa::path(
    put,
    path = "/api/life/budget",
    request_body = LifeBudgetUpdateRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "예산 변경 또는 멱등 재조회", body = LifeBudgetUpdateResponse),
        (status = 400, description = "예산 변경 요청이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 예산을 변경할 수 없음", body = LifeFailure),
        (status = 500, description = "예산 변경 저장 실패"),
    )
)]
async fn update_life_budget(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<LifeBudgetUpdateRequest>, JsonRejection>,
) -> Result<Json<LifeBudgetUpdateResponse>, LifeRouteError> {
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = UpdateLifeBudgetCommand::try_from(request)?;
    match state.update_life_budget(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/life/arrears/{id}/payments",
    params(("id" = String, Path, pattern = "^[1-9][0-9]*$")),
    request_body = EssentialArrearPaymentRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "필수 생활비 미납 일부 또는 전액 상환", body = EssentialArrearPaymentResponse),
        (status = 400, description = "미납 상환 요청이 잘못됨", body = LifeFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 미납액을 상환할 수 없음", body = LifeFailure),
        (status = 500, description = "미납 상환 저장 실패"),
    )
)]
async fn pay_essential_arrear(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<EssentialArrearPaymentRequest>, JsonRejection>,
) -> Result<Json<EssentialArrearPaymentResponse>, LifeRouteError> {
    let arrear_id = ResourceId::parse(&id).map_err(|_| LifeFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| LifeFailureCode::InvalidCommand)?;
    let command = essential_arrear_payment_command(arrear_id, request)?;
    match state.pay_essential_arrear(user.id, &command).await? {
        LifeCommandResult::Applied(response) => Ok(Json(*response)),
        LifeCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum CareerArtifactKindRequest {
    Portfolio,
    Resume,
    LinkedinProfile,
}

impl From<CareerArtifactKindRequest> for ArtifactKind {
    fn from(kind: CareerArtifactKindRequest) -> Self {
        match kind {
            CareerArtifactKindRequest::Portfolio => Self::Portfolio,
            CareerArtifactKindRequest::Resume => Self::Resume,
            CareerArtifactKindRequest::LinkedinProfile => Self::LinkedinProfile,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum CareerIndustryRequest {
    ItSoftware,
    FinanceInsurance,
    Manufacturing,
    ConstructionEngineering,
    RetailService,
    PublicSocial,
}

impl From<CareerIndustryRequest> for Industry {
    fn from(industry: CareerIndustryRequest) -> Self {
        match industry {
            CareerIndustryRequest::ItSoftware => Self::ItSoftware,
            CareerIndustryRequest::FinanceInsurance => Self::FinanceInsurance,
            CareerIndustryRequest::Manufacturing => Self::Manufacturing,
            CareerIndustryRequest::ConstructionEngineering => Self::ConstructionEngineering,
            CareerIndustryRequest::RetailService => Self::RetailService,
            CareerIndustryRequest::PublicSocial => Self::PublicSocial,
        }
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
struct CareerPageParams {
    #[param(
        value_type = String,
        required = false,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    before: Option<String>,
    #[param(
        value_type = u32,
        required = false,
        default = 50,
        minimum = 1,
        maximum = 200
    )]
    limit: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CareerArtifactParams {
    kind: Option<CareerArtifactKindRequest>,
    #[param(
        value_type = String,
        required = false,
        min_length = 1,
        max_length = 20,
        pattern = "^[1-9][0-9]*$"
    )]
    before: Option<String>,
    #[param(
        value_type = u32,
        required = false,
        default = 50,
        minimum = 1,
        maximum = 200
    )]
    limit: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum CareerPlatformRequest {
    Sarangbang,
    Jobkorea,
    Saramin,
    Wanted,
    Linkedin,
    Work24,
}

impl From<CareerPlatformRequest> for CareerPlatform {
    fn from(platform: CareerPlatformRequest) -> Self {
        match platform {
            CareerPlatformRequest::Sarangbang => Self::Sarangbang,
            CareerPlatformRequest::Jobkorea => Self::Jobkorea,
            CareerPlatformRequest::Saramin => Self::Saramin,
            CareerPlatformRequest::Wanted => Self::Wanted,
            CareerPlatformRequest::Linkedin => Self::Linkedin,
            CareerPlatformRequest::Work24 => Self::Work24,
        }
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CareerJobsParams {
    platform: Option<CareerPlatformRequest>,
    industry: Option<CareerIndustryRequest>,
    #[param(
        value_type = String,
        required = false,
        min_length = 64,
        max_length = 64,
        pattern = "^[0-9a-f]{64}$"
    )]
    before: Option<String>,
    #[param(value_type = u32, required = false, default = 50, minimum = 1, maximum = 200)]
    limit: Option<String>,
}

fn career_page_query(
    before: Option<String>,
    limit: Option<String>,
) -> Result<CareerPageQuery, CareerFailureCode> {
    let before = before
        .as_deref()
        .map(ResourceId::parse)
        .transpose()
        .map_err(|_| CareerFailureCode::InvalidCommand)?
        .map(ResourceId::get);
    let limit = limit
        .as_deref()
        .map(str::parse::<u32>)
        .transpose()
        .map_err(|_| CareerFailureCode::InvalidCommand)?
        .unwrap_or(DEFAULT_CAREER_PAGE_SIZE);
    if !(1..=MAX_CAREER_PAGE_SIZE).contains(&limit) {
        return Err(CareerFailureCode::InvalidCommand);
    }
    Ok(CareerPageQuery { before, limit })
}

fn career_jobs_page_query(
    params: CareerJobsParams,
) -> Result<CareerJobsPageQuery, CareerFailureCode> {
    let before = params
        .before
        .map(|value| {
            if is_posting_key(&value) {
                Ok(value)
            } else {
                Err(CareerFailureCode::InvalidCommand)
            }
        })
        .transpose()?;
    let limit = params
        .limit
        .as_deref()
        .map(str::parse::<u32>)
        .transpose()
        .map_err(|_| CareerFailureCode::InvalidCommand)?
        .unwrap_or(DEFAULT_CAREER_PAGE_SIZE);
    if !(1..=MAX_CAREER_PAGE_SIZE).contains(&limit) {
        return Err(CareerFailureCode::InvalidCommand);
    }
    Ok(CareerJobsPageQuery {
        before,
        limit,
        platform: params.platform.map(CareerPlatform::from),
        industry: params.industry.map(Industry::from),
    })
}

fn is_posting_key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CareerCursorRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
}

impl CareerCursorRequest {
    fn into_parts(self) -> Result<(CommandId, CommandCursor), CareerFailureCode> {
        career_command_parts(
            self.command_id,
            self.expected_run_revision,
            self.expected_state_revision,
            self.expected_game_day,
        )
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MilitaryServiceStartRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    military_option_version_id: String,
}

impl TryFrom<MilitaryServiceStartRequest> for StartMilitaryServiceCommand {
    type Error = CareerFailureCode;

    fn try_from(request: MilitaryServiceStartRequest) -> Result<Self, Self::Error> {
        let (command_id, cursor) = career_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        Ok(Self {
            command_id,
            cursor,
            military_option_version_id: ResourceId::parse(&request.military_option_version_id)
                .map_err(|_| CareerFailureCode::InvalidCommand)?,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MilitarySavingsEnrollmentRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    #[schema(maximum = 9007199254740991_u64)]
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    product_version_id: String,
    #[schema(minimum = 1, maximum = 9007199254740991_i64)]
    monthly_contribution_krw: i64,
    #[schema(minimum = 1, maximum = 31)]
    debit_day_of_month: u8,
}

impl TryFrom<MilitarySavingsEnrollmentRequest> for OpenMilitarySavingsCommand {
    type Error = CareerFailureCode;

    fn try_from(request: MilitarySavingsEnrollmentRequest) -> Result<Self, Self::Error> {
        if !(1..=MAX_MILITARY_MONEY_KRW).contains(&request.monthly_contribution_krw)
            || !(1..=31).contains(&request.debit_day_of_month)
        {
            return Err(CareerFailureCode::InvalidCommand);
        }
        let (command_id, cursor) = career_command_parts(
            request.command_id,
            request.expected_run_revision,
            request.expected_state_revision,
            request.expected_game_day,
        )?;
        Ok(Self {
            command_id,
            cursor,
            product_version_id: ResourceId::parse(&request.product_version_id)
                .map_err(|_| CareerFailureCode::InvalidCommand)?,
            monthly_contribution_krw: request.monthly_contribution_krw,
            debit_day_of_month: request.debit_day_of_month,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CareerApplicationRequest {
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 64, max_length = 64, pattern = "^[0-9a-f]{64}$")]
    posting_key: String,
    #[serde(default, deserialize_with = "deserialize_optional_version_id")]
    #[schema(required = false, nullable = false, pattern = "^[1-9][0-9]*$")]
    resume_version_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_version_id")]
    #[schema(required = false, nullable = false, pattern = "^[1-9][0-9]*$")]
    portfolio_version_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_version_id")]
    #[schema(required = false, nullable = false, pattern = "^[1-9][0-9]*$")]
    linkedin_profile_version_id: Option<String>,
}

fn deserialize_optional_version_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom("artifact version ID must be omitted, not null"))
}

impl TryFrom<CareerApplicationRequest> for ApplyCareerCommand {
    type Error = CareerFailureCode;

    fn try_from(request: CareerApplicationRequest) -> Result<Self, Self::Error> {
        if !is_posting_key(&request.posting_key) {
            return Err(CareerFailureCode::InvalidCommand);
        }
        let command_id =
            CommandId::parse(request.command_id).map_err(|_| CareerFailureCode::InvalidCommand)?;
        let versions = [
            request.resume_version_id.as_deref(),
            request.portfolio_version_id.as_deref(),
            request.linkedin_profile_version_id.as_deref(),
        ];
        let parsed = versions
            .into_iter()
            .map(|value| value.map(ResourceId::parse).transpose())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| CareerFailureCode::InvalidCommand)?;
        if parsed.iter().all(Option::is_none) {
            return Err(CareerFailureCode::InvalidCommand);
        }
        let distinct = parsed.iter().flatten().copied().collect::<HashSet<_>>();
        if distinct.len() != parsed.iter().flatten().count() {
            return Err(CareerFailureCode::InvalidCommand);
        }
        Ok(Self {
            command_id,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            posting_key: request.posting_key,
            resume_version_id: parsed[0],
            portfolio_version_id: parsed[1],
            linkedin_profile_version_id: parsed[2],
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CareerInterviewConfirmationRequest {
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    decision: CareerInterviewDecisionRequest,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
enum CareerInterviewDecisionRequest {
    Confirm,
    Decline,
}

impl From<CareerInterviewDecisionRequest> for InterviewDecision {
    fn from(decision: CareerInterviewDecisionRequest) -> Self {
        match decision {
            CareerInterviewDecisionRequest::Confirm => Self::Confirm,
            CareerInterviewDecisionRequest::Decline => Self::Decline,
        }
    }
}

fn career_command_parts(
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
) -> Result<(CommandId, CommandCursor), CareerFailureCode> {
    if expected_state_revision > MAX_JSON_SAFE_INTEGER {
        return Err(CareerFailureCode::InvalidCommand);
    }
    Ok((
        CommandId::parse(command_id).map_err(|_| CareerFailureCode::InvalidCommand)?,
        CommandCursor {
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
        },
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CareerFocusRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 64)]
    focused_job_family_key: String,
}

impl TryFrom<CareerFocusRequest> for FocusCareerCommand {
    type Error = CareerFailureCode;

    fn try_from(request: CareerFocusRequest) -> Result<Self, Self::Error> {
        if request.focused_job_family_key.is_empty()
            || request.focused_job_family_key.len() > 64
            || !request.focused_job_family_key.is_ascii()
        {
            return Err(CareerFailureCode::InvalidCommand);
        }
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| CareerFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            focused_job_family_key: request.focused_job_family_key,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CareerActivityStartRequest {
    #[schema(
        format = "uuid",
        min_length = 36,
        max_length = 36,
        pattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    )]
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")]
    activity_catalog_entry_id: String,
    #[schema(minimum = 1, maximum = 3)]
    priority: u8,
}

impl TryFrom<CareerActivityStartRequest> for StartCareerActivityCommand {
    type Error = CareerFailureCode;

    fn try_from(request: CareerActivityStartRequest) -> Result<Self, Self::Error> {
        if !(1..=3).contains(&request.priority) {
            return Err(CareerFailureCode::InvalidCommand);
        }
        Ok(Self {
            command_id: CommandId::parse(request.command_id)
                .map_err(|_| CareerFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: request.expected_run_revision,
                expected_state_revision: request.expected_state_revision,
                expected_game_day: request.expected_game_day,
            },
            activity_catalog_entry_id: ResourceId::parse(&request.activity_catalog_entry_id)
                .map_err(|_| CareerFailureCode::InvalidCommand)?,
            priority: request.priority,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum CareerArtifactPublishRequest {
    Portfolio {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        headline: String,
        summary: String,
        #[schema(max_items = 12)]
        evidence_ids: Vec<String>,
    },
    Resume {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        headline: String,
        summary: String,
        #[schema(max_items = 40)]
        evidence_ids: Vec<String>,
    },
    LinkedinProfile {
        command_id: String,
        expected_run_revision: u32,
        expected_state_revision: u64,
        expected_game_day: u32,
        headline: String,
        summary: String,
        #[schema(max_items = 30)]
        evidence_ids: Vec<String>,
        open_to_work: bool,
        #[schema(max_items = 3)]
        industries: Vec<CareerIndustryRequest>,
    },
}

impl TryFrom<CareerArtifactPublishRequest> for PublishCareerArtifactCommand {
    type Error = CareerFailureCode;

    fn try_from(request: CareerArtifactPublishRequest) -> Result<Self, Self::Error> {
        let (
            command_id,
            expected_run_revision,
            expected_state_revision,
            expected_game_day,
            kind,
            headline,
            summary,
            raw_evidence_ids,
            linkedin,
        ) = match request {
            CareerArtifactPublishRequest::Portfolio {
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                headline,
                summary,
                evidence_ids,
            } => (
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                ArtifactKind::Portfolio,
                headline,
                summary,
                evidence_ids,
                None,
            ),
            CareerArtifactPublishRequest::Resume {
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                headline,
                summary,
                evidence_ids,
            } => (
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                ArtifactKind::Resume,
                headline,
                summary,
                evidence_ids,
                None,
            ),
            CareerArtifactPublishRequest::LinkedinProfile {
                command_id,
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
                headline,
                summary,
                evidence_ids,
                open_to_work,
                industries,
            } => {
                if industries.len() > 3
                    || industries.iter().copied().collect::<HashSet<_>>().len() != industries.len()
                {
                    return Err(CareerFailureCode::InvalidCommand);
                }
                (
                    command_id,
                    expected_run_revision,
                    expected_state_revision,
                    expected_game_day,
                    ArtifactKind::LinkedinProfile,
                    headline,
                    summary,
                    evidence_ids,
                    Some(LinkedinFields {
                        open_to_work,
                        industries: industries.into_iter().map(Industry::from).collect(),
                    }),
                )
            }
        };
        let evidence_ids = raw_evidence_ids
            .iter()
            .map(|raw| {
                ResourceId::parse(raw)
                    .map(ResourceId::get)
                    .map_err(|_| CareerFailureCode::InvalidCommand)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if evidence_ids.iter().copied().collect::<HashSet<_>>().len() != evidence_ids.len() {
            return Err(CareerFailureCode::InvalidCommand);
        }
        Ok(Self {
            command_id: CommandId::parse(command_id)
                .map_err(|_| CareerFailureCode::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision,
                expected_state_revision,
                expected_game_day,
            },
            draft: ArtifactDraft {
                kind,
                headline,
                summary,
                evidence_ids,
                linkedin,
            },
        })
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct CareerFailure {
    #[schema(value_type = String)]
    code: CareerFailureCode,
    message: &'static str,
}

enum CareerRouteError {
    Rejected(CareerFailureCode),
    Internal(AppError),
}

impl From<CareerFailureCode> for CareerRouteError {
    fn from(code: CareerFailureCode) -> Self {
        Self::Rejected(code)
    }
}

impl From<anyhow::Error> for CareerRouteError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for CareerRouteError {
    fn into_response(self) -> axum::response::Response {
        let code = match self {
            Self::Rejected(code) => code,
            Self::Internal(error) => return error.into_response(),
        };
        let status = if code == CareerFailureCode::InvalidCommand {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::CONFLICT
        };
        (
            status,
            Json(CareerFailure {
                code,
                message: career_failure_message(code),
            }),
        )
            .into_response()
    }
}

const fn career_failure_message(code: CareerFailureCode) -> &'static str {
    match code {
        CareerFailureCode::InvalidCommand => "커리어 요청 형식이 올바르지 않습니다",
        CareerFailureCode::CharacterRequired => "먼저 캐릭터를 생성해야 합니다",
        CareerFailureCode::PolicyUnavailable => "현재 적용할 커리어 제도를 찾을 수 없습니다",
        CareerFailureCode::CatalogUnavailable => "현재 런의 커리어 카탈로그를 찾을 수 없습니다",
        CareerFailureCode::NotEligible => "현재 상태에서는 이 커리어 활동을 할 수 없습니다",
        CareerFailureCode::ActivityLimit => "동시에 진행할 수 있는 활동 한도를 초과했습니다",
        CareerFailureCode::ArtifactRequired => "필요한 커리어 산출물이 없습니다",
        CareerFailureCode::PostingClosed => "채용 공고가 마감되었습니다",
        CareerFailureCode::ApplicationLimit => "지원 한도를 초과했습니다",
        CareerFailureCode::AlreadyApplied => "이미 지원한 공고입니다",
        CareerFailureCode::InterviewExpired => "면접 확인 기한이 지났습니다",
        CareerFailureCode::OfferExpired => "오퍼 응답 기한이 지났습니다",
        CareerFailureCode::AlreadyEmployed => "이미 근로계약이 진행 중입니다",
        CareerFailureCode::MilitaryStateConflict => "현재 병역 상태와 요청이 맞지 않습니다",
        CareerFailureCode::InsufficientWalletCash => "활동 비용을 낼 지갑 현금이 부족합니다",
        CareerFailureCode::LimitExceeded => "허용 한도를 초과했습니다",
        CareerFailureCode::IdempotencyConflict => "같은 명령 ID가 다른 요청에 사용되었습니다",
        CareerFailureCode::SettlementConflict => "이미 처리 중이거나 완료된 커리어 정산입니다",
        CareerFailureCode::Busy => "게임 상태가 변경되었습니다. 최신 상태에서 다시 시도하세요",
    }
}

#[utoipa::path(
    get,
    path = "/api/career/specs",
    params(CareerPageParams),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 focus 점수와 evidence 페이지", body = CareerSpecsResponse),
        (status = 400, description = "페이지 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "커리어 스펙 조회 실패"),
    )
)]
async fn career_specs(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<CareerPageParams>, QueryRejection>,
) -> Result<Json<CareerSpecsResponse>, CareerRouteError> {
    let Query(query) = query.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let query = career_page_query(query.before, query.limit)?;
    Ok(Json(state.career_specs(user.id, query).await?))
}

#[utoipa::path(
    get,
    path = "/api/career/activities",
    params(CareerPageParams),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "활동 카탈로그, active 활동과 이력 페이지", body = CareerActivitiesResponse),
        (status = 400, description = "페이지 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "커리어 활동 조회 실패"),
    )
)]
async fn career_activities(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<CareerPageParams>, QueryRejection>,
) -> Result<Json<CareerActivitiesResponse>, CareerRouteError> {
    let Query(query) = query.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let query = career_page_query(query.before, query.limit)?;
    Ok(Json(state.career_activities(user.id, query).await?))
}

#[utoipa::path(
    get,
    path = "/api/career/artifacts",
    params(CareerArtifactParams),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "불변 커리어 산출물 버전 페이지", body = CareerArtifactsResponse),
        (status = 400, description = "페이지 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "커리어 산출물 조회 실패"),
    )
)]
async fn career_artifacts(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<CareerArtifactParams>, QueryRejection>,
) -> Result<Json<CareerArtifactsResponse>, CareerRouteError> {
    let Query(query) = query.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let page = career_page_query(query.before, query.limit)?;
    let query = CareerArtifactPageQuery {
        kind: query.kind.map(ArtifactKind::from),
        page,
    };
    Ok(Json(state.career_artifacts(user.id, query).await?))
}

#[utoipa::path(
    get,
    path = "/api/career/jobs",
    params(CareerJobsParams),
    security(("sessionCookie" = [])),
    responses((status = 200, body = CareerJobsResponse), (status = 400, body = CareerFailure))
)]
async fn career_jobs(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<CareerJobsParams>, QueryRejection>,
) -> Result<Json<CareerJobsResponse>, CareerRouteError> {
    let Query(query) = query.map_err(|_| CareerFailureCode::InvalidCommand)?;
    Ok(Json(
        state
            .career_jobs(user.id, career_jobs_page_query(query)?)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/api/career/applications",
    params(CareerPageParams),
    security(("sessionCookie" = [])),
    responses((status = 200, body = CareerApplicationsResponse), (status = 400, body = CareerFailure))
)]
async fn career_applications(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<CareerPageParams>, QueryRejection>,
) -> Result<Json<CareerApplicationsResponse>, CareerRouteError> {
    let Query(query) = query.map_err(|_| CareerFailureCode::InvalidCommand)?;
    Ok(Json(
        state
            .career_applications(user.id, career_page_query(query.before, query.limit)?)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/api/career/employment",
    security(("sessionCookie" = [])),
    responses((status = 200, body = CareerEmploymentResponse))
)]
async fn career_employment(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<CareerEmploymentResponse>, CareerRouteError> {
    Ok(Json(state.career_employment(user.id).await?))
}

#[utoipa::path(
    get,
    path = "/api/career/payroll",
    params(CareerPageParams),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "급여 명세 페이지", body = CareerPayrollResponse),
        (status = 400, description = "페이지 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "급여 명세 조회 실패"),
    )
)]
async fn career_payroll(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<CareerPageParams>, QueryRejection>,
) -> Result<Json<CareerPayrollResponse>, CareerRouteError> {
    let Query(query) = query.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let query = career_page_query(query.before, query.limit)?;
    Ok(Json(state.career_payroll(user.id, query).await?))
}

#[utoipa::path(
    get,
    path = "/api/career/tax-years/{year}",
    params(("year" = u16, Path, minimum = 1, maximum = 9999, description = "조회할 근로소득 귀속연도")),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "근로소득 연말정산 상태", body = CareerEmploymentTaxYearSnapshot),
        (status = 400, description = "연도 형식이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "근로소득 연말정산 조회 실패"),
    )
)]
async fn career_employment_tax_year(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(year): Path<String>,
) -> Result<Json<CareerEmploymentTaxYearSnapshot>, CareerRouteError> {
    let year = year
        .parse::<u16>()
        .ok()
        .filter(|year| *year > 0 && *year <= 9999)
        .ok_or(CareerFailureCode::InvalidCommand)?;
    Ok(Json(state.career_employment_tax_year(user.id, year).await?))
}

#[utoipa::path(
    get,
    path = "/api/military/options",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 런에서 선택 가능한 병역 옵션", body = MilitaryOptionsResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "병역 옵션 조회 실패"),
    )
)]
async fn military_options(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<MilitaryOptionsResponse>, CareerRouteError> {
    Ok(Json(state.military_options(user.id).await?))
}

#[utoipa::path(
    get,
    path = "/api/military/service",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 병역 상태와 복무 이력", body = MilitaryServiceResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "복무 상태 조회 실패"),
    )
)]
async fn military_service(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<MilitaryServiceResponse>, CareerRouteError> {
    Ok(Json(state.military_service(user.id).await?))
}

#[utoipa::path(
    get,
    path = "/api/military/savings-products",
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "현재 가입 가능한 장병적금 상품", body = MilitarySavingsProductsResponse),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "장병적금 상품 조회 실패"),
    )
)]
async fn military_savings_products(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<MilitarySavingsProductsResponse>, CareerRouteError> {
    Ok(Json(state.military_savings_products(user.id).await?))
}

#[utoipa::path(
    get,
    path = "/api/military/savings",
    params(CareerPageParams),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "장병적금 계약과 납입 이력 페이지", body = MilitarySavingsHistoryResponse),
        (status = 400, description = "페이지 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 500, description = "장병적금 이력 조회 실패"),
    )
)]
async fn military_savings(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    query: Result<Query<CareerPageParams>, QueryRejection>,
) -> Result<Json<MilitarySavingsHistoryResponse>, CareerRouteError> {
    let Query(query) = query.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let query = career_page_query(query.before, query.limit)?;
    Ok(Json(state.military_savings(user.id, query).await?))
}

#[utoipa::path(
    post,
    path = "/api/career/applications",
    request_body = CareerApplicationRequest,
    security(("sessionCookie" = [])),
    responses((status = 200, body = CareerApplicationResponse), (status = 400, body = CareerFailure), (status = 409, body = CareerFailure))
)]
async fn apply_career(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<CareerApplicationRequest>, JsonRejection>,
) -> Result<Json<CareerApplicationResponse>, CareerRouteError> {
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let command = ApplyCareerCommand::try_from(request)?;
    match state.apply_career(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/career/applications/{id}/interview-confirmation",
    request_body = CareerInterviewConfirmationRequest,
    params(("id" = String, Path, pattern = "^[1-9][0-9]*$")),
    security(("sessionCookie" = [])),
    responses((status = 200, body = CareerApplicationResponse), (status = 400, body = CareerFailure), (status = 409, body = CareerFailure))
)]
async fn confirm_career_interview(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(application_id): Path<String>,
    request: Result<Json<CareerInterviewConfirmationRequest>, JsonRejection>,
) -> Result<Json<CareerApplicationResponse>, CareerRouteError> {
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let application_id =
        ResourceId::parse(&application_id).map_err(|_| CareerFailureCode::InvalidCommand)?;
    let (command_id, cursor) = career_command_parts(
        request.command_id,
        request.expected_run_revision,
        request.expected_state_revision,
        request.expected_game_day,
    )?;
    let command = ConfirmCareerInterviewCommand {
        command_id,
        cursor,
        application_id,
        decision: request.decision.into(),
    };
    match state.confirm_career_interview(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

async fn career_path_command(
    request: Result<Json<CareerCursorRequest>, JsonRejection>,
    path_id: String,
) -> Result<(ResourceId, CommandId, CommandCursor), CareerRouteError> {
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let id = ResourceId::parse(&path_id).map_err(|_| CareerFailureCode::InvalidCommand)?;
    let (command_id, cursor) = request.into_parts()?;
    Ok((id, command_id, cursor))
}

#[utoipa::path(post, path = "/api/career/applications/{id}/withdraw", request_body = CareerCursorRequest, params(("id" = String, Path, pattern = "^[1-9][0-9]*$")), security(("sessionCookie" = [])), responses((status = 200, body = CareerApplicationResponse), (status = 400, body = CareerFailure), (status = 409, body = CareerFailure)))]
async fn withdraw_career_application(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<CareerCursorRequest>, JsonRejection>,
) -> Result<Json<CareerApplicationResponse>, CareerRouteError> {
    let (application_id, command_id, cursor) = career_path_command(request, id).await?;
    let command = WithdrawCareerApplicationCommand {
        command_id,
        cursor,
        application_id,
    };
    match state.withdraw_career_application(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(post, path = "/api/career/invitations/{id}/accept", request_body = CareerCursorRequest, params(("id" = String, Path, pattern = "^[1-9][0-9]*$")), security(("sessionCookie" = [])), responses((status = 200, body = CareerInvitationResponse), (status = 400, body = CareerFailure), (status = 409, body = CareerFailure)))]
async fn accept_career_invitation(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<CareerCursorRequest>, JsonRejection>,
) -> Result<Json<CareerInvitationResponse>, CareerRouteError> {
    let (invitation_id, command_id, cursor) = career_path_command(request, id).await?;
    let command = AcceptCareerInvitationCommand {
        command_id,
        cursor,
        invitation_id,
    };
    match state.accept_career_invitation(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(post, path = "/api/career/invitations/{id}/decline", request_body = CareerCursorRequest, params(("id" = String, Path, pattern = "^[1-9][0-9]*$")), security(("sessionCookie" = [])), responses((status = 200, body = CareerInvitationResponse), (status = 400, body = CareerFailure), (status = 409, body = CareerFailure)))]
async fn decline_career_invitation(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<CareerCursorRequest>, JsonRejection>,
) -> Result<Json<CareerInvitationResponse>, CareerRouteError> {
    let (invitation_id, command_id, cursor) = career_path_command(request, id).await?;
    let command = DeclineCareerInvitationCommand {
        command_id,
        cursor,
        invitation_id,
    };
    match state.decline_career_invitation(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(post, path = "/api/career/offers/{id}/accept", request_body = CareerCursorRequest, params(("id" = String, Path, pattern = "^[1-9][0-9]*$")), security(("sessionCookie" = [])), responses((status = 200, body = CareerOfferResponse), (status = 400, body = CareerFailure), (status = 409, body = CareerFailure)))]
async fn accept_career_offer(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<CareerCursorRequest>, JsonRejection>,
) -> Result<Json<CareerOfferResponse>, CareerRouteError> {
    let (offer_id, command_id, cursor) = career_path_command(request, id).await?;
    let command = AcceptCareerOfferCommand {
        command_id,
        cursor,
        offer_id,
    };
    match state.accept_career_offer(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(post, path = "/api/career/offers/{id}/decline", request_body = CareerCursorRequest, params(("id" = String, Path, pattern = "^[1-9][0-9]*$")), security(("sessionCookie" = [])), responses((status = 200, body = CareerOfferResponse), (status = 400, body = CareerFailure), (status = 409, body = CareerFailure)))]
async fn decline_career_offer(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<CareerCursorRequest>, JsonRejection>,
) -> Result<Json<CareerOfferResponse>, CareerRouteError> {
    let (offer_id, command_id, cursor) = career_path_command(request, id).await?;
    let command = DeclineCareerOfferCommand {
        command_id,
        cursor,
        offer_id,
    };
    match state.decline_career_offer(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/military/service",
    request_body = MilitaryServiceStartRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "복무 시작 예약 또는 멱등 재조회", body = MilitaryServiceCommandResponse),
        (status = 400, description = "복무 시작 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 복무를 시작할 수 없음", body = CareerFailure),
        (status = 500, description = "복무 시작 저장 실패"),
    )
)]
async fn start_military_service(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<MilitaryServiceStartRequest>, JsonRejection>,
) -> Result<Json<MilitaryServiceCommandResponse>, CareerRouteError> {
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let command = StartMilitaryServiceCommand::try_from(request)?;
    match state.start_military_service(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/military/savings",
    request_body = MilitarySavingsEnrollmentRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "장병적금 가입 또는 멱등 재조회", body = MilitarySavingsCommandResponse),
        (status = 400, description = "장병적금 가입 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 장병적금에 가입할 수 없음", body = CareerFailure),
        (status = 500, description = "장병적금 가입 저장 실패"),
    )
)]
async fn open_military_savings(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<MilitarySavingsEnrollmentRequest>, JsonRejection>,
) -> Result<Json<MilitarySavingsCommandResponse>, CareerRouteError> {
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let command = OpenMilitarySavingsCommand::try_from(request)?;
    match state.open_military_savings(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/military/savings/{id}/close",
    request_body = CareerCursorRequest,
    params(("id" = String, Path, min_length = 1, max_length = 20, pattern = "^[1-9][0-9]*$")),
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "장병적금 중도해지 또는 멱등 재조회", body = MilitarySavingsCommandResponse),
        (status = 400, description = "장병적금 중도해지 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 장병적금을 해지할 수 없음", body = CareerFailure),
        (status = 500, description = "장병적금 중도해지 저장 실패"),
    )
)]
async fn close_military_savings(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<CareerCursorRequest>, JsonRejection>,
) -> Result<Json<MilitarySavingsCommandResponse>, CareerRouteError> {
    let contract_id = ResourceId::parse(&id).map_err(|_| CareerFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let (command_id, cursor) = request.into_parts()?;
    let command = CloseMilitarySavingsCommand {
        command_id,
        cursor,
        contract_id,
    };
    match state.close_military_savings(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/career/focus",
    request_body = CareerFocusRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "focus 변경 또는 멱등 재조회", body = CareerFocusResponse),
        (status = 400, description = "focus 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 focus를 변경할 수 없음", body = CareerFailure),
        (status = 500, description = "focus 저장 실패"),
    )
)]
async fn focus_career(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<CareerFocusRequest>, JsonRejection>,
) -> Result<Json<CareerFocusResponse>, CareerRouteError> {
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let command = FocusCareerCommand::try_from(request)?;
    match state.focus_career(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/career/activities",
    request_body = CareerActivityStartRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "활동 시작 또는 멱등 재조회", body = CareerActivityResponse),
        (status = 400, description = "활동 시작 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 활동을 시작할 수 없음", body = CareerFailure),
        (status = 500, description = "활동 시작 저장 실패"),
    )
)]
async fn start_career_activity(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<CareerActivityStartRequest>, JsonRejection>,
) -> Result<Json<CareerActivityResponse>, CareerRouteError> {
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let command = StartCareerActivityCommand::try_from(request)?;
    match state.start_career_activity(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/career/activities/{id}/cancel",
    params(("id" = String, Path, pattern = "^[1-9][0-9]*$")),
    request_body = CareerCursorRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "활동 취소 또는 멱등 재조회", body = CareerActivityResponse),
        (status = 400, description = "활동 취소 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 활동을 취소할 수 없음", body = CareerFailure),
        (status = 500, description = "활동 취소 저장 실패"),
    )
)]
async fn cancel_career_activity(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    request: Result<Json<CareerCursorRequest>, JsonRejection>,
) -> Result<Json<CareerActivityResponse>, CareerRouteError> {
    let activity_id = ResourceId::parse(&id).map_err(|_| CareerFailureCode::InvalidCommand)?;
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let (command_id, cursor) = request.into_parts()?;
    let command = CancelCareerActivityCommand {
        command_id,
        cursor,
        activity_id,
    };
    match state.cancel_career_activity(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[utoipa::path(
    post,
    path = "/api/career/artifacts",
    request_body = CareerArtifactPublishRequest,
    security(("sessionCookie" = [])),
    responses(
        (status = 200, description = "산출물 게시 또는 멱등 재조회", body = CareerArtifactResponse),
        (status = 400, description = "산출물 요청이 잘못됨", body = CareerFailure),
        (status = 401, description = "로그인하지 않음"),
        (status = 409, description = "현재 상태에서 산출물을 게시할 수 없음", body = CareerFailure),
        (status = 500, description = "산출물 저장 실패"),
    )
)]
async fn publish_career_artifact(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    request: Result<Json<CareerArtifactPublishRequest>, JsonRejection>,
) -> Result<Json<CareerArtifactResponse>, CareerRouteError> {
    let Json(request) = request.map_err(|_| CareerFailureCode::InvalidCommand)?;
    let command = PublishCareerArtifactCommand::try_from(request)?;
    match state.publish_career_artifact(user.id, &command).await? {
        CareerCommandResult::Applied(response) => Ok(Json(*response)),
        CareerCommandResult::Rejected(code) => Err(code.into()),
    }
}

#[derive(Debug, Default, Deserialize)]
struct MarketHistoryQuery {
    days: Option<u32>,
}

enum MarketHistoryRouteError {
    InvalidDays,
    Internal(AppError),
}

impl From<anyhow::Error> for MarketHistoryRouteError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for MarketHistoryRouteError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::InvalidDays => (
                StatusCode::BAD_REQUEST,
                Json(GameCommandFailure {
                    code: GameCommandFailureCode::InvalidCommand,
                    message: "조회 기간은 1일 이상 3,660일 이하여야 합니다",
                }),
            )
                .into_response(),
            Self::Internal(error) => error.into_response(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/markets/LLX/history",
    params(
        ("days" = Option<u32>, Query, description = "최근 게임일 수, 기본 365, 최대 3660")
    ),
    responses(
        (status = 200, description = "현재 게임일까지의 LLX 일봉", body = MarketHistoryResponse),
        (status = 400, description = "조회 기간이 허용 범위를 벗어남", body = GameCommandFailure),
        (status = 500, description = "시장 히스토리 조회 실패"),
    )
)]
async fn market_history(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Query(query): Query<MarketHistoryQuery>,
) -> Result<Json<MarketHistoryResponse>, MarketHistoryRouteError> {
    let days = query.days.unwrap_or(DEFAULT_MARKET_HISTORY_DAYS);
    if !(1..=MAX_MARKET_HISTORY_DAYS).contains(&days) {
        return Err(MarketHistoryRouteError::InvalidDays);
    }

    Ok(Json(state.market_history(user.id, days).await?))
}

#[utoipa::path(
    post,
    path = "/api/clock",
    request_body = ClockRequest,
    responses(
        (status = 200, description = "배속 또는 일시정지가 반영된 스냅샷", body = GameSnapshot),
        (status = 409, description = "캐릭터 또는 활성 SSE 연결이 없음", body = GameCommandFailure),
        (status = 422, description = "지원하지 않는 속도"),
        (status = 500, description = "게임 시계 변경 실패"),
    )
)]
async fn clock(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Json(request): Json<ClockRequest>,
) -> Result<Json<GameSnapshot>, GameCommandError> {
    Ok(Json(state.set_clock(user.id, request.speed.0).await?))
}

/// Stream of game-day advances.
///
/// Events are named `tick` and identify durable order as `runRevision:stateRevision`.
/// A reconnecting client sends that value as `Last-Event-ID`, leaving room to replay later.
#[utoipa::path(
    get,
    path = "/api/stream",
    responses(
        (status = 200, description = "현재 상태와 이후 게임 틱", content_type = "text/event-stream"),
        (status = 500, description = "스트림 시작 실패"),
    )
)]
async fn stream(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let (current, receiver, connection) = state.open_stream(user.id).await?.into_parts();

    let updates = BroadcastStream::new(receiver)
        .map_while(|result| match result {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "SSE subscriber lagged; reconnecting from a fresh state"
                );
                None
            }
        })
        .map(|snapshot| Ok(to_event(&snapshot)));

    // Send current state once on connect so the client can draw without a separate fetch
    let initial = tokio_stream::once(Ok(to_event(&current).retry(RETRY_HINT)));

    let stream = ConnectedEventStream {
        inner: Box::pin(initial.chain(updates)),
        _connection: connection,
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(KEEP_ALIVE)))
}

struct ConnectedEventStream {
    inner: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>,
    _connection: StreamConnection,
}

impl Stream for ConnectedEventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().inner.as_mut().poll_next(cx)
    }
}

fn to_event(snapshot: &GameSnapshot) -> Event {
    Event::default()
        .event("tick")
        .id(format!(
            "{}:{}",
            snapshot.run_revision, snapshot.state_revision
        ))
        .json_data(snapshot)
        .unwrap_or_else(|_| Event::default().event("error").data("스냅샷 직렬화 실패"))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_finance_contract_is_generated {
        use super::*;

        fn given_openapi_document() -> serde_json::Value {
            serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize")
        }

        fn when_parameter_is_read<'a>(
            document: &'a serde_json::Value,
            name: &str,
        ) -> &'a serde_json::Value {
            document
                .pointer("/paths/~1api~1finance~1ledger/get/parameters")
                .and_then(serde_json::Value::as_array)
                .and_then(|parameters| {
                    parameters
                        .iter()
                        .find(|parameter| parameter.get("name") == Some(&serde_json::json!(name)))
                })
                .expect("finance ledger parameter must exist")
        }

        #[test]
        fn given_finance_paths_when_read_then_they_require_the_session_cookie() {
            let document = given_openapi_document();

            for operation in [
                "/paths/~1api~1finance~1accounts/get",
                "/paths/~1api~1finance~1accounts/post",
                "/paths/~1api~1finance~1accounts~1{id}~1close/post",
                "/paths/~1api~1finance~1isa~1{id}~1close/post",
                "/paths/~1api~1finance~1pensions~1{id}~1start/post",
                "/paths/~1api~1finance~1pensions~1{id}~1withdrawals/post",
                "/paths/~1api~1finance~1cash-products/get",
                "/paths/~1api~1finance~1deposits/post",
                "/paths/~1api~1finance~1deposits~1{id}~1close/post",
                "/paths/~1api~1finance~1tax-years~1{year}/get",
                "/paths/~1api~1finance~1transfers/post",
                "/paths/~1api~1finance~1ledger/get",
            ] {
                assert_eq!(
                    document.pointer(&format!("{operation}/security")),
                    Some(&serde_json::json!([{ "sessionCookie": [] }]))
                );
                assert!(
                    document
                        .pointer(&format!("{operation}/responses/401"))
                        .is_some()
                );
            }
            assert_eq!(
                document.pointer("/components/securitySchemes/sessionCookie"),
                Some(&serde_json::json!({
                    "type": "apiKey",
                    "in": "cookie",
                    "name": SESSION_COOKIE,
                    "description": "로그인 세션 쿠키"
                }))
            );
        }

        #[test]
        fn given_the_transfer_schema_when_read_then_identifiers_and_amount_are_constrained() {
            let document = given_openapi_document();
            let required = document
                .pointer("/components/schemas/FinanceTransferRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("finance transfer fields must be required");

            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
                "accountId",
                "direction",
                "amountKrw",
            ] {
                assert!(required.contains(&serde_json::json!(field)));
            }
            assert_eq!(
                document.pointer(
                    "/components/schemas/FinanceTransferRequest/properties/commandId/pattern"
                ),
                Some(&serde_json::json!(
                    "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
                ))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/FinanceTransferRequest/properties/commandId/format"
                ),
                Some(&serde_json::json!("uuid"))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/FinanceTransferRequest/properties/accountId/pattern"
                ),
                Some(&serde_json::json!("^[1-9][0-9]*$"))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/FinanceTransferRequest/properties/amountKrw/minimum"
                ),
                Some(&serde_json::json!(1))
            );
        }

        #[test]
        fn given_cash_product_commands_when_read_then_cursor_ids_and_amount_are_constrained() {
            let document = given_openapi_document();
            let cma_required = document
                .pointer("/components/schemas/CmaAccountOpenRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("CMA open fields must be required");
            let deposit_required = document
                .pointer("/components/schemas/DepositOpenRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("deposit open fields must be required");
            let close_required = document
                .pointer("/components/schemas/FinanceCursorCommandRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("close-command cursor fields must be required");

            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
            ] {
                assert!(cma_required.contains(&serde_json::json!(field)));
                assert!(deposit_required.contains(&serde_json::json!(field)));
                assert!(close_required.contains(&serde_json::json!(field)));
            }
            for field in [
                "kind",
                "productVersionId",
                "settlementAccountId",
                "amountKrw",
            ] {
                assert!(deposit_required.contains(&serde_json::json!(field)));
            }
            assert_eq!(
                document
                    .pointer("/components/schemas/DepositOpenRequest/properties/amountKrw/minimum"),
                Some(&serde_json::json!(1))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/CmaAccountOpenRequest/properties/productVersionId/pattern"
                ),
                Some(&serde_json::json!("^[1-9][0-9]*$"))
            );
            for path in [
                "/paths/~1api~1finance~1accounts~1{id}~1close/post/parameters/0/schema/pattern",
                "/paths/~1api~1finance~1deposits~1{id}~1close/post/parameters/0/schema/pattern",
            ] {
                assert_eq!(
                    document.pointer(path),
                    Some(&serde_json::json!("^[1-9][0-9]*$"))
                );
            }
        }

        #[test]
        fn given_tax_account_commands_when_read_then_variants_ids_and_limits_are_constrained() {
            let document = given_openapi_document();
            let tax_open_required = document
                .pointer("/components/schemas/TaxAccountOpenRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("tax-account open fields must be required");
            let pension_start_required = document
                .pointer("/components/schemas/PensionStartRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("pension start fields must be required");
            let withdrawal_required = document
                .pointer("/components/schemas/PensionWithdrawalRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("pension withdrawal fields must be required");

            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
            ] {
                assert!(tax_open_required.contains(&serde_json::json!(field)));
                assert!(pension_start_required.contains(&serde_json::json!(field)));
                assert!(withdrawal_required.contains(&serde_json::json!(field)));
            }
            assert!(tax_open_required.contains(&serde_json::json!("type")));
            for field in ["paymentYears", "lifetime"] {
                assert!(pension_start_required.contains(&serde_json::json!(field)));
            }
            for field in ["amountKrw", "type", "reason"] {
                assert!(withdrawal_required.contains(&serde_json::json!(field)));
            }
            assert_eq!(
                document.pointer(
                    "/components/schemas/PensionStartRequest/properties/paymentYears/minimum"
                ),
                Some(&serde_json::json!(5))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/PensionStartRequest/properties/paymentYears/maximum"
                ),
                Some(&serde_json::json!(100))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/PensionWithdrawalRequest/properties/amountKrw/minimum"
                ),
                Some(&serde_json::json!(1))
            );
            for path in [
                "/paths/~1api~1finance~1isa~1{id}~1close/post/parameters/0/schema/pattern",
                "/paths/~1api~1finance~1pensions~1{id}~1start/post/parameters/0/schema/pattern",
                "/paths/~1api~1finance~1pensions~1{id}~1withdrawals/post/parameters/0/schema/pattern",
            ] {
                assert_eq!(
                    document.pointer(path),
                    Some(&serde_json::json!("^[1-9][0-9]*$"))
                );
            }
            assert_eq!(
                document
                    .pointer("/components/schemas/FinanceAccountOpenRequest/oneOf")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len),
                Some(3)
            );
            assert_eq!(
                document
                    .pointer("/components/schemas/FinanceAccountOpenResponse/oneOf")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len),
                Some(3)
            );
        }

        #[test]
        fn given_finance_enums_when_read_then_the_wire_values_are_fixed() {
            let document = given_openapi_document();

            assert_eq!(
                document.pointer("/components/schemas/TransferDirection/enum"),
                Some(&serde_json::json!(["walletToAccount", "accountToWallet"]))
            );
            assert_eq!(
                document.pointer("/components/schemas/DepositKindRequest/enum"),
                Some(&serde_json::json!(["termDeposit", "installmentSavings"]))
            );
            assert_eq!(
                document.pointer("/components/schemas/TaxAccountOpenType/enum"),
                Some(&serde_json::json!([
                    "isaGeneral",
                    "isaLowIncome",
                    "pensionSavings",
                    "irp"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/PensionWithdrawalRequestKind/enum"),
                Some(&serde_json::json!(["pension", "unavoidable", "nonPension"]))
            );
            assert_eq!(
                document.pointer("/components/schemas/IrpWithdrawalReason/enum"),
                Some(&serde_json::json!([
                    "homePurchase",
                    "housingDeposit",
                    "medicalCare",
                    "disaster",
                    "bankruptcy",
                    "rehabilitation",
                    "securedLoanRepayment"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/CashProductKind/enum"),
                Some(&serde_json::json!([
                    "cmaRp",
                    "cmaIssuedNote",
                    "termDeposit",
                    "installmentSavings"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/FinanceFailureCode/enum"),
                Some(&serde_json::json!([
                    "invalidCommand",
                    "characterRequired",
                    "accountNotFound",
                    "accountClosed",
                    "accountTypeNotAllowed",
                    "accountNotEmpty",
                    "accountAlreadyExists",
                    "insufficientWalletCash",
                    "insufficientAccountCash",
                    "policyNotEligible",
                    "limitExceeded",
                    "settlementConflict",
                    "idempotencyConflict",
                    "busy",
                    "productNotFound",
                    "contractNotFound",
                    "contractClosed",
                    "rateUnavailable",
                    "marketClosed",
                    "insufficientQuantity",
                    "positionLimit"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/FinancialAccountStatus/enum"),
                Some(&serde_json::json!(["open", "matured", "closed"]))
            );
            assert_eq!(
                document.pointer("/components/schemas/FinancialAccountType/enum"),
                Some(&serde_json::json!([
                    "taxableBrokerage",
                    "cma",
                    "isaGeneral",
                    "isaLowIncome",
                    "pensionSavings",
                    "irp",
                    "krxGold"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/SettlementKind/enum"),
                Some(&serde_json::json!([
                    "cmaInterest",
                    "depositMaturity",
                    "savingsInstallment",
                    "savingsMaturity",
                    "bondCoupon",
                    "bondMaturity",
                    "llxDistribution",
                    "financialIncomeFiling",
                    "employmentPayroll",
                    "employmentReconciliation",
                    "militaryPay",
                    "militarySavingsInstallment",
                    "militarySavingsMaturity",
                    "militarySavingsGovernmentMatch",
                    "loanInstallment",
                    "leaseRent",
                    "livingCostMonth",
                    "propertyTaxPayment",
                    "welfareBenefitPayment",
                    "insurancePremium"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/LedgerSourceKind/enum"),
                Some(&serde_json::json!([
                    "m2OpeningBalance",
                    "transfer",
                    "trade",
                    "cashProductEnrollment",
                    "cashProductClose",
                    "isaClose",
                    "pensionWithdrawal",
                    "interestAccrual",
                    "scheduledSettlement",
                    "specActivity",
                    "employmentPayroll",
                    "careerRewardPayment",
                    "pensionCreditAllocation",
                    "militaryPay",
                    "militarySavingsInstallment",
                    "militarySavingsMaturity",
                    "militarySavingsGovernmentMatch",
                    "militarySavingsEarlyClose",
                    "livingCostMonth",
                    "essentialArrearPayment",
                    "loanOrigination",
                    "loanInstallment",
                    "loanPrepayment",
                    "debtAuthorityBridge",
                    "leaseMove",
                    "leaseRent",
                    "leaseArrearPayment",
                    "propertyPurchase",
                    "propertySale",
                    "propertyTaxPayment",
                    "welfareBenefitPayment",
                    "lifeEventChoice",
                    "insurancePremiumPayment",
                    "insuranceClaimPayment",
                    "insolvencyDistribution",
                    "insolvencyDischarge",
                    "corporationEstablishment",
                    "corporationOfficerPayroll",
                    "corporationDividend",
                    "correction"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/LedgerAccountCode/enum"),
                Some(&serde_json::json!([
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
                    "careerDevelopmentExpense",
                    "salaryIncome",
                    "employeeNationalPensionExpense",
                    "employeeHealthInsuranceExpense",
                    "employeeLongTermCareExpense",
                    "employeeEmploymentInsuranceExpense",
                    "employmentIncomeTaxWithholding",
                    "employmentLocalIncomeTaxWithholding",
                    "otherIncomeReward",
                    "otherIncomeTaxWithholding",
                    "otherLocalIncomeTaxWithholding",
                    "pensionTaxExcludedContribution",
                    "pensionCreditedContribution",
                    "militaryPayIncome",
                    "militarySavingsPrincipal",
                    "militarySavingsBankInterest",
                    "militarySavingsGovernmentMatchIncome",
                    "livingCostExpense",
                    "essentialArrearLiability",
                    "loanPrincipalLiability",
                    "loanInterestExpense",
                    "loanInterestLiability",
                    "loanFeeExpense",
                    "taxObligationLiability",
                    "leaseDepositAsset",
                    "movingExpense",
                    "leaseRentExpense",
                    "leaseArrearLiability",
                    "propertyAsset",
                    "acquisitionIncidentalExpense",
                    "propertyDispositionExpense",
                    "propertyTaxExpense",
                    "welfareBenefitIncome",
                    "lifeEventExpense",
                    "insurancePremiumExpense",
                    "insuranceClaimRecovery",
                    "insolvencyDischargedDebt",
                    "insolvencyDischargeGain",
                    "corporationInvestmentAsset",
                    "corporationRegistrationExpense"
                ]))
            );
        }

        #[test]
        fn given_the_ledger_query_when_read_then_cursor_and_page_size_are_optional_and_bounded() {
            let document = given_openapi_document();
            let before = when_parameter_is_read(&document, "before");
            let limit = when_parameter_is_read(&document, "limit");

            assert_ne!(before.get("required"), Some(&serde_json::json!(true)));
            assert_eq!(
                before.pointer("/schema/pattern"),
                Some(&serde_json::json!("^[1-9][0-9]*$"))
            );
            assert_ne!(limit.get("required"), Some(&serde_json::json!(true)));
            assert_eq!(
                limit.pointer("/schema/default"),
                Some(&serde_json::json!(50))
            );
            assert_eq!(
                limit.pointer("/schema/minimum"),
                Some(&serde_json::json!(1))
            );
            assert_eq!(
                limit.pointer("/schema/maximum"),
                Some(&serde_json::json!(200))
            );
        }

        #[test]
        fn given_finance_responses_when_read_then_nullable_fields_and_array_bounds_are_fixed() {
            let document = given_openapi_document();
            let ledger_required = document
                .pointer("/components/schemas/LedgerPageResponse/required")
                .and_then(serde_json::Value::as_array)
                .expect("ledger response fields must be required");
            let posting_required = document
                .pointer("/components/schemas/LedgerPostingSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("ledger posting fields must be required");

            assert!(ledger_required.contains(&serde_json::json!("nextBefore")));
            assert!(posting_required.contains(&serde_json::json!("accountId")));
            for pointer in [
                "/components/schemas/LedgerPageResponse/properties/nextBefore/type",
                "/components/schemas/LedgerPostingSnapshot/properties/accountId/type",
            ] {
                let types = document
                    .pointer(pointer)
                    .and_then(serde_json::Value::as_array)
                    .expect("nullable resource ID must have a type union");
                assert!(types.contains(&serde_json::json!("string")));
                assert!(types.contains(&serde_json::json!("null")));
            }
            assert_eq!(
                document.pointer(
                    "/components/schemas/FinanceSnapshot/properties/pendingSettlements/maxItems"
                ),
                Some(&serde_json::json!(20))
            );
            for (field, maximum) in [
                ("accounts", 32),
                ("cmaAccounts", 32),
                ("cashContracts", 100),
                ("depositProtection", 16),
                ("isaAccounts", 1),
                ("pensionAccounts", 2),
            ] {
                assert_eq!(
                    document.pointer(&format!(
                        "/components/schemas/FinanceSnapshot/properties/{field}/maxItems"
                    )),
                    Some(&serde_json::json!(maximum))
                );
            }
            assert_eq!(
                document.pointer(
                    "/components/schemas/LedgerPageResponse/properties/transactions/maxItems"
                ),
                Some(&serde_json::json!(200))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LedgerTransactionSnapshot/properties/postings/minItems"
                ),
                Some(&serde_json::json!(2))
            );
        }

        #[test]
        fn given_invalid_finance_input_when_documented_then_it_uses_the_fixed_failure_schema() {
            let document = given_openapi_document();

            for operation in [
                "/paths/~1api~1finance~1accounts/post",
                "/paths/~1api~1finance~1accounts~1{id}~1close/post",
                "/paths/~1api~1finance~1isa~1{id}~1close/post",
                "/paths/~1api~1finance~1pensions~1{id}~1start/post",
                "/paths/~1api~1finance~1pensions~1{id}~1withdrawals/post",
                "/paths/~1api~1finance~1deposits/post",
                "/paths/~1api~1finance~1deposits~1{id}~1close/post",
                "/paths/~1api~1finance~1transfers/post",
            ] {
                assert_eq!(
                    document.pointer(&format!(
                        "{operation}/responses/400/content/application~1json/schema/$ref"
                    )),
                    Some(&serde_json::json!("#/components/schemas/FinanceFailure"))
                );
                assert!(
                    document
                        .pointer(&format!("{operation}/responses/422"))
                        .is_none()
                );
            }
            assert_eq!(
                document.pointer("/components/schemas/FinanceFailure/properties/code/$ref"),
                Some(&serde_json::json!(
                    "#/components/schemas/FinanceFailureCode"
                ))
            );
        }

        #[test]
        fn given_account_open_variants_when_parsed_then_only_supported_shapes_are_accepted() {
            let command = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3
            });
            let mut cma = command.clone();
            cma.as_object_mut()
                .expect("test command must be an object")
                .extend([
                    ("type".to_owned(), serde_json::json!("cma")),
                    ("productVersionId".to_owned(), serde_json::json!("1")),
                ]);
            let mut isa = command.clone();
            isa.as_object_mut()
                .expect("test command must be an object")
                .insert("type".to_owned(), serde_json::json!("isaGeneral"));
            let mut unsupported = command;
            unsupported
                .as_object_mut()
                .expect("test command must be an object")
                .insert("type".to_owned(), serde_json::json!("taxableBrokerage"));

            let cma = serde_json::from_value::<FinanceAccountOpenRequest>(cma);
            let isa = serde_json::from_value::<FinanceAccountOpenRequest>(isa);
            let unsupported = serde_json::from_value::<FinanceAccountOpenRequest>(unsupported);

            assert!(matches!(cma, Ok(FinanceAccountOpenRequest::Cma(_))));
            assert!(matches!(isa, Ok(FinanceAccountOpenRequest::Tax(_))));
            assert!(unsupported.is_err());
        }

        #[test]
        fn given_pension_withdrawal_when_reason_is_missing_then_the_request_is_rejected() {
            let base = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "amountKrw": 10000,
                "type": "pension",
                "reason": null
            });
            let mut missing_reason = base.clone();
            missing_reason
                .as_object_mut()
                .expect("test request must be an object")
                .remove("reason");

            let explicit_null = serde_json::from_value::<PensionWithdrawalRequest>(base);
            let missing = serde_json::from_value::<PensionWithdrawalRequest>(missing_reason);

            assert!(explicit_null.is_ok());
            assert!(missing.is_err());
        }

        #[test]
        fn given_pension_start_outside_the_payment_range_when_converted_then_store_validation_is_preserved()
         {
            let request = serde_json::from_value::<PensionStartRequest>(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "paymentYears": 4,
                "lifetime": false
            }))
            .expect("the request shape is valid before semantic store validation");

            let result = request.into_command(ResourceId::parse("1").expect("valid resource ID"));

            assert_eq!(
                result
                    .expect("the store must receive fingerprintable semantic values")
                    .payment_years,
                4
            );
        }
    }

    mod context_clock_contract_is_generated {
        use super::*;

        #[test]
        fn given_the_openapi_document_when_read_then_speeds_are_the_exact_numeric_enum() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize");

            assert_eq!(
                document.pointer("/components/schemas/AutoSpeed/enum"),
                Some(&serde_json::json!([1, 2, 4, 8]))
            );
        }

        #[test]
        fn given_the_openapi_document_when_read_then_clock_speed_is_a_required_field() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize");

            assert_eq!(
                document.pointer("/components/schemas/ClockRequest/required"),
                Some(&serde_json::json!(["speed"]))
            );
            assert_eq!(
                document.pointer("/components/schemas/ClockSetting/oneOf/0/type"),
                Some(&serde_json::json!("null"))
            );
            let snapshot_required = document
                .pointer("/components/schemas/GameSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("GameSnapshot required fields must be listed");
            assert!(snapshot_required.contains(&serde_json::json!("runRevision")));
            assert!(snapshot_required.contains(&serde_json::json!("stateRevision")));
            assert!(snapshot_required.contains(&serde_json::json!("characterName")));
            assert!(snapshot_required.contains(&serde_json::json!("autoSpeed")));
            assert!(snapshot_required.contains(&serde_json::json!("market")));
            assert!(snapshot_required.contains(&serde_json::json!("portfolio")));
            assert!(
                document
                    .pointer("/components/schemas/MarketSnapshot/required")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|required| {
                        required.contains(&serde_json::json!("world"))
                            && required.contains(&serde_json::json!("date"))
                            && required.contains(&serde_json::json!("open"))
                            && required.contains(&serde_json::json!("regime"))
                            && required.contains(&serde_json::json!("index"))
                            && required.contains(&serde_json::json!("rates"))
                    })
            );
        }
    }

    mod context_run_start_contract_is_parsed {
        use super::*;

        fn given_sandbox_request() -> serde_json::Value {
            serde_json::json!({
                "mode": "sandbox",
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 0,
                "expectedStateRevision": 0,
                "expectedGameDay": 0,
                "character": {
                    "name": "샌드박스",
                    "age": 25,
                    "gender": "other",
                    "military": "exempted",
                    "region": "capitalArea",
                    "background": "independent",
                    "education": "bachelor",
                    "careerYears": 1,
                    "certifications": 1,
                    "startingCashKrw": 10000000,
                    "health": "normal",
                    "dependents": 0
                },
                "startingLoans": []
            })
        }

        #[test]
        fn given_strict_sandbox_when_converted_then_explicit_manifest_kind_is_preserved() {
            let request = serde_json::from_value::<RunStartRequest>(given_sandbox_request())
                .expect("sandbox 요청 문법이 유효해야 한다");

            let action = request
                .into_action()
                .expect("sandbox 명령으로 변환되어야 한다");

            assert!(matches!(
                action,
                RunStartAction::Start(command)
                    if matches!(
                        command.manifest_kind,
                        StartGameManifestKind::Sandbox
                    )
            ));
        }

        #[test]
        fn given_sandbox_with_ranked_field_when_parsed_then_unknown_field_is_rejected() {
            let mut request = given_sandbox_request();
            request
                .as_object_mut()
                .expect("sandbox 요청은 객체여야 한다")
                .insert("pointBudgetVersionId".to_owned(), serde_json::json!("1"));

            let parsed = serde_json::from_value::<RunStartRequest>(request);

            assert!(parsed.is_err());
        }

        #[test]
        fn given_valid_ranked_shape_when_converted_then_ranked_selection_is_preserved() {
            let request = serde_json::from_value::<RunStartRequest>(serde_json::json!({
                "mode": "rankedCustom",
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 0,
                "expectedStateRevision": 0,
                "expectedGameDay": 0,
                "pointBudgetVersionId": "1",
                "selections": [{"optionId": "1", "quantity": 1}]
            }))
            .expect("ranked custom 요청 문법이 유효해야 한다");

            let action = request.into_action().expect("구조 검증은 통과해야 한다");

            assert!(matches!(
                action,
                RunStartAction::RankedCustom {
                    budget_version_id,
                    selections,
                    ..
                } if budget_version_id == ResourceId::from_u64(1)
                    && selections == vec![PointSelection {
                        option_id: ResourceId::from_u64(1),
                        quantity: 1,
                    }]
            ));
        }

        #[test]
        fn given_run_start_openapi_when_read_then_path_and_session_security_are_fixed() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI 문서를 직렬화해야 한다");

            assert_eq!(
                document.pointer("/paths/~1api~1runs/post/security/0/sessionCookie"),
                Some(&serde_json::json!([]))
            );
            assert_eq!(
                document.pointer(
                    "/paths/~1api~1runs/post/responses/200/content/application~1json/schema/$ref"
                ),
                Some(&serde_json::json!("#/components/schemas/RunStartResponse"))
            );
        }
    }

    mod context_durable_game_command_contract_is_generated {
        use super::*;

        fn given_openapi_document() -> serde_json::Value {
            serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize")
        }

        fn required_fields<'a>(
            document: &'a serde_json::Value,
            schema: &str,
        ) -> &'a Vec<serde_json::Value> {
            document
                .pointer(&format!("/components/schemas/{schema}/required"))
                .and_then(serde_json::Value::as_array)
                .expect("command schema must list required fields")
        }

        fn given_v2_start_request() -> serde_json::Value {
            serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 0,
                "expectedStateRevision": 0,
                "expectedGameDay": 0,
                "character": {
                    "name": "테스터",
                    "age": 25,
                    "gender": "other",
                    "military": "exempted",
                    "region": "capitalArea",
                    "background": "independent",
                    "education": "bachelor",
                    "careerYears": 1,
                    "certifications": 1,
                    "startingCashKrw": 10000000,
                    "health": "normal",
                    "dependents": 0
                },
                "startingLoans": [
                    {
                        "kind": "studentLoan",
                        "productVersionId": "11",
                        "principalKrw": 20000000
                    },
                    {
                        "kind": "unsecuredLoan",
                        "productVersionId": "12",
                        "principalKrw": 3000000
                    }
                ]
            })
        }

        #[test]
        fn given_start_and_advance_requests_when_read_then_every_command_cursor_field_is_required()
        {
            let document = given_openapi_document();

            let start_v1 = required_fields(&document, "CharacterStartV1Request");
            let start_v2 = required_fields(&document, "CharacterStartV2Request");
            let advance = required_fields(&document, "AdvanceRequest");

            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
            ] {
                assert!(start_v1.contains(&serde_json::json!(field)));
                assert!(start_v2.contains(&serde_json::json!(field)));
                assert!(advance.contains(&serde_json::json!(field)));
            }
            assert!(start_v1.contains(&serde_json::json!("character")));
            assert!(start_v2.contains(&serde_json::json!("character")));
            assert!(start_v2.contains(&serde_json::json!("startingLoans")));
            assert!(advance.contains(&serde_json::json!("days")));
            assert_eq!(
                document
                    .pointer("/components/schemas/CharacterStartRequest/oneOf")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::len),
                Some(2)
            );
            assert_eq!(
                document.pointer("/components/schemas/AdvanceRequest/properties/days/minimum"),
                Some(&serde_json::json!(1))
            );
            assert_eq!(
                document.pointer("/components/schemas/AdvanceRequest/properties/days/maximum"),
                Some(&serde_json::json!(30))
            );
        }

        #[test]
        fn given_command_ids_when_read_then_canonical_lowercase_uuid_is_documented_everywhere() {
            let document = given_openapi_document();
            let expected_pattern =
                serde_json::json!("^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$");

            for pointer in [
                "/components/schemas/CharacterStartV1Request/properties/commandId",
                "/components/schemas/CharacterStartV2Request/properties/commandId",
                "/components/schemas/AdvanceRequest/properties/commandId",
                "/components/schemas/CharacterStartSnapshot/properties/commandId",
                "/components/schemas/AdvanceCommandSnapshot/properties/commandId",
            ] {
                assert_eq!(
                    document.pointer(&format!("{pointer}/minLength")),
                    Some(&serde_json::json!(36))
                );
                assert_eq!(
                    document.pointer(&format!("{pointer}/maxLength")),
                    Some(&serde_json::json!(36))
                );
                assert_eq!(
                    document.pointer(&format!("{pointer}/pattern")),
                    Some(&expected_pattern)
                );
            }
        }

        #[test]
        fn given_command_responses_when_read_then_result_and_cursor_fields_are_required() {
            let document = given_openapi_document();
            let cursor = required_fields(&document, "GameCommandCursorSnapshot");
            let start = required_fields(&document, "CharacterStartSnapshot");
            let advance = required_fields(&document, "AdvanceCommandSnapshot");

            for field in ["runRevision", "stateRevision", "gameDay"] {
                assert!(cursor.contains(&serde_json::json!(field)));
            }
            for field in ["commandId", "committedCursor", "replayed"] {
                assert!(start.contains(&serde_json::json!(field)));
            }
            for field in [
                "commandId",
                "requestedDays",
                "initialCursor",
                "committedCursor",
                "replayed",
            ] {
                assert!(advance.contains(&serde_json::json!(field)));
            }
            assert_eq!(
                document.pointer(
                    "/paths/~1api~1characters/post/responses/200/content/application~1json/schema/$ref"
                ),
                Some(&serde_json::json!(
                    "#/components/schemas/CharacterStartResponse"
                ))
            );
            assert_eq!(
                document.pointer(
                    "/paths/~1api~1advance/post/responses/200/content/application~1json/schema/$ref"
                ),
                Some(&serde_json::json!("#/components/schemas/AdvanceResponse"))
            );
        }

        #[test]
        fn given_unknown_wrapper_or_character_fields_when_parsed_then_the_command_is_rejected() {
            let base = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 0,
                "expectedStateRevision": 0,
                "expectedGameDay": 0,
                "character": {
                    "name": "테스터",
                    "age": 25,
                    "gender": "other",
                    "military": "exempted",
                    "region": "capitalArea",
                    "background": "independent",
                    "education": "bachelor",
                    "careerYears": 1,
                    "certifications": 1,
                    "startingCashKrw": 10000000,
                    "studentLoanKrw": 0,
                    "creditLoanKrw": 0,
                    "health": "normal",
                    "dependents": 0
                }
            });
            let mut wrapper_changed = base.clone();
            wrapper_changed
                .as_object_mut()
                .expect("테스트 요청은 객체여야 한다")
                .insert("unexpected".to_owned(), serde_json::json!(true));
            let mut character_changed = base;
            character_changed
                .pointer_mut("/character")
                .and_then(serde_json::Value::as_object_mut)
                .expect("테스트 캐릭터는 객체여야 한다")
                .insert("unexpected".to_owned(), serde_json::json!(true));

            let wrapper = serde_json::from_value::<CharacterStartRequest>(wrapper_changed);
            let character = serde_json::from_value::<CharacterStartRequest>(character_changed);

            assert!(wrapper.is_err());
            assert!(character.is_err());
        }

        #[test]
        fn given_a_semantically_invalid_character_when_converted_then_store_validation_is_preserved()
         {
            let request = serde_json::from_value::<CharacterStartRequest>(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 0,
                "expectedStateRevision": 0,
                "expectedGameDay": 0,
                "character": {
                    "name": "",
                    "age": 18,
                    "gender": "other",
                    "military": "exempted",
                    "region": "capitalArea",
                    "background": "independent",
                    "education": "bachelor",
                    "careerYears": 1,
                    "certifications": 1,
                    "startingCashKrw": 10000000,
                    "studentLoanKrw": 0,
                    "creditLoanKrw": 0,
                    "health": "normal",
                    "dependents": 0
                }
            }))
            .expect("요청 문법은 유효해야 한다");

            let command = request
                .into_command()
                .expect("fingerprint 가능한 의미값은 저장소까지 전달되어야 한다");

            assert_eq!(command.draft.name, "");
            assert_eq!(command.draft.age, 18);
        }

        #[test]
        fn given_v2_시작대출_when_명령으로변환하면_then_상품과원금을보존한다() {
            let request = serde_json::from_value::<CharacterStartRequest>(given_v2_start_request())
                .expect("v2 시작 요청은 유효해야 한다");

            let command = request
                .into_command()
                .expect("v2 시작 명령으로 변환되어야 한다");

            assert_eq!(command.draft.student_loan_krw, 20_000_000);
            assert_eq!(command.draft.credit_loan_krw, 3_000_000);
            let loans = command
                .starting_loans
                .expect("v2 명령은 상품 선택을 보존해야 한다");
            assert_eq!(loans.len(), 2);
            assert_eq!(loans[0].product_kind, LoanProductKind::StudentLoan);
            assert_eq!(loans[0].product_version_id.get(), 11);
            assert_eq!(loans[1].product_kind, LoanProductKind::UnsecuredLoan);
            assert_eq!(loans[1].product_version_id.get(), 12);
        }

        #[test]
        fn given_v1금액과_v2대출을섞음_when_parse하면_then_거절한다() {
            let mut request = given_v2_start_request();
            request
                .pointer_mut("/character")
                .and_then(serde_json::Value::as_object_mut)
                .expect("캐릭터 요청은 객체여야 한다")
                .insert("studentLoanKrw".to_owned(), serde_json::json!(20_000_000));

            let result = serde_json::from_value::<CharacterStartRequest>(request);

            assert!(result.is_err());
        }

        #[test]
        fn given_v2대출이_canonical순서가아님_when_명령으로변환하면_then_거절한다() {
            let mut request = given_v2_start_request();
            request
                .pointer_mut("/startingLoans")
                .and_then(serde_json::Value::as_array_mut)
                .expect("시작 대출은 배열이어야 한다")
                .reverse();
            let parsed = serde_json::from_value::<CharacterStartRequest>(request)
                .expect("문법 형태는 유효해야 한다");

            let result = parsed.into_command();

            assert!(matches!(result, Err(GameLoopError::InvalidCommand)));
        }

        #[test]
        fn given_v2대출에_unknown필드_when_parse하면_then_거절한다() {
            let mut request = given_v2_start_request();
            request
                .pointer_mut("/startingLoans/0")
                .and_then(serde_json::Value::as_object_mut)
                .expect("시작 대출은 객체여야 한다")
                .insert("unexpected".to_owned(), serde_json::json!(true));

            let result = serde_json::from_value::<CharacterStartRequest>(request);

            assert!(result.is_err());
        }
    }

    mod context_market_trading_contract_is_generated {
        use super::*;

        #[test]
        fn given_the_openapi_document_when_read_then_order_cursor_fields_are_required() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize");
            let required = document
                .pointer("/components/schemas/TradeOrderRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("TradeOrderRequest required fields must be listed");

            for field in [
                "orderId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
                "side",
                "symbol",
                "quantity",
            ] {
                assert!(required.contains(&serde_json::json!(field)));
            }
        }

        #[test]
        fn given_the_openapi_document_when_read_then_order_and_history_paths_are_present() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize");

            assert!(
                document
                    .pointer("/paths/~1api~1portfolio~1orders/post")
                    .is_some()
            );
            assert!(
                document
                    .pointer("/paths/~1api~1markets~1LLX~1history/get")
                    .is_some()
            );
        }
    }

    mod context_커리어_protocol을_검증하는_경우 {
        use super::*;

        fn given_linked_in_request() -> serde_json::Value {
            serde_json::json!({
                "kind": "linkedinProfile",
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "headline": "개발자",
                "summary": "문제 해결 경험",
                "evidenceIds": ["1", "2"],
                "openToWork": true,
                "industries": ["itSoftware"]
            })
        }

        #[test]
        fn given_linked_in_exact_object_when_변환하면_then_tagged_fields를_보존한다() {
            let request =
                serde_json::from_value::<CareerArtifactPublishRequest>(given_linked_in_request())
                    .expect("LinkedIn 요청 문법이 유효해야 한다");

            let command = PublishCareerArtifactCommand::try_from(request)
                .expect("LinkedIn 요청을 명령으로 바꿀 수 있어야 한다");

            assert_eq!(command.draft.kind, ArtifactKind::LinkedinProfile);
            assert_eq!(command.draft.evidence_ids, vec![1, 2]);
            assert_eq!(
                command
                    .draft
                    .linkedin
                    .expect("LinkedIn 전용 필드가 있어야 한다")
                    .industries,
                vec![Industry::ItSoftware]
            );
        }

        #[test]
        fn given_portfolio에_linked_in_전용필드_when_parse하면_then_unknown_field로_거절한다() {
            let request = serde_json::json!({
                "kind": "portfolio",
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "headline": "포트폴리오",
                "summary": "",
                "evidenceIds": [],
                "openToWork": true
            });

            let result = serde_json::from_value::<CareerArtifactPublishRequest>(request);

            assert!(result.is_err());
        }

        #[test]
        fn given_중복_evidence_id_when_명령으로_변환하면_then_invalid_command로_거절한다() {
            let mut request = given_linked_in_request();
            request["evidenceIds"] = serde_json::json!(["1", "1"]);
            let request = serde_json::from_value::<CareerArtifactPublishRequest>(request)
                .expect("요청 문법은 유효해야 한다");

            let result = PublishCareerArtifactCommand::try_from(request);

            assert_eq!(result, Err(CareerFailureCode::InvalidCommand));
        }

        #[test]
        fn given_명시적_null_artifact_version_when_parse하면_then_요청을_거절한다() {
            let request = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "postingKey": "a".repeat(64),
                "resumeVersionId": null,
                "portfolioVersionId": "1"
            });

            let result = serde_json::from_value::<CareerApplicationRequest>(request);

            assert!(result.is_err());
        }

        #[test]
        fn given_생략한_optional_artifact_version_when_parse하면_then_요청을_허용한다() {
            let request = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "postingKey": "a".repeat(64),
                "portfolioVersionId": "1"
            });

            let result = serde_json::from_value::<CareerApplicationRequest>(request);

            assert!(result.is_ok());
        }

        #[test]
        fn given_optional_artifact_version_when_openapi를_읽으면_then_생략만_허용한다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let required = document
                .pointer("/components/schemas/CareerApplicationRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("지원 요청의 필수 필드 목록이 있어야 한다");

            for field in [
                "resumeVersionId",
                "portfolioVersionId",
                "linkedinProfileVersionId",
            ] {
                assert!(!required.contains(&serde_json::json!(field)));
                assert_eq!(
                    document.pointer(&format!(
                        "/components/schemas/CareerApplicationRequest/properties/{field}/type"
                    )),
                    Some(&serde_json::json!("string"))
                );
            }
        }

        #[test]
        fn given_복무시작_exact요청_when_명령으로_변환하면_then_cursor와_option을_보존한다() {
            let request =
                serde_json::from_value::<MilitaryServiceStartRequest>(serde_json::json!({
                    "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                    "expectedRunRevision": 1,
                    "expectedStateRevision": 2,
                    "expectedGameDay": 3,
                    "militaryOptionVersionId": "7"
                }))
                .expect("복무 시작 요청 문법이 유효해야 한다");

            let command = StartMilitaryServiceCommand::try_from(request)
                .expect("복무 시작 명령으로 바꿀 수 있어야 한다");

            assert_eq!(command.military_option_version_id, ResourceId::from_u64(7));
            assert_eq!(command.cursor.expected_run_revision, 1);
            assert_eq!(command.cursor.expected_state_revision, 2);
            assert_eq!(command.cursor.expected_game_day, 3);
        }

        #[test]
        fn given_장병적금에_unknown필드_when_parse하면_then_요청을_거절한다() {
            let request = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "productVersionId": "8",
                "monthlyContributionKrw": 300000,
                "debitDayOfMonth": 10,
                "unexpected": true
            });

            let result = serde_json::from_value::<MilitarySavingsEnrollmentRequest>(request);

            assert!(result.is_err());
        }

        #[test]
        fn given_json_safe범위를_넘는_납입액_when_명령으로_변환하면_then_invalid_command로_거절한다()
         {
            let request =
                serde_json::from_value::<MilitarySavingsEnrollmentRequest>(serde_json::json!({
                    "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                    "expectedRunRevision": 1,
                    "expectedStateRevision": 2,
                    "expectedGameDay": 3,
                    "productVersionId": "8",
                    "monthlyContributionKrw": 9007199254740992_i64,
                    "debitDayOfMonth": 10
                }))
                .expect("i64 범위의 JSON 숫자는 문법상 읽을 수 있어야 한다");

            let result = OpenMilitarySavingsCommand::try_from(request);

            assert_eq!(result, Err(CareerFailureCode::InvalidCommand));
        }

        #[test]
        fn given_군명령_openapi_when_읽으면_then_cursor와_domain필드가_모두_required다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let service = document
                .pointer("/components/schemas/MilitaryServiceStartRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("복무 시작 필수 필드가 있어야 한다");
            let savings = document
                .pointer("/components/schemas/MilitarySavingsEnrollmentRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("장병적금 필수 필드가 있어야 한다");

            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
            ] {
                assert!(service.contains(&serde_json::json!(field)));
                assert!(savings.contains(&serde_json::json!(field)));
            }
            assert!(service.contains(&serde_json::json!("militaryOptionVersionId")));
            for field in [
                "productVersionId",
                "monthlyContributionKrw",
                "debitDayOfMonth",
            ] {
                assert!(savings.contains(&serde_json::json!(field)));
            }
        }

        #[test]
        fn given_다음커리어action_when_직렬화하면_then_exact_tagged_object를_반환한다() {
            let item = CareerPendingScheduleItemSnapshot::CareerAction {
                id: ResourceId::from_u64(9),
                due_game_day: 12,
                kind: CareerScheduledActionKindSnapshot::MilitaryServiceStart,
            };

            let value = serde_json::to_value(item).expect("다음 일정을 직렬화할 수 있어야 한다");

            assert_eq!(
                value,
                serde_json::json!({
                    "sourceKind": "careerAction",
                    "id": "9",
                    "dueGameDay": 12,
                    "kind": "militaryServiceStart"
                })
            );
        }

        #[test]
        fn given_경력evidence_when_직렬화하면_then_인정경력일을_nullable필드로_반환한다() {
            let evidence = CareerEvidenceSnapshot {
                id: ResourceId::from_u64(1),
                evidence_key: "militaryService:1:2".to_owned(),
                catalog_entry_id: ResourceId::from_u64(2),
                catalog_entry_key: "military-experience".to_owned(),
                display_name: "복무 경력".to_owned(),
                kind: crate::career::EvidenceKind::Experience,
                acquired_game_day: 100,
                expires_on_game_day: None,
                period_start_date: Some("2026-01-01".to_owned()),
                period_end_exclusive_date: Some("2027-01-01".to_owned()),
                credited_experience_days: Some(365),
            };

            let value = serde_json::to_value(evidence).expect("evidence를 직렬화할 수 있어야 한다");

            assert_eq!(value["creditedExperienceDays"], serde_json::json!(365));
        }

        #[test]
        fn given_portfolio_response_when_직렬화하면_then_camel_case_exact_shape를_반환한다() {
            let artifact = CareerArtifactVersionSnapshot::Portfolio {
                id: ResourceId::from_u64(7),
                version_no: 2,
                headline: "포트폴리오".to_owned(),
                summary: String::new(),
                evidence_ids: vec![ResourceId::from_u64(1)],
                completeness_bp: 6_000,
                created_game_day: 4,
            };

            let value = serde_json::to_value(artifact).expect("응답을 직렬화할 수 있어야 한다");

            assert_eq!(
                value,
                serde_json::json!({
                    "kind": "portfolio",
                    "id": "7",
                    "versionNo": 2,
                    "headline": "포트폴리오",
                    "summary": "",
                    "evidenceIds": ["1"],
                    "completenessBp": 6000,
                    "createdGameDay": 4
                })
            );
        }

        #[test]
        fn given_커리어_paths_when_openapi를_읽으면_then_모두_session_cookie를_요구한다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");

            for operation in [
                "/paths/~1api~1career~1specs/get",
                "/paths/~1api~1career~1activities/get",
                "/paths/~1api~1career~1activities/post",
                "/paths/~1api~1career~1activities~1{id}~1cancel/post",
                "/paths/~1api~1career~1artifacts/get",
                "/paths/~1api~1career~1artifacts/post",
                "/paths/~1api~1career~1focus/post",
                "/paths/~1api~1military~1options/get",
                "/paths/~1api~1military~1service/get",
                "/paths/~1api~1military~1service/post",
                "/paths/~1api~1military~1savings-products/get",
                "/paths/~1api~1military~1savings/get",
                "/paths/~1api~1military~1savings/post",
                "/paths/~1api~1military~1savings~1{id}~1close/post",
            ] {
                assert_eq!(
                    document.pointer(&format!("{operation}/security")),
                    Some(&serde_json::json!([{ "sessionCookie": [] }]))
                );
                assert!(
                    document
                        .pointer(&format!("{operation}/responses/401"))
                        .is_some()
                );
            }
        }
    }

    mod context_life_contract_is_generated {
        use super::*;

        fn given_budget_selections() -> serde_json::Value {
            serde_json::json!([
                { "category": "housing", "bandId": "11" },
                { "category": "food", "bandId": "11" },
                { "category": "transport", "bandId": "11" },
                { "category": "communication", "bandId": "11" },
                { "category": "utilities", "bandId": "11" },
                { "category": "healthcare", "bandId": "11" },
                { "category": "education", "bandId": "11" },
                { "category": "dependentCare", "bandId": "11" },
                { "category": "discretionary", "bandId": "11" }
            ])
        }

        fn given_budget_request(selections: serde_json::Value) -> LifeBudgetUpdateRequest {
            serde_json::from_value(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "selections": selections
            }))
            .expect("life budget request syntax must be valid")
        }

        fn given_quote_request() -> LoanQuoteRequest {
            serde_json::from_value(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "productVersionId": "17",
                "principalKrw": 30_000_000
            }))
            .expect("loan quote request syntax must be valid")
        }

        fn given_loan_execution_request() -> LoanExecutionRequest {
            serde_json::from_value(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "quoteId": "23"
            }))
            .expect("loan execution request syntax must be valid")
        }

        fn given_loan_prepayment_request() -> LoanPrepaymentRequest {
            serde_json::from_value(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "principalKrw": 5_000_000
            }))
            .expect("loan prepayment request syntax must be valid")
        }

        #[test]
        fn given_life_paths_when_openapi_is_read_then_they_require_the_session_cookie() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize");

            for operation in [
                "/paths/~1api~1loans/post",
                "/paths/~1api~1loans~1products/get",
                "/paths/~1api~1loans~1quotes/post",
                "/paths/~1api~1loans~1{loanId}/get",
                "/paths/~1api~1loans~1{loanId}~1installments/get",
                "/paths/~1api~1loans~1{loanId}~1prepayments/post",
                "/paths/~1api~1credit/get",
                "/paths/~1api~1welfare~1programs/get",
                "/paths/~1api~1welfare~1applications/post",
                "/paths/~1api~1housing~1listings/get",
                "/paths/~1api~1housing~1lease-deposit-loan-quotes/post",
                "/paths/~1api~1housing~1leases~1current/get",
                "/paths/~1api~1housing~1leases/post",
                "/paths/~1api~1housing~1holdings/get",
                "/paths/~1api~1housing~1mortgage-quotes/post",
                "/paths/~1api~1housing~1purchases/post",
                "/paths/~1api~1life~1budget/get",
                "/paths/~1api~1life~1budget/put",
                "/paths/~1api~1life~1arrears~1{id}~1payments/post",
                "/paths/~1api~1life~1events/get",
                "/paths/~1api~1life~1events~1{eventId}~1choices/post",
                "/paths/~1api~1insurance~1contracts/get",
                "/paths/~1api~1insurance~1contracts/post",
                "/paths/~1api~1insurance~1contracts~1{contractId}~1cancellations/post",
                "/paths/~1api~1insurance~1claims/post",
                "/paths/~1api~1insolvency/get",
                "/paths/~1api~1insolvency~1cases/post",
                "/paths/~1api~1insolvency~1{caseId}~1actions/post",
                "/paths/~1api~1insolvency~1{caseId}/get",
                "/paths/~1api~1insolvency~1{caseId}~1claims/get",
                "/paths/~1api~1insolvency~1{caseId}~1liquidations/get",
                "/paths/~1api~1corporations~1templates/get",
                "/paths/~1api~1corporations/post",
                "/paths/~1api~1corporations~1{corporationId}/get",
                "/paths/~1api~1corporations~1{corporationId}~1settings/put",
            ] {
                assert_eq!(
                    document.pointer(&format!("{operation}/security")),
                    Some(&serde_json::json!([{ "sessionCookie": [] }]))
                );
                assert!(
                    document
                        .pointer(&format!("{operation}/responses/401"))
                        .is_some()
                );
            }
        }

        #[test]
        fn given_life_schemas_when_openapi_is_read_then_fixed_values_stay_exact() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize");

            assert_eq!(
                document.pointer("/components/schemas/LifeFailureCodeSnapshot/enum"),
                Some(&serde_json::json!([
                    "invalidCommand",
                    "characterRequired",
                    "insufficientWalletCash",
                    "rateUnavailable",
                    "creditRestricted",
                    "incomeUnavailable",
                    "debtServiceLimit",
                    "collateralLimit",
                    "affordabilityLimit",
                    "contractConflict",
                    "idempotencyConflict",
                    "settlementConflict",
                    "housingResourceNotFound",
                    "welfareResourceNotFound",
                    "eventNotFound",
                    "eventExpired",
                    "insuranceResourceNotFound",
                    "insolvencyResourceNotFound",
                    "insolvencyCompositionUnsupported",
                    "insolvencyCompositionChanged",
                    "insolvencyStateConflict",
                    "corporationResourceNotFound",
                    "corporationStateConflict",
                    "claimNotCovered",
                    "ineligible",
                    "valuationUnavailable",
                    "policyUnsupported",
                    "busy"
                ]))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LivingCostMonthSnapshot/properties/prorationScale/minimum"
                ),
                Some(&serde_json::json!(377_580))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LivingCostMonthSnapshot/properties/prorationScale/maximum"
                ),
                Some(&serde_json::json!(377_580))
            );
            assert_eq!(
                document.pointer("/components/schemas/InsuranceCapabilitySnapshot/enum"),
                Some(&serde_json::json!(["contractsAndClaims", "unavailable"]))
            );
            assert_eq!(
                document.pointer("/components/schemas/InsuranceEligibilityStatusSnapshot/enum"),
                Some(&serde_json::json!([
                    "eligible",
                    "ineligible",
                    "indeterminate"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/InsuranceEligibilityReasonSnapshot/enum"),
                Some(&serde_json::json!([
                    "ageOutsideRange",
                    "dependentRequired",
                    "residenceRequired",
                    "militaryServing",
                    "authorityUnavailable"
                ]))
            );
            assert_eq!(
                document.pointer("/components/schemas/InsuranceContractStatusSnapshot/enum"),
                Some(&serde_json::json!([
                    "active",
                    "lapsed",
                    "expired",
                    "cancelled"
                ]))
            );
        }

        #[test]
        fn given_loan_read_schemas_when_openapi_is_read_then_bounds_and_nullable_id_are_exact() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize");
            let catalog_required = document
                .pointer("/components/schemas/LoanProductCatalogResponse/required")
                .and_then(serde_json::Value::as_array)
                .expect("loan product response fields must be required");
            let credit_required = document
                .pointer("/components/schemas/CreditResponse/required")
                .and_then(serde_json::Value::as_array)
                .expect("credit response fields must be required");

            for field in ["creditModelVersionId", "products"] {
                assert!(catalog_required.contains(&serde_json::json!(field)));
            }
            for field in [
                "creditBand",
                "creditReasons",
                "activeLoans",
                "nextLoanInstallment",
                "totalLoanBalanceKrw",
            ] {
                assert!(credit_required.contains(&serde_json::json!(field)));
            }
            let model_id_types = document
                .pointer(
                    "/components/schemas/LoanProductCatalogResponse/properties/creditModelVersionId/type",
                )
                .and_then(serde_json::Value::as_array)
                .expect("nullable credit model ID must have a type union");
            assert!(model_id_types.contains(&serde_json::json!("string")));
            assert!(model_id_types.contains(&serde_json::json!("null")));
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanProductCatalogResponse/properties/products/maxItems",
                ),
                Some(&serde_json::json!(16))
            );
            assert_eq!(
                document
                    .pointer("/components/schemas/CreditResponse/properties/activeLoans/maxItems"),
                Some(&serde_json::json!(8))
            );
        }

        #[test]
        fn given_대출견적_schema_when_openapi를_읽으면_then_요청경계와_응답필드가_exact하다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let required = document
                .pointer("/components/schemas/LoanQuoteRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("대출 견적 요청 필드는 모두 필수여야 한다");

            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
                "productVersionId",
                "principalKrw",
            ] {
                assert!(required.contains(&serde_json::json!(field)));
            }
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanQuoteRequest/properties/expectedStateRevision/maximum",
                ),
                Some(&serde_json::json!(MAX_JSON_SAFE_INTEGER))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanQuoteRequest/properties/principalKrw/maximum",
                ),
                Some(&serde_json::json!(MAX_JSON_SAFE_INTEGER))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanQuoteRequest/properties/productVersionId/pattern",
                ),
                Some(&serde_json::json!("^[1-9][0-9]*$"))
            );
            assert!(
                document
                    .pointer("/components/schemas/LoanQuoteResponse/properties/result")
                    .is_some()
            );
            assert!(
                document
                    .pointer("/components/schemas/LoanQuoteResponse/properties/replayed")
                    .is_some()
            );
            assert!(
                document
                    .pointer("/components/schemas/LoanQuoteResponse/properties/snapshot")
                    .is_some()
            );
        }

        #[test]
        fn given_정상_대출견적요청_when_command로_변환하면_then_cursor와_상품과_원금을_보존한다() {
            let request = given_quote_request();

            let command = CreateLoanQuoteCommand::try_from(request)
                .expect("정상 대출 견적 요청을 변환할 수 있어야 한다");

            assert_eq!(
                command.command_id.as_str(),
                "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2"
            );
            assert_eq!(
                command.cursor,
                CommandCursor {
                    expected_run_revision: 1,
                    expected_state_revision: 2,
                    expected_game_day: 3,
                }
            );
            assert_eq!(command.product_version_id, ResourceId::from_u64(17));
            assert_eq!(command.principal_krw, 30_000_000);
        }

        #[test]
        fn given_대출실행_schema_when_openapi를_읽으면_then_요청과_응답계약이_exact하다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");

            let request_required = document
                .pointer("/components/schemas/LoanExecutionRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("대출 실행 요청 필드는 모두 필수여야 한다");
            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
                "quoteId",
            ] {
                assert!(request_required.contains(&serde_json::json!(field)));
            }
            assert_eq!(request_required.len(), 5);
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanExecutionRequest/properties/expectedStateRevision/maximum",
                ),
                Some(&serde_json::json!(MAX_JSON_SAFE_INTEGER))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanExecutionRequest/properties/quoteId/pattern",
                ),
                Some(&serde_json::json!("^[1-9][0-9]*$"))
            );

            let result_required = document
                .pointer("/components/schemas/LoanExecutionResultSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("대출 실행 결과 필드는 모두 필수여야 한다");
            for field in [
                "loanId",
                "quoteId",
                "productVersionId",
                "principalKrw",
                "activatedGameDay",
                "maturityGameDay",
                "annualRateBp",
                "repaymentMethod",
                "termMonths",
                "firstInstallment",
            ] {
                assert!(result_required.contains(&serde_json::json!(field)));
            }
            assert_eq!(result_required.len(), 10);
            for field in ["result", "replayed", "snapshot"] {
                assert!(
                    document
                        .pointer(&format!(
                            "/components/schemas/LoanExecutionResponse/properties/{field}"
                        ))
                        .is_some()
                );
            }
        }

        #[test]
        fn given_정상_대출실행요청_when_command로_변환하면_then_cursor와_견적id를_보존한다() {
            let request = given_loan_execution_request();

            let command = ExecuteLoanCommand::try_from(request)
                .expect("정상 대출 실행 요청을 변환할 수 있어야 한다");

            assert_eq!(
                command.command_id.as_str(),
                "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2"
            );
            assert_eq!(
                command.cursor,
                CommandCursor {
                    expected_run_revision: 1,
                    expected_state_revision: 2,
                    expected_game_day: 3,
                }
            );
            assert_eq!(command.quote_id, ResourceId::from_u64(23));
        }

        #[test]
        fn given_알수없는_대출실행필드_when_json을_파싱하면_then_거절한다() {
            let request = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "quoteId": "23",
                "principalKrw": 30_000_000
            });

            let result = serde_json::from_value::<LoanExecutionRequest>(request);

            assert!(result.is_err());
        }

        #[test]
        fn given_leading_zero_견적id_when_대출실행command로_변환하면_then_거절한다() {
            let mut request = given_loan_execution_request();
            request.quote_id = "023".to_owned();

            let result = ExecuteLoanCommand::try_from(request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_unsafe_state_revision_when_대출실행command로_변환하면_then_거절한다() {
            let mut request = given_loan_execution_request();
            request.expected_state_revision = MAX_JSON_SAFE_INTEGER + 1;

            let result = ExecuteLoanCommand::try_from(request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_대출중도상환_schema_when_openapi를_읽으면_then_요청과_응답계약이_exact하다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let request_required = document
                .pointer("/components/schemas/LoanPrepaymentRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("중도상환 요청 필드는 모두 필수여야 한다");
            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
                "principalKrw",
            ] {
                assert!(request_required.contains(&serde_json::json!(field)));
            }
            assert_eq!(request_required.len(), 5);
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanPrepaymentRequest/properties/principalKrw/minimum"
                ),
                Some(&serde_json::json!(1))
            );
            assert_eq!(
                document.pointer(
                    "/paths/~1api~1loans~1{loanId}~1prepayments/post/parameters/0/schema/pattern"
                ),
                Some(&serde_json::json!("^[1-9][0-9]*$"))
            );
            assert_eq!(
                document.pointer("/components/schemas/LoanPrepaymentStatusSnapshot/enum"),
                Some(&serde_json::json!(["active", "paidOff"]))
            );
            let result_required = document
                .pointer("/components/schemas/LoanPrepaymentResultSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("중도상환 결과 필드는 모두 필수여야 한다");
            for field in [
                "loanId",
                "paymentId",
                "principalKrw",
                "feeKrw",
                "totalDebitedKrw",
                "appliedGameDay",
                "remainingPrincipalKrw",
                "status",
                "prepaymentEffect",
                "remainingInstallments",
                "nextInstallment",
                "finalInstallmentDueGameDay",
            ] {
                assert!(result_required.contains(&serde_json::json!(field)));
            }
            assert_eq!(result_required.len(), 12);
            let next_required = document
                .pointer("/components/schemas/LoanPrepaymentNextInstallmentSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("다음 중도상환 회차 필드는 모두 필수여야 한다");
            assert_eq!(next_required.len(), 6);
            for field in ["result", "replayed", "snapshot"] {
                assert!(
                    document
                        .pointer(&format!(
                            "/components/schemas/LoanPrepaymentResponse/properties/{field}"
                        ))
                        .is_some()
                );
            }
        }

        #[test]
        fn given_정상_중도상환요청_when_command로_변환하면_then_path대출과_cursor와_원금을_보존한다()
         {
            let request = given_loan_prepayment_request();

            let command = prepay_loan_command("31", request)
                .expect("정상 대출 중도상환 요청을 변환할 수 있어야 한다");

            assert_eq!(command.loan_id, ResourceId::from_u64(31));
            assert_eq!(command.principal_krw, 5_000_000);
            assert_eq!(
                command.cursor,
                CommandCursor {
                    expected_run_revision: 1,
                    expected_state_revision: 2,
                    expected_game_day: 3,
                }
            );
        }

        #[test]
        fn given_중도상환body에_loan_id가있을때_when_json을_파싱하면_then_거절한다() {
            let request = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "principalKrw": 5_000_000,
                "loanId": "31"
            });

            let result = serde_json::from_value::<LoanPrepaymentRequest>(request);

            assert!(result.is_err());
        }

        #[test]
        fn given_0원_when_중도상환command로_변환하면_then_거절한다() {
            let mut request = given_loan_prepayment_request();
            request.principal_krw = 0;

            let result = prepay_loan_command("31", request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_leading_zero_대출id_when_중도상환command로_변환하면_then_거절한다() {
            let request = given_loan_prepayment_request();

            let result = prepay_loan_command("031", request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_생활명령실패_when_메시지를만들면_then_계약과생활비에범용문구를쓴다() {
            assert_eq!(
                life_failure_message(LifeFailureCode::InsufficientWalletCash),
                "지갑 현금이 부족합니다"
            );
            assert_eq!(
                life_failure_message(LifeFailureCode::ContractConflict),
                "현재 상태에서 이 계약 요청을 처리할 수 없습니다"
            );
        }

        #[test]
        fn given_알수없는_대출견적필드_when_json을_파싱하면_then_거절한다() {
            let request = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "productVersionId": "17",
                "principalKrw": 30_000_000,
                "annualRateBp": 1
            });

            let result = serde_json::from_value::<LoanQuoteRequest>(request);

            assert!(result.is_err());
        }

        #[test]
        fn given_unsafe_state_revision_when_대출견적command로_변환하면_then_거절한다() {
            let mut request = given_quote_request();
            request.expected_state_revision = MAX_JSON_SAFE_INTEGER + 1;

            let result = CreateLoanQuoteCommand::try_from(request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_unsafe_principal_when_대출견적command로_변환하면_then_거절한다() {
            let mut request = given_quote_request();
            request.principal_krw = MAX_JSON_SAFE_INTEGER as i64 + 1;

            let result = CreateLoanQuoteCommand::try_from(request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_zero_principal_when_대출견적command로_변환하면_then_거절한다() {
            let mut request = given_quote_request();
            request.principal_krw = 0;

            let result = CreateLoanQuoteCommand::try_from(request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_비정규_uuid_when_대출견적command로_변환하면_then_거절한다() {
            let mut request = given_quote_request();
            request.command_id = "4F521F4C-9DD8-4D20-8E1F-15CB13CBE0F2".to_owned();

            let result = CreateLoanQuoteCommand::try_from(request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_leading_zero_상품id_when_대출견적command로_변환하면_then_거절한다() {
            let mut request = given_quote_request();
            request.product_version_id = "017".to_owned();

            let result = CreateLoanQuoteCommand::try_from(request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_every_category_once_when_budget_is_converted_then_it_is_canonicalized() {
            let request = given_budget_request(given_budget_selections());

            let command = UpdateLifeBudgetCommand::try_from(request)
                .expect("complete life budget must be accepted");

            assert_eq!(command.selections.len(), LivingCostCategory::ALL.len());
            assert!(
                command
                    .selections
                    .iter()
                    .map(|selection| selection.category)
                    .eq(LivingCostCategory::ALL)
            );
        }

        #[test]
        fn given_a_duplicate_category_when_budget_is_converted_then_it_is_rejected() {
            let mut selections = given_budget_selections();
            selections[8]["category"] = serde_json::json!("housing");
            let request = given_budget_request(selections);

            let result = UpdateLifeBudgetCommand::try_from(request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_an_unknown_payment_field_when_parsed_then_it_is_rejected() {
            let request = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "amountKrw": 10000,
                "remainingKrw": 20000
            });

            let result = serde_json::from_value::<EssentialArrearPaymentRequest>(request);

            assert!(result.is_err());
        }

        #[test]
        fn given_an_unsafe_payment_amount_when_converted_then_it_is_rejected() {
            let request =
                serde_json::from_value::<EssentialArrearPaymentRequest>(serde_json::json!({
                    "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                    "expectedRunRevision": 1,
                    "expectedStateRevision": 2,
                    "expectedGameDay": 3,
                    "amountKrw": 9007199254740992_i64
                }))
                .expect("an i64 JSON number must parse before semantic validation");

            let result = essential_arrear_payment_command(ResourceId::from_u64(1), request);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_정규_dual_cursor_when_파싱하면_then_두_exclusive경계를보존한다() {
            let loan_id = ResourceId::from_u64(31);

            let result = parse_loan_installment_cursor("v1.l31.i12.p8", loan_id)
                .expect("정규 cursor를 파싱할 수 있어야 한다");

            assert_eq!(
                result,
                LoanInstallmentPageCursor {
                    loan_id,
                    installment_before: Some(12),
                    payment_before: Some(8),
                }
            );
        }

        #[test]
        fn given_0_sentinel_cursor_when_파싱하면_then_두window가_exhausted다() {
            let loan_id = ResourceId::from_u64(31);

            let result = parse_loan_installment_cursor("v1.l31.i0.p0", loan_id)
                .expect("종료 cursor도 정규 token이어야 한다");

            assert_eq!(result.installment_before, None);
            assert_eq!(result.payment_before, None);
        }

        #[test]
        fn given_최초query와_terminal_cursor_when_query를만들면_then_outer_before로구분한다() {
            let loan_id = ResourceId::from_u64(31);
            let initial = loan_installment_page_query(
                loan_id,
                LoanInstallmentsQuery {
                    before: None,
                    limit: None,
                },
            )
            .expect("최초 query를 만들 수 있어야 한다");
            let terminal = loan_installment_page_query(
                loan_id,
                LoanInstallmentsQuery {
                    before: Some("v1.l31.i0.p0".to_owned()),
                    limit: None,
                },
            )
            .expect("terminal cursor query를 만들 수 있어야 한다");

            assert_eq!(initial.before, None);
            assert_eq!(
                terminal.before,
                Some(LoanInstallmentPageCursor {
                    loan_id,
                    installment_before: None,
                    payment_before: None,
                })
            );
            assert_eq!(initial.limit, 50);
        }

        #[test]
        fn given_비정규_or_다른대출_cursor_when_파싱하면_then_모두거절한다() {
            let loan_id = ResourceId::from_u64(31);

            for value in [
                "v1.l031.i12.p8",
                "v1.l31.i012.p8",
                "v1.l31.i12.p08",
                "v1.l32.i12.p8",
                "v1.l31.i65536.p8",
                "v1.l31.i12.p4294967296",
                "V1.l31.i12.p8",
                "v1.l31.i12.p8.extra",
            ] {
                assert_eq!(
                    parse_loan_installment_cursor(value, loan_id),
                    Err(LifeFailureCode::InvalidCommand),
                    "{value}는 거절해야 한다"
                );
            }
        }

        #[test]
        fn given_limit범위밖_when_history_query를만들면_then_거절한다() {
            let query = LoanInstallmentsQuery {
                before: None,
                limit: Some("51".to_owned()),
            };

            let result = loan_installment_page_query(ResourceId::from_u64(31), query);

            assert_eq!(result, Err(LifeFailureCode::InvalidCommand));
        }

        #[test]
        fn given_정규주거region_when_query를파싱하면_then_typed_region을보존한다() {
            let value = serde_json::json!({"region": "rural"});

            let parsed = serde_json::from_value::<HousingListingsQuery>(value)
                .expect("정규 주거 지역을 파싱할 수 있어야 한다");
            let query = HousingListingsQueryState {
                region: parsed.region.map(Into::into),
            };

            assert_eq!(query.region, Some(LifeRegionKey::Rural));
        }

        #[test]
        fn given_알수없는주거region_when_query를파싱하면_then_거절한다() {
            let value = serde_json::json!({"region": "overseas"});

            let result = serde_json::from_value::<HousingListingsQuery>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_주거query에unknown필드_when_deserialize하면_then_거절한다() {
            let value = serde_json::json!({"region": "rural", "limit": 24});

            let result = serde_json::from_value::<HousingListingsQuery>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_정규현금전세요청_when_command로바꾸면_then_listing과cursor를보존한다() {
            let request = serde_json::from_value::<StartHousingLeaseRequest>(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "listingId": "17",
                "offerKind": "jeonse"
            }))
            .expect("정규 전세 요청을 파싱할 수 있어야 한다");

            let command = StartHousingLeaseCommand::try_from(request)
                .expect("정규 전세 요청을 command로 바꿀 수 있어야 한다");

            assert_eq!(command.listing_id, ResourceId::from_u64(17));
            assert_eq!(command.offer_kind, HousingLeaseOfferKind::Jeonse);
            assert_eq!(command.cursor.expected_run_revision, 1);
            assert_eq!(command.cursor.expected_state_revision, 2);
            assert_eq!(command.cursor.expected_game_day, 3);
        }

        #[test]
        fn given_월세요청_when_임대차request로deserialize하면_then_월세command로보존한다() {
            let request = serde_json::from_value::<StartHousingLeaseRequest>(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "listingId": "17",
                "offerKind": "monthlyRent"
            }))
            .expect("정규 월세 요청을 파싱할 수 있어야 한다");

            let command = StartHousingLeaseCommand::try_from(request)
                .expect("정규 월세 요청을 command로 바꿀 수 있어야 한다");

            assert_eq!(command.listing_id, ResourceId::from_u64(17));
            assert_eq!(command.offer_kind, HousingLeaseOfferKind::MonthlyRent);
        }

        #[test]
        fn given_금액필드가있는전세요청_when_deserialize하면_then_거절한다() {
            let value = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "listingId": "17",
                "offerKind": "jeonse",
                "depositKrw": 10_000_000
            });

            let result = serde_json::from_value::<StartHousingLeaseRequest>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_정규주담대견적요청_when_command로바꾸면_then_listing과원금을보존한다() {
            let request = serde_json::from_value::<MortgageQuoteRequest>(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "listingId": "17",
                "productVersionId": "23",
                "principalKrw": 300_000_000
            }))
            .expect("정규 주담대 견적 요청을 파싱할 수 있어야 한다");

            let command = CreateMortgageQuoteCommand::try_from(request)
                .expect("정규 주담대 견적 요청을 command로 바꿀 수 있어야 한다");

            assert_eq!(command.listing_id, ResourceId::from_u64(17));
            assert_eq!(command.product_version_id, ResourceId::from_u64(23));
            assert_eq!(command.principal_krw, 300_000_000);
            assert_eq!(command.cursor.expected_state_revision, 2);
        }

        #[test]
        fn given_mortgage_quote_id가_null인매수요청_when_command로바꾸면_then_현금매수다() {
            let request = serde_json::from_value::<PropertyPurchaseRequest>(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "listingId": "17",
                "mortgageQuoteId": null
            }))
            .expect("명시적 null 현금 매수 요청을 파싱할 수 있어야 한다");

            let command = PurchasePropertyCommand::try_from(request)
                .expect("현금 매수 요청을 command로 바꿀 수 있어야 한다");

            assert_eq!(command.listing_id, ResourceId::from_u64(17));
            assert_eq!(command.mortgage_quote_id, None);
        }

        #[test]
        fn given_mortgage_quote_id가누락된매수요청_when_deserialize하면_then_거절한다() {
            let value = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "listingId": "17"
            });

            let result = serde_json::from_value::<PropertyPurchaseRequest>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_정규월세연체상환요청_when_command로바꾸면_then_path와cursor를보존한다() {
            let request = serde_json::from_value::<LeaseArrearPaymentRequest>(serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "amountKrw": 40_000
            }))
            .expect("정규 월세 연체 상환 요청을 파싱할 수 있어야 한다");

            let command = lease_arrear_payment_command(ResourceId::from_u64(17), request)
                .expect("정규 월세 연체 상환 요청을 command로 바꿀 수 있어야 한다");

            assert_eq!(command.arrear_id, ResourceId::from_u64(17));
            assert_eq!(command.amount_krw, 40_000);
            assert_eq!(command.cursor.expected_run_revision, 1);
            assert_eq!(command.cursor.expected_state_revision, 2);
            assert_eq!(command.cursor.expected_game_day, 3);
        }

        #[test]
        fn given_월세연체상환요청에unknown필드_when_deserialize하면_then_거절한다() {
            let value = serde_json::json!({
                "commandId": "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2",
                "expectedRunRevision": 1,
                "expectedStateRevision": 2,
                "expectedGameDay": 3,
                "amountKrw": 40_000,
                "leaseArrearId": "17"
            });

            let result = serde_json::from_value::<LeaseArrearPaymentRequest>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_주거listing_contract_when_openapi를읽으면_then_path와중첩schema가완전하다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let required = document
                .pointer("/components/schemas/HousingListingsResponse/required")
                .and_then(serde_json::Value::as_array)
                .expect("주거 listing 응답 필드는 모두 필수여야 한다");
            let expected_required = vec![
                serde_json::json!("rateStatus"),
                serde_json::json!("modelVersionId"),
                serde_json::json!("gameDay"),
                serde_json::json!("yearMonth"),
                serde_json::json!("residenceRegionKey"),
                serde_json::json!("selectedRegionKey"),
                serde_json::json!("regions"),
                serde_json::json!("priceIndexPpm"),
                serde_json::json!("rentIndexPpm"),
                serde_json::json!("listings"),
            ];

            assert!(
                document
                    .pointer("/paths/~1api~1housing~1listings/get")
                    .is_some()
            );
            assert_eq!(required, &expected_required);
            for schema in [
                "HousingRegionKeySnapshot",
                "HousingRateStatusSnapshot",
                "HousingPropertyTypeSnapshot",
                "HousingOfferSnapshot",
                "HousingRegionSnapshot",
                "HousingListingSnapshot",
            ] {
                assert!(
                    document
                        .pointer(&format!("/components/schemas/{schema}"))
                        .is_some(),
                    "{schema}가 components에 있어야 한다"
                );
            }
        }

        #[test]
        fn given_c3_매수_contract_when_openapi를읽으면_then_요청과응답이_strict하다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let quote_required = document
                .pointer("/components/schemas/MortgageQuoteRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("주담대 견적 요청 필드는 모두 필수여야 한다");
            let purchase_required = document
                .pointer("/components/schemas/PropertyPurchaseRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("주택 매수 요청 필드는 모두 필수여야 한다");
            let quote_result_required = document
                .pointer("/components/schemas/MortgageQuoteResultSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("주담대 견적 결과 필드는 모두 필수여야 한다");
            let purchase_result_required = document
                .pointer("/components/schemas/PropertyPurchaseResultSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("주택 매수 결과 필드는 모두 필수여야 한다");

            assert_eq!(quote_required.len(), 7);
            assert_eq!(purchase_required.len(), 6);
            assert!(purchase_required.contains(&serde_json::json!("mortgageQuoteId")));
            assert_eq!(quote_result_required.len(), 30);
            assert_eq!(purchase_result_required.len(), 12);
            assert_eq!(
                document.pointer("/components/schemas/MortgageStressTreatmentSnapshot/enum"),
                Some(&serde_json::json!(["fullTermFixed"]))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/MortgageQuoteResultSnapshot/properties/stressRateBp/maximum"
                ),
                Some(&serde_json::json!(0))
            );
            assert!(
                document
                    .pointer("/components/schemas/MortgageLtvSnapshot/properties/ratioPpm/maximum")
                    .is_none()
            );
            for path in [
                "/paths/~1api~1housing~1holdings/get",
                "/paths/~1api~1housing~1mortgage-quotes/post",
                "/paths/~1api~1housing~1purchases/post",
            ] {
                assert!(document.pointer(path).is_some(), "{path}가 있어야 한다");
            }
            for field in [
                "activePropertyHoldings",
                "hasMoreActivePropertyHoldings",
                "totalPropertyBookValueKrw",
            ] {
                assert!(
                    document
                        .pointer("/components/schemas/LifeSnapshot/required")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|required| required.contains(&serde_json::json!(field)))
                );
            }
        }

        #[test]
        fn given_현금임대차contract_when_openapi를읽으면_then_request와응답필드가완전하다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let cash_request_required = document
                .pointer("/components/schemas/StartHousingLeaseCashRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("현금 임대차 요청 필드는 모두 필수여야 한다");
            let financed_request_required = document
                .pointer("/components/schemas/StartHousingLeaseFinancedRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("대출 임대차 요청 필드는 모두 필수여야 한다");
            let request_variants = document
                .pointer("/components/schemas/StartHousingLeaseRequest/oneOf")
                .and_then(serde_json::Value::as_array)
                .expect("임대차 요청은 strict union이어야 한다");
            let current_required = document
                .pointer("/components/schemas/HousingLeaseCurrentResponse/required")
                .and_then(serde_json::Value::as_array)
                .expect("현재 임대차 응답 필드는 모두 필수여야 한다");
            let result_required = document
                .pointer("/components/schemas/HousingLeaseMoveResultSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("임대차 이동 결과 필드는 모두 필수여야 한다");

            assert_eq!(cash_request_required.len(), 6);
            assert_eq!(financed_request_required.len(), 7);
            assert_eq!(request_variants.len(), 2);
            let expected_current_required = [
                "leaseCapability",
                "renewalRule",
                "leaseLifecycleTerms",
                "movingCosts",
                "tenantLeaseDepositKrw",
                "activeLease",
                "monthlyRentTerms",
                "activeArrears",
                "hasMoreActiveArrears",
                "totalLeaseArrearKrw",
            ]
            .map(serde_json::Value::from);

            assert_eq!(current_required, &expected_current_required);
            assert_eq!(result_required.len(), 17);
            assert_eq!(
                document.pointer(
                    "/components/schemas/StartHousingLeaseCashRequest/properties/listingId/pattern"
                ),
                Some(&serde_json::json!("^[1-9][0-9]*$"))
            );
            for property in [
                "/components/schemas/StartHousingLeaseFinancedRequest/properties/offerKind/$ref",
                "/components/schemas/LeaseDepositLoanQuoteRequest/properties/offerKind/$ref",
            ] {
                assert_eq!(
                    document.pointer(property),
                    Some(&serde_json::json!(
                        "#/components/schemas/JeonseHousingLeaseOfferKindRequest"
                    ))
                );
            }
            assert_eq!(
                document.pointer(
                    "/components/schemas/LeaseDepositLoanQuoteResultSnapshot/properties/offerKind/$ref"
                ),
                Some(&serde_json::json!(
                    "#/components/schemas/JeonseHousingLeaseOfferKindSnapshot"
                ))
            );
            assert_eq!(
                document.pointer("/components/schemas/JeonseHousingLeaseOfferKindRequest/enum"),
                Some(&serde_json::json!(["jeonse"]))
            );
            assert_eq!(
                document.pointer("/components/schemas/JeonseHousingLeaseOfferKindSnapshot/enum"),
                Some(&serde_json::json!(["jeonse"]))
            );
            assert_eq!(
                document.pointer("/components/schemas/RegulatoryDsrAppliedSnapshot/type"),
                Some(&serde_json::json!("boolean"))
            );
            assert_eq!(
                document.pointer("/components/schemas/RegulatoryDsrAppliedSnapshot/enum"),
                Some(&serde_json::json!([false]))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LeaseDepositLoanQuoteResultSnapshot/properties/regulatoryDsrApplied/$ref"
                ),
                Some(&serde_json::json!(
                    "#/components/schemas/RegulatoryDsrAppliedSnapshot"
                ))
            );
            assert!(result_required.contains(&serde_json::json!("depositLoanExecution")));
            assert!(result_required.contains(&serde_json::json!("repaidDepositLoan")));
            let active_lease_required = document
                .pointer("/components/schemas/ActiveHousingLeaseSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("현재 임대차 필드는 모두 필수여야 한다");
            assert!(active_lease_required.contains(&serde_json::json!("depositLoanId")));
            assert_eq!(
                document.pointer(
                    "/components/schemas/HousingLeaseCurrentResponse/properties/movingCosts/maxItems"
                ),
                Some(&serde_json::json!(4))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/MonthlyRentTerminationReviewTermsSnapshot/properties/afterGameDays/minimum"
                ),
                Some(&serde_json::json!(1))
            );
            for schema in [
                "LeaseLifecycleTermsSnapshot",
                "ActiveLeaseTermSnapshot",
                "LeaseRenewalNoticeSnapshot",
                "LeaseTerminationReviewSnapshot",
            ] {
                assert!(
                    document
                        .pointer(&format!("/components/schemas/{schema}"))
                        .is_some(),
                    "{schema}가 components에 있어야 한다"
                );
            }
            assert!(
                document
                    .pointer("/paths/~1api~1housing~1leases~1current/get")
                    .is_some()
            );
            assert!(
                document
                    .pointer("/paths/~1api~1housing~1leases/post")
                    .is_some()
            );
        }

        #[test]
        fn given_월세연체상환contract_when_openapi를읽으면_then_path와금액경계가완전하다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let request_required = document
                .pointer("/components/schemas/LeaseArrearPaymentRequest/required")
                .and_then(serde_json::Value::as_array)
                .expect("월세 연체 상환 요청 필드는 모두 필수여야 한다");
            let result_required = document
                .pointer("/components/schemas/LeaseArrearPaymentResultSnapshot/required")
                .and_then(serde_json::Value::as_array)
                .expect("월세 연체 상환 결과 필드는 모두 필수여야 한다");

            assert_eq!(request_required.len(), 5);
            assert_eq!(result_required.len(), 4);
            assert_eq!(
                document.pointer(
                    "/paths/~1api~1housing~1lease-arrears~1{id}~1payments/post/parameters/0/schema/pattern"
                ),
                Some(&serde_json::json!("^[1-9][0-9]*$"))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LeaseArrearPaymentRequest/properties/amountKrw/minimum"
                ),
                Some(&serde_json::json!(1))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LeaseArrearPaymentRequest/properties/amountKrw/maximum"
                ),
                Some(&serde_json::json!(MAX_JSON_SAFE_INTEGER))
            );
        }

        #[test]
        fn given_history에_unknown_query_when_deserialize하면_then_거절한다() {
            let value = serde_json::json!({"before": null, "offset": "10"});

            let result = serde_json::from_value::<LoanInstallmentsQuery>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_상세에_unknown_query_when_deserialize하면_then_거절한다() {
            let value = serde_json::json!({"expand": "payments"});

            let result = serde_json::from_value::<LoanDetailQuery>(value);

            assert!(result.is_err());
        }

        #[test]
        fn given_대출상세와history_schema_when_openapi를읽으면_then_exact필드와query경계다() {
            let document =
                serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI를 만들 수 있어야 한다");
            let detail_required = document
                .pointer("/components/schemas/LoanDetailResponse/required")
                .and_then(serde_json::Value::as_array)
                .expect("대출 상세 필드는 모두 필수여야 한다");
            let history_required = document
                .pointer("/components/schemas/LoanInstallmentsResponse/required")
                .and_then(serde_json::Value::as_array)
                .expect("대출 history 필드는 모두 필수여야 한다");

            assert_eq!(detail_required.len(), 27);
            for field in [
                "currentAnnualRateBp",
                "termMonths",
                "totalInstallments",
                "maturityGameDay",
                "finalInstallmentDueGameDay",
                "nextInstallmentNo",
                "oldestUnpaidDueGameDay",
                "prepaymentFeePpm",
                "prepaymentEffect",
                "leaseContractId",
                "propertyHoldingId",
            ] {
                assert!(detail_required.contains(&serde_json::json!(field)));
            }
            assert_eq!(history_required.len(), 6);
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanInstallmentsResponse/properties/installments/maxItems"
                ),
                Some(&serde_json::json!(50))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanInstallmentsResponse/properties/payments/maxItems"
                ),
                Some(&serde_json::json!(50))
            );
            assert_eq!(
                document.pointer(
                    "/components/schemas/LoanPaymentSnapshot/properties/allocations/maxItems"
                ),
                Some(&serde_json::json!(8))
            );
            assert_eq!(
                document.pointer("/components/schemas/LoanPaymentAllocationKindSnapshot/enum"),
                Some(&serde_json::json!([
                    "overdueFee",
                    "overdueInterest",
                    "overduePrincipal",
                    "currentFee",
                    "currentInterest",
                    "currentPrincipal",
                    "prepaymentFee",
                    "prepaymentPrincipal"
                ]))
            );

            let parameters = document
                .pointer("/paths/~1api~1loans~1{loanId}~1installments/get/parameters")
                .and_then(serde_json::Value::as_array)
                .expect("history query parameters가 있어야 한다");
            let before = parameters
                .iter()
                .find(|parameter| parameter.get("name") == Some(&serde_json::json!("before")))
                .expect("before parameter가 있어야 한다");
            let limit = parameters
                .iter()
                .find(|parameter| parameter.get("name") == Some(&serde_json::json!("limit")))
                .expect("limit parameter가 있어야 한다");
            assert_eq!(
                before.pointer("/schema/pattern"),
                Some(&serde_json::json!(
                    "^v1\\.l[1-9][0-9]*\\.i(?:0|[1-9][0-9]*)\\.p(?:0|[1-9][0-9]*)$"
                ))
            );
            assert_eq!(
                limit.pointer("/schema/default"),
                Some(&serde_json::json!(50))
            );
            assert_eq!(
                limit.pointer("/schema/minimum"),
                Some(&serde_json::json!(1))
            );
            assert_eq!(
                limit.pointer("/schema/maximum"),
                Some(&serde_json::json!(50))
            );
            assert!(
                document
                    .pointer("/paths/~1api~1loans~1{loanId}/get/responses/404")
                    .is_some()
            );
            assert!(
                document
                    .pointer("/paths/~1api~1loans~1{loanId}~1installments/get/responses/404")
                    .is_some()
            );
        }
    }

    mod context_clock_request_is_parsed {
        use super::*;

        #[test]
        fn given_explicit_null_when_parsed_then_it_means_pause() {
            let request = serde_json::from_value::<ClockRequest>(serde_json::json!({
                "speed": null
            }))
            .expect("explicit null must be accepted");

            assert_eq!(request.speed.0, None);
        }

        #[test]
        fn given_the_speed_field_is_missing_when_parsed_then_it_is_rejected() {
            let request = serde_json::from_value::<ClockRequest>(serde_json::json!({}));

            assert!(request.is_err());
        }

        #[test]
        fn given_an_unsupported_speed_when_parsed_then_it_is_rejected() {
            let request = serde_json::from_value::<ClockRequest>(serde_json::json!({
                "speed": 3
            }));

            assert!(request.is_err());
        }
    }
}
