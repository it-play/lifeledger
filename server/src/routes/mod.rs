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
use axum::routing::{get, post};
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
use crate::career::{ArtifactDraft, ArtifactKind, CareerFailureCode, Industry, LinkedinFields};
use crate::character;
use crate::error::AppError;
use crate::finance::{
    AssetOrderSide, BondCatalog, BondOrderCommand, CashProductKind, CloseCashProductCommand,
    CloseCmaAccountCommand, CommandCursor, CommandId, FinanceFailureCode, FinancialAccountType,
    GoldCatalog, GoldOrderCommand, GoldWithdrawalCommand, IrpWithdrawalReason, M2dAccountType,
    OpenCashProductCommand, OpenCmaAccountCommand, OpenGoldAccountCommand,
    PensionWithdrawalRequestKind, ResourceId, TransferCommand, TransferDirection,
};
use crate::state::{
    AdvanceCommandSnapshot, AdvanceResponse, AppState, AssetCommandResult, AutoSpeed,
    BondOrderResponse, CareerActivitiesResponse, CareerActivityCatalogSnapshot,
    CareerActivityHistorySnapshot, CareerActivityResponse, CareerActivityResultSnapshot,
    CareerActivitySnapshot, CareerApplicationResponse, CareerApplicationResultSnapshot,
    CareerApplicationSnapshot, CareerApplicationsResponse, CareerArtifactResponse,
    CareerArtifactResultSnapshot, CareerArtifactSnapshot, CareerArtifactVersionSnapshot,
    CareerArtifactsResponse, CareerCommandResult, CareerEmploymentContractSnapshot,
    CareerEmploymentResponse, CareerEvidenceSnapshot, CareerFocusResponse,
    CareerFocusResultSnapshot, CareerInvitationResponse, CareerInvitationResultSnapshot,
    CareerInvitationSnapshot, CareerJobSnapshot, CareerJobsResponse, CareerOfferResponse,
    CareerOfferResultSnapshot, CareerOfferSnapshot, CareerOpenApplicationSnapshot,
    CareerScoresSnapshot, CareerSnapshot, CareerSpecsResponse, CashContractSnapshot,
    CashProductCatalogResponse, CashProductCommandResult, CashProductVersionSnapshot,
    CharacterStartResponse, CharacterStartSnapshot, CmaAccountCloseResponse,
    CmaAccountCloseSnapshot, CmaAccountOpenResponse, CmaAccountOpenSnapshot, CmaAccountSnapshot,
    DepositCloseResponse, DepositCloseSnapshot, DepositKindSnapshot, DepositOpenResponse,
    DepositOpenSnapshot, DepositProtectionSnapshot, FinanceAccountsResponse, FinanceCommandResult,
    FinanceSnapshot, FinanceTransferResponse, FinanceTransferSnapshot, FinancialAccountSnapshot,
    FinancialIncomeAssessmentSnapshot, FinancialIncomeSourceSnapshot, FinancialIncomeYearSnapshot,
    FinancialIncomeYearStatusSnapshot, FinancialInstitutionSnapshot, GameCommandCursorSnapshot,
    GameLoopError, GameSnapshot, GoldAccountOpenResponse, GoldOrderResponse,
    GoldWithdrawalResponse, IsaAccountSnapshot, IsaCloseResponse, IsaCloseSnapshot,
    LedgerPageResponse, LedgerPostingSnapshot, LedgerTransactionSnapshot, M2MarketFactorsSnapshot,
    MarketHistoryPoint, MarketHistoryResponse, MarketIndexSnapshot, MarketRatesSnapshot,
    MarketSnapshot, PendingSettlementSnapshot, PensionAccountSnapshot, PensionStartResponse,
    PensionStartSnapshot, PensionTaxLayersSnapshot, PensionWithdrawalResponse,
    PensionWithdrawalSnapshot, PlaceOrderResult, PolicySetSnapshot, PortfolioOrderResponse,
    StreamConnection, TaxAccountCommandResult, TaxAccountOpenResponse, TaxAccountOpenSnapshot,
};
use crate::store::{
    AcceptCareerInvitationCommand, AcceptCareerOfferCommand, ApplyCareerCommand,
    CancelCareerActivityCommand, CareerArtifactPageQuery, CareerJobsPageQuery, CareerPageQuery,
    CareerPlatform, CloseIsaAccountCommand, ConfirmCareerInterviewCommand,
    DeclineCareerInvitationCommand, DeclineCareerOfferCommand, FocusCareerCommand,
    InterviewDecision, ManualAdvanceCommand, OpenTaxAccountCommand, PensionWithdrawalCommand,
    PublishCareerArtifactCommand, StartCareerActivityCommand, StartGameCommand,
    StartPensionCommand, WithdrawCareerApplicationCommand,
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
        apply_career,
        confirm_career_interview,
        withdraw_career_application,
        accept_career_invitation,
        decline_career_invitation,
        accept_career_offer,
        decline_career_offer,
        market_history,
        clock,
        stream,
        auth::providers,
        auth::me,
        auth::logout,
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

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CharacterStartRequest {
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

impl CharacterStartRequest {
    fn into_command(self) -> Result<StartGameCommand, GameLoopError> {
        Ok(StartGameCommand {
            command_id: CommandId::parse(self.command_id)
                .map_err(|_| GameLoopError::InvalidCommand)?,
            cursor: CommandCursor {
                expected_run_revision: self.expected_run_revision,
                expected_state_revision: self.expected_state_revision,
                expected_game_day: self.expected_game_day,
            },
            draft: self.character,
        })
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
    CharacterRequired,
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
            GameLoopError::CharacterRequired => (
                StatusCode::CONFLICT,
                GameCommandFailureCode::CharacterRequired,
                "먼저 캐릭터를 생성해야 합니다",
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
    expected_state_revision: u64,
    expected_game_day: u32,
}

impl CareerCursorRequest {
    fn into_parts(self) -> Result<(CommandId, CommandCursor), CareerFailureCode> {
        Ok((
            CommandId::parse(self.command_id).map_err(|_| CareerFailureCode::InvalidCommand)?,
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
struct CareerApplicationRequest {
    command_id: String,
    expected_run_revision: u32,
    expected_state_revision: u64,
    expected_game_day: u32,
    #[schema(min_length = 64, max_length = 64, pattern = "^[0-9a-f]{64}$")]
    posting_key: String,
    #[schema(required = false, nullable, pattern = "^[1-9][0-9]*$")]
    resume_version_id: Option<String>,
    #[schema(required = false, nullable, pattern = "^[1-9][0-9]*$")]
    portfolio_version_id: Option<String>,
    #[schema(required = false, nullable, pattern = "^[1-9][0-9]*$")]
    linkedin_profile_version_id: Option<String>,
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
                    "financialIncomeFiling"
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
                    "careerDevelopmentExpense"
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

        #[test]
        fn given_start_and_advance_requests_when_read_then_every_command_cursor_field_is_required()
        {
            let document = given_openapi_document();

            let start = required_fields(&document, "CharacterStartRequest");
            let advance = required_fields(&document, "AdvanceRequest");

            for field in [
                "commandId",
                "expectedRunRevision",
                "expectedStateRevision",
                "expectedGameDay",
            ] {
                assert!(start.contains(&serde_json::json!(field)));
                assert!(advance.contains(&serde_json::json!(field)));
            }
            assert!(start.contains(&serde_json::json!("character")));
            assert!(advance.contains(&serde_json::json!("days")));
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
                "/components/schemas/CharacterStartRequest/properties/commandId",
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
