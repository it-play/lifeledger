use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize, Serializer};
use utoipa::ToSchema;

pub const LLX_SYMBOL: &str = "LLX";
pub const MAX_TRADE_QUANTITY: u32 = 1_000_000;

/// A non-zero database resource id. JSON uses a decimal string to preserve all `u64` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AccountId(u64);

impl AccountId {
    pub const fn from_u64(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    fn parse(raw: String) -> Result<Self, TradeFailure> {
        parse_resource_id(&raw)
            .map(Self)
            .ok_or(TradeFailure::invalid_order(
                "계좌 ID는 0이 아닌 표준 10진 문자열이어야 합니다",
            ))
    }
}

impl Serialize for AccountId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Buy,
    Sell,
}

/// The HTTP order shape before the domain validates its identifier and limits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TradeOrderRequest {
    pub order_id: String,
    pub account_id: String,
    pub expected_run_revision: u32,
    pub expected_state_revision: u64,
    pub expected_game_day: u32,
    pub side: OrderSide,
    pub symbol: String,
    pub quantity: u32,
}

/// A lower-case, hyphenated UUID that is safe to use as an idempotency key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderId(String);

impl OrderId {
    pub fn parse(raw: String) -> Result<Self, TradeFailure> {
        if is_canonical_uuid(&raw) {
            Ok(Self(raw))
        } else {
            Err(TradeFailure::invalid_order(
                "주문 ID는 표준 UUID 형식이어야 합니다",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeOrder {
    pub order_id: OrderId,
    pub account_id: AccountId,
    pub expected_run_revision: u32,
    pub expected_state_revision: u64,
    pub expected_game_day: u32,
    pub side: OrderSide,
    pub symbol: String,
    pub quantity: u32,
}

impl TradeOrder {
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub(crate) fn validate(&self) -> Result<(), TradeFailure> {
        if self.symbol != LLX_SYMBOL {
            return Err(TradeFailure::invalid_order(
                "지원하지 않는 상품의 주문입니다",
            ));
        }
        if !(1..=MAX_TRADE_QUANTITY).contains(&self.quantity) {
            return Err(TradeFailure::invalid_order(
                "주문 수량은 1주 이상 1,000,000주 이하여야 합니다",
            ));
        }

        Ok(())
    }
}

impl TryFrom<TradeOrderRequest> for TradeOrder {
    type Error = TradeFailure;

    fn try_from(request: TradeOrderRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            order_id: OrderId::parse(request.order_id)?,
            account_id: AccountId::parse(request.account_id)?,
            expected_run_revision: request.expected_run_revision,
            expected_state_revision: request.expected_state_revision,
            expected_game_day: request.expected_game_day,
            side: request.side,
            symbol: request.symbol,
            quantity: request.quantity,
        })
    }
}

/// One account-owned LLX position. Valuation is derived from a market close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionState {
    pub account_id: AccountId,
    pub symbol: String,
    pub quantity: u32,
    pub cost_basis_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioPosition {
    #[schema(value_type = String)]
    pub account_id: AccountId,
    pub symbol: String,
    pub quantity: u32,
    pub cost_basis_krw: i64,
    pub average_price_krw: i64,
    pub current_price_krw: i64,
    pub market_value_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Portfolio {
    pub positions: Vec<PortfolioPosition>,
    pub market_value_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TradeExecution {
    pub order_id: String,
    #[schema(value_type = String)]
    pub account_id: AccountId,
    pub symbol: String,
    pub side: OrderSide,
    pub quantity: u32,
    pub price_krw: i64,
    pub gross_amount_krw: i64,
    pub fee_krw: i64,
    pub tax_krw: i64,
    pub removed_cost_basis_krw: i64,
    pub realized_gain_loss_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TradeFailureCode {
    InvalidOrder,
    CharacterRequired,
    AccountNotFound,
    AccountClosed,
    AccountTypeNotAllowed,
    MarketClosed,
    InsufficientAccountCash,
    InsufficientQuantity,
    PositionLimit,
    IdempotencyConflict,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TradeFailure {
    pub code: TradeFailureCode,
    pub message: &'static str,
}

impl TradeFailure {
    pub const fn invalid_order(message: &'static str) -> Self {
        Self {
            code: TradeFailureCode::InvalidOrder,
            message,
        }
    }

    pub const fn character_required() -> Self {
        Self {
            code: TradeFailureCode::CharacterRequired,
            message: "먼저 캐릭터를 만들어야 합니다",
        }
    }

    pub const fn market_closed() -> Self {
        Self {
            code: TradeFailureCode::MarketClosed,
            message: "휴장일에는 주문할 수 없습니다",
        }
    }

    pub const fn account_not_found() -> Self {
        Self {
            code: TradeFailureCode::AccountNotFound,
            message: "계좌를 찾을 수 없습니다",
        }
    }

    pub const fn account_closed() -> Self {
        Self {
            code: TradeFailureCode::AccountClosed,
            message: "닫힌 계좌에서는 주문할 수 없습니다",
        }
    }

    pub const fn account_type_not_allowed() -> Self {
        Self {
            code: TradeFailureCode::AccountTypeNotAllowed,
            message: "이 계좌에서는 해당 상품을 거래할 수 없습니다",
        }
    }

    pub const fn insufficient_account_cash() -> Self {
        Self {
            code: TradeFailureCode::InsufficientAccountCash,
            message: "주문에 필요한 계좌 현금이 부족합니다",
        }
    }

    pub const fn insufficient_quantity() -> Self {
        Self {
            code: TradeFailureCode::InsufficientQuantity,
            message: "매도할 보유수량이 부족합니다",
        }
    }

    pub const fn position_limit() -> Self {
        Self {
            code: TradeFailureCode::PositionLimit,
            message: "주문 후 허용된 보유 한도를 초과합니다",
        }
    }

    pub const fn idempotency_conflict() -> Self {
        Self {
            code: TradeFailureCode::IdempotencyConflict,
            message: "같은 주문 ID가 다른 주문에 사용되었습니다",
        }
    }

    pub const fn busy() -> Self {
        Self {
            code: TradeFailureCode::Busy,
            message: "게임 상태가 변경되었습니다. 최신 상태에서 다시 주문하세요",
        }
    }
}

impl Display for TradeFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message)
    }
}

impl Error for TradeFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingMathError {
    InvalidPosition,
    NonPositivePrice,
    ArithmeticOverflow,
}

impl Display for TradingMathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPosition => formatter.write_str("stored position is invalid"),
            Self::NonPositivePrice => formatter.write_str("market price must be positive"),
            Self::ArithmeticOverflow => formatter.write_str("trading arithmetic overflowed"),
        }
    }
}

impl Error for TradingMathError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TradeMutation {
    pub account_id: AccountId,
    pub account_cash_krw: i64,
    pub position: Option<PositionState>,
    pub gross_amount_krw: i64,
    pub fee_krw: i64,
    pub tax_krw: i64,
    pub removed_cost_basis_krw: i64,
    pub realized_gain_loss_krw: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct TradeCharges {
    pub fee_krw: i64,
    pub tax_krw: i64,
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

fn parse_resource_id(raw: &str) -> Option<u64> {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes[0] == b'0' || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }

    raw.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given_request(order_id: &str) -> TradeOrderRequest {
        TradeOrderRequest {
            order_id: order_id.to_owned(),
            account_id: "7".to_owned(),
            expected_run_revision: 3,
            expected_state_revision: 42,
            expected_game_day: 17,
            side: OrderSide::Buy,
            symbol: LLX_SYMBOL.to_owned(),
            quantity: 10,
        }
    }

    mod context_an_order_uses_a_canonical_account_id {
        use super::*;

        #[test]
        fn given_the_largest_u64_decimal_when_validated_then_the_exact_id_is_preserved() {
            let mut request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            request.account_id = u64::MAX.to_string();

            let order = TradeOrder::try_from(request).expect("u64 최댓값 계좌 ID여야 한다");

            assert_eq!(order.account_id.get(), u64::MAX);
        }

        #[test]
        fn given_zero_when_validated_then_the_account_id_is_rejected() {
            let mut request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            request.account_id = "0".to_owned();

            let failure = TradeOrder::try_from(request).expect_err("0은 리소스 ID가 아니다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }

        #[test]
        fn given_a_leading_zero_when_validated_then_the_account_id_is_rejected() {
            let mut request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            request.account_id = "07".to_owned();

            let failure =
                TradeOrder::try_from(request).expect_err("leading zero는 canonical이 아니다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }

        #[test]
        fn given_a_value_above_u64_when_validated_then_the_account_id_is_rejected() {
            let mut request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            request.account_id = "18446744073709551616".to_owned();

            let failure = TradeOrder::try_from(request).expect_err("u64 범위를 넘어야 한다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }

        #[test]
        fn given_a_non_decimal_character_when_validated_then_the_account_id_is_rejected() {
            let mut request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            request.account_id = "+7".to_owned();

            let failure = TradeOrder::try_from(request).expect_err("10진 숫자만 허용해야 한다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }
    }

    mod context_an_account_id_is_sent_to_the_api {
        use super::*;

        #[test]
        fn given_an_account_id_when_serialized_in_outputs_then_it_is_a_decimal_string() {
            let account_id = AccountId::from_u64(u64::MAX).expect("0이 아닌 ID여야 한다");
            let position = PortfolioPosition {
                account_id,
                symbol: LLX_SYMBOL.to_owned(),
                quantity: 1,
                cost_basis_krw: 100,
                average_price_krw: 100,
                current_price_krw: 100,
                market_value_krw: 100,
            };
            let execution = TradeExecution {
                order_id: "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2".to_owned(),
                account_id,
                symbol: LLX_SYMBOL.to_owned(),
                side: OrderSide::Buy,
                quantity: 1,
                price_krw: 100,
                gross_amount_krw: 100,
                fee_krw: 0,
                tax_krw: 0,
                removed_cost_basis_krw: 0,
                realized_gain_loss_krw: 0,
                replayed: false,
            };

            let position_json = serde_json::to_value(position).expect("포지션을 직렬화해야 한다");
            let execution_json = serde_json::to_value(execution).expect("체결을 직렬화해야 한다");

            assert_eq!(
                [
                    position_json["accountId"].clone(),
                    execution_json["accountId"].clone(),
                ],
                [
                    serde_json::Value::from(u64::MAX.to_string()),
                    serde_json::Value::from(u64::MAX.to_string()),
                ]
            );
        }
    }

    mod context_an_order_uses_a_canonical_uuid {
        use super::*;

        #[test]
        fn given_a_lowercase_hyphenated_uuid_when_validated_then_it_is_accepted() {
            let request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");

            let order = TradeOrder::try_from(request).expect("canonical UUID여야 한다");

            assert_eq!(
                order.order_id.as_str(),
                "4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2"
            );
        }

        #[test]
        fn given_an_uppercase_uuid_when_validated_then_it_is_rejected() {
            let request = given_request("4F521F4C-9DD8-4D20-8E1F-15CB13CBE0F2");

            let failure = TradeOrder::try_from(request).expect_err("canonical UUID가 아니다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }
    }

    mod context_an_order_has_an_unsupported_shape {
        use super::*;

        #[test]
        fn given_an_unknown_symbol_when_validated_then_it_is_rejected() {
            let mut request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            request.symbol = "USD".to_owned();
            let order = TradeOrder::try_from(request)
                .expect("식별자 문법이 유효하면 의미 검증까지 전달되어야 한다");

            let failure = order.validate().expect_err("LLX만 허용해야 한다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }

        #[test]
        fn given_zero_quantity_when_validated_then_it_is_rejected() {
            let mut request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            request.quantity = 0;
            let order = TradeOrder::try_from(request)
                .expect("식별자 문법이 유효하면 의미 검증까지 전달되어야 한다");

            let failure = order.validate().expect_err("0주는 허용하지 않아야 한다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }

        #[test]
        fn given_quantity_above_the_order_limit_when_validated_then_it_is_rejected() {
            let mut request = given_request("4f521f4c-9dd8-4d20-8e1f-15cb13cbe0f2");
            request.quantity = MAX_TRADE_QUANTITY + 1;
            let order = TradeOrder::try_from(request)
                .expect("식별자 문법이 유효하면 의미 검증까지 전달되어야 한다");

            let failure = order.validate().expect_err("주문 한도를 넘어야 한다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }
    }

    mod context_the_trade_failure_contract_is_serialized {
        use super::*;

        #[test]
        fn given_all_fixed_codes_when_serialized_then_their_camel_case_values_do_not_change() {
            let codes = [
                TradeFailureCode::InvalidOrder,
                TradeFailureCode::CharacterRequired,
                TradeFailureCode::AccountNotFound,
                TradeFailureCode::AccountClosed,
                TradeFailureCode::AccountTypeNotAllowed,
                TradeFailureCode::MarketClosed,
                TradeFailureCode::InsufficientAccountCash,
                TradeFailureCode::InsufficientQuantity,
                TradeFailureCode::PositionLimit,
                TradeFailureCode::IdempotencyConflict,
                TradeFailureCode::Busy,
            ];

            let serialized = codes
                .iter()
                .map(|code| serde_json::to_value(code).expect("직렬화할 수 있어야 한다"))
                .collect::<Vec<_>>();

            assert_eq!(
                serialized,
                [
                    "invalidOrder",
                    "characterRequired",
                    "accountNotFound",
                    "accountClosed",
                    "accountTypeNotAllowed",
                    "marketClosed",
                    "insufficientAccountCash",
                    "insufficientQuantity",
                    "positionLimit",
                    "idempotencyConflict",
                    "busy",
                ]
                .map(serde_json::Value::from)
            );
        }
    }
}
