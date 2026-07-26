//! Strict M2-D asset command, catalog, receipt, and bounded snapshot contracts (§9.4).

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::{Date, Month};

use super::super::{CommandCursor, CommandId, FinanceFailureCode, ResourceId};

pub const MAX_BOND_CATALOG_PRODUCTS: usize = 2;
pub const MAX_BOND_CATALOG_SERIES: usize = 160;
pub const MAX_PENDING_LLX_ENTITLEMENTS: usize = 8;
pub const MAX_BOND_POSITION_SNAPSHOTS: usize = 640;
pub const MAX_GOLD_ACCOUNT_SNAPSHOTS: usize = 1;
pub const MAX_PHYSICAL_GOLD_HOLDINGS: usize = 2;
pub const MAX_BOND_ORDER_UNITS: u32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum M2dContractError {
    InvalidDate,
    InvalidQuantity,
    InvalidAmount,
    InvalidCatalog,
    SnapshotLimitExceeded,
}

impl Display for M2dContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidDate => "date must use canonical YYYY-MM-DD form",
            Self::InvalidQuantity => "asset quantity is outside its contract range",
            Self::InvalidAmount => "asset amount violates its non-negative contract",
            Self::InvalidCatalog => "asset catalog violates its sealed shape",
            Self::SnapshotLimitExceeded => "asset snapshot exceeds its bounded contract",
        };
        formatter.write_str(message)
    }
}

impl Error for M2dContractError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, utoipa::ToSchema)]
#[schema(value_type = String, format = Date)]
pub struct CanonicalDate(Date);

impl CanonicalDate {
    pub fn parse(raw: &str) -> Result<Self, M2dContractError> {
        let bytes = raw.as_bytes();
        if bytes.len() != 10
            || bytes.get(4) != Some(&b'-')
            || bytes.get(7) != Some(&b'-')
            || bytes
                .iter()
                .enumerate()
                .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
        {
            return Err(M2dContractError::InvalidDate);
        }

        let year = raw
            .get(0..4)
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|value| (1..=9_999).contains(value))
            .ok_or(M2dContractError::InvalidDate)?;
        let month = raw
            .get(5..7)
            .and_then(|value| value.parse::<u8>().ok())
            .and_then(|value| Month::try_from(value).ok())
            .ok_or(M2dContractError::InvalidDate)?;
        let day = raw
            .get(8..10)
            .and_then(|value| value.parse::<u8>().ok())
            .ok_or(M2dContractError::InvalidDate)?;
        Date::from_calendar_date(year, month, day)
            .map(Self)
            .map_err(|_| M2dContractError::InvalidDate)
    }

    pub const fn from_date(date: Date) -> Self {
        Self(date)
    }

    pub const fn as_date(self) -> Date {
        self.0
    }
}

impl Display for CanonicalDate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.0.year(),
            u8::from(self.0.month()),
            self.0.day()
        )
    }
}

impl Serialize for CanonicalDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CanonicalDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum AssetOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum M2dAccountType {
    KrxGold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum GoldUnit {
    Gram,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BondOrderCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_id: ResourceId,
    pub series_id: ResourceId,
    pub side: AssetOrderSide,
    pub bond_units: u32,
}

impl BondOrderCommand {
    pub fn validate(&self) -> Result<(), M2dContractError> {
        if !(1..=MAX_BOND_ORDER_UNITS).contains(&self.bond_units) {
            return Err(M2dContractError::InvalidQuantity);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenGoldAccountCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    #[serde(rename = "type")]
    pub account_type: M2dAccountType,
    pub product_version_id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldOrderCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_id: ResourceId,
    pub side: AssetOrderSide,
    pub quantity_gram: u32,
}

impl GoldOrderCommand {
    pub fn validate(&self) -> Result<(), M2dContractError> {
        if self.quantity_gram == 0 {
            return Err(M2dContractError::InvalidQuantity);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldWithdrawalCommand {
    pub command_id: CommandId,
    pub cursor: CommandCursor,
    pub account_id: ResourceId,
    pub bar_size_gram: u32,
    pub bar_count: u32,
}

impl GoldWithdrawalCommand {
    pub fn validate(&self) -> Result<(), M2dContractError> {
        if !matches!(self.bar_size_gram, 100 | 1_000) || self.bar_count == 0 {
            return Err(M2dContractError::InvalidQuantity);
        }
        self.bar_size_gram
            .checked_mul(self.bar_count)
            .ok_or(M2dContractError::InvalidQuantity)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BondProductCatalogItem {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
    pub term_years: u8,
    pub face_value_krw: i64,
    pub max_order_units: u32,
    pub max_position_units: u32,
    pub buy_fee_ppm: i64,
    pub sell_fee_ppm: i64,
}

impl BondProductCatalogItem {
    fn validate(&self) -> Result<(), M2dContractError> {
        if !matches!(self.term_years, 3 | 10)
            || self.key.is_empty()
            || self.display_name.is_empty()
            || self.face_value_krw <= 0
            || self.max_order_units == 0
            || self.max_position_units == 0
            || self.max_order_units > self.max_position_units
            || !valid_rate(self.buy_fee_ppm)
            || !valid_rate(self.sell_fee_ppm)
        {
            return Err(M2dContractError::InvalidCatalog);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BondSeriesCatalogItem {
    pub id: ResourceId,
    pub product_version_id: ResourceId,
    pub issued_date: CanonicalDate,
    pub maturity_date: CanonicalDate,
    pub coupon_rate_bp: i32,
    pub issue_yield_bp: i32,
    pub next_coupon_date: CanonicalDate,
    pub dirty_price_krw: i64,
    pub current_yield_bp: i32,
}

impl BondSeriesCatalogItem {
    fn validate(&self) -> Result<(), M2dContractError> {
        if self.maturity_date <= self.issued_date
            || self.next_coupon_date <= self.issued_date
            || self.next_coupon_date > self.maturity_date
            || self.coupon_rate_bp < 0
            || self.issue_yield_bp < 0
            || self.current_yield_bp < 0
            || self.dirty_price_krw <= 0
        {
            return Err(M2dContractError::InvalidCatalog);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BondCatalog {
    pub market_version: String,
    pub products: Vec<BondProductCatalogItem>,
    pub series: Vec<BondSeriesCatalogItem>,
}

impl BondCatalog {
    pub fn validate(&self) -> Result<(), M2dContractError> {
        if self.market_version.is_empty()
            || (!self.products.is_empty() && self.products.len() != MAX_BOND_CATALOG_PRODUCTS)
            || self.series.len() > MAX_BOND_CATALOG_SERIES
            || (self.products.is_empty() && !self.series.is_empty())
        {
            return Err(M2dContractError::InvalidCatalog);
        }
        for product in &self.products {
            product.validate()?;
        }
        for series in &self.series {
            series.validate()?;
            if !self
                .products
                .iter()
                .any(|product| product.id == series.product_version_id)
            {
                return Err(M2dContractError::InvalidCatalog);
            }
        }
        if self.products.len() == 2
            && (self.products[0].term_years != 3 || self.products[1].term_years != 10)
        {
            return Err(M2dContractError::InvalidCatalog);
        }
        if self.products.len() == 2 && self.products[0].id == self.products[1].id {
            return Err(M2dContractError::InvalidCatalog);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldWithdrawalBar {
    pub bar_size_gram: u32,
    pub fee_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldProductCatalogItem {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
    pub unit: GoldUnit,
    pub buy_fee_ppm: i64,
    pub sell_fee_ppm: i64,
    pub buy_tax_ppm: i64,
    pub sell_tax_ppm: i64,
    pub withdrawal_bars: [GoldWithdrawalBar; 2],
}

impl GoldProductCatalogItem {
    fn validate(&self) -> Result<(), M2dContractError> {
        if self.key.is_empty()
            || self.display_name.is_empty()
            || !valid_rate(self.buy_fee_ppm)
            || !valid_rate(self.sell_fee_ppm)
            || !valid_rate(self.buy_tax_ppm)
            || !valid_rate(self.sell_tax_ppm)
            || self.withdrawal_bars[0].bar_size_gram != 100
            || self.withdrawal_bars[1].bar_size_gram != 1_000
            || self.withdrawal_bars.iter().any(|bar| bar.fee_krw < 0)
        {
            return Err(M2dContractError::InvalidCatalog);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldCatalog {
    pub market_version: String,
    pub products: Vec<GoldProductCatalogItem>,
}

impl GoldCatalog {
    pub fn validate(&self) -> Result<(), M2dContractError> {
        if self.market_version.is_empty() || self.products.len() > MAX_GOLD_ACCOUNT_SNAPSHOTS {
            return Err(M2dContractError::InvalidCatalog);
        }
        for product in &self.products {
            product.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BondOrderReceipt {
    pub command_id: CommandId,
    pub execution_id: ResourceId,
    pub account_id: ResourceId,
    pub series_id: ResourceId,
    pub side: AssetOrderSide,
    pub bond_units: u32,
    pub dirty_price_krw: i64,
    pub gross_amount_krw: i64,
    pub fee_krw: i64,
    pub tax_krw: i64,
    pub removed_cost_basis_krw: i64,
    pub realized_gain_loss_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenGoldAccountReceipt {
    pub command_id: CommandId,
    pub account_id: ResourceId,
    #[serde(rename = "type")]
    pub account_type: M2dAccountType,
    pub product_version_id: ResourceId,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldOrderReceipt {
    pub command_id: CommandId,
    pub execution_id: ResourceId,
    pub account_id: ResourceId,
    pub side: AssetOrderSide,
    pub quantity_gram: u32,
    pub price_krw_per_gram: i64,
    pub gross_amount_krw: i64,
    pub fee_krw: i64,
    pub tax_krw: i64,
    pub removed_cost_basis_krw: i64,
    pub realized_gain_loss_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldWithdrawalReceipt {
    pub command_id: CommandId,
    pub withdrawal_id: ResourceId,
    pub account_id: ResourceId,
    pub bar_size_gram: u32,
    pub bar_count: u32,
    pub quantity_gram: u32,
    pub removed_cost_basis_krw: i64,
    pub vat_krw: i64,
    pub fee_krw: i64,
    pub cash_charged_krw: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexProductSnapshot {
    pub id: ResourceId,
    pub key: String,
    pub display_name: String,
    pub annual_management_fee_ppm: i64,
    pub annual_distribution_rate_ppm: i64,
    pub day_count_denominator: u32,
    pub buy_fee_ppm: i64,
    pub sell_fee_ppm: i64,
    pub sell_tax_ppm: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductBundleSnapshot {
    pub index_product: IndexProductSnapshot,
    pub bond_product_version_ids: [ResourceId; 2],
    pub gold_product_version_id: ResourceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PendingEntitlementStatus {
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlxDistributionEntitlementSnapshot {
    pub id: ResourceId,
    pub account_id: ResourceId,
    pub record_date: CanonicalDate,
    pub payment_date: CanonicalDate,
    pub quantity: u32,
    pub gross_amount_krw: i64,
    pub status: PendingEntitlementStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BondPositionSnapshot {
    pub account_id: ResourceId,
    pub series_id: ResourceId,
    pub bond_units: u32,
    pub total_cost_basis_krw: i64,
    pub dirty_price_krw: i64,
    pub market_value_krw: i64,
    pub unrealized_gain_loss_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldAccountSnapshot {
    pub account_id: ResourceId,
    pub product_version_id: ResourceId,
    pub quantity_gram: u32,
    pub total_cost_basis_krw: i64,
    pub average_cost_krw_per_gram: Option<i64>,
    pub close_krw_per_gram: i64,
    pub market_value_krw: i64,
    pub unrealized_gain_loss_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhysicalGoldHoldingSnapshot {
    pub bar_size_gram: u32,
    pub bar_count: u32,
    pub total_quantity_gram: u32,
    pub close_krw_per_gram: i64,
    pub market_value_krw: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct M2dAssetSnapshot {
    pub product_bundle: Option<ProductBundleSnapshot>,
    pub llx_distribution_entitlements: Vec<LlxDistributionEntitlementSnapshot>,
    pub bond_positions: Vec<BondPositionSnapshot>,
    pub gold_accounts: Vec<GoldAccountSnapshot>,
    pub physical_gold_holdings: Vec<PhysicalGoldHoldingSnapshot>,
}

impl M2dAssetSnapshot {
    pub fn validate(&self) -> Result<(), M2dContractError> {
        if self.llx_distribution_entitlements.len() > MAX_PENDING_LLX_ENTITLEMENTS
            || self.bond_positions.len() > MAX_BOND_POSITION_SNAPSHOTS
            || self.gold_accounts.len() > MAX_GOLD_ACCOUNT_SNAPSHOTS
            || self.physical_gold_holdings.len() > MAX_PHYSICAL_GOLD_HOLDINGS
        {
            return Err(M2dContractError::SnapshotLimitExceeded);
        }
        if let Some(bundle) = &self.product_bundle
            && (bundle.index_product.key.is_empty()
                || bundle.index_product.display_name.is_empty()
                || bundle.index_product.day_count_denominator == 0
                || !valid_rate(bundle.index_product.annual_management_fee_ppm)
                || !valid_rate(bundle.index_product.annual_distribution_rate_ppm)
                || !valid_rate(bundle.index_product.buy_fee_ppm)
                || !valid_rate(bundle.index_product.sell_fee_ppm)
                || !valid_rate(bundle.index_product.sell_tax_ppm)
                || bundle.bond_product_version_ids[0] == bundle.bond_product_version_ids[1])
        {
            return Err(M2dContractError::InvalidCatalog);
        }
        for entitlement in &self.llx_distribution_entitlements {
            if entitlement.quantity == 0
                || entitlement.gross_amount_krw < 0
                || entitlement.payment_date <= entitlement.record_date
            {
                return Err(M2dContractError::InvalidAmount);
            }
        }
        for position in &self.bond_positions {
            if position.bond_units == 0
                || position.total_cost_basis_krw <= 0
                || position.dirty_price_krw <= 0
                || position.market_value_krw <= 0
            {
                return Err(M2dContractError::InvalidAmount);
            }
        }
        for account in &self.gold_accounts {
            let empty = account.quantity_gram == 0
                && account.total_cost_basis_krw == 0
                && account.average_cost_krw_per_gram.is_none()
                && account.market_value_krw == 0
                && account.unrealized_gain_loss_krw == 0;
            let holding = account.quantity_gram > 0
                && account.total_cost_basis_krw > 0
                && account
                    .average_cost_krw_per_gram
                    .is_some_and(|value| value > 0)
                && account.market_value_krw > 0;
            if account.close_krw_per_gram <= 0 || (!empty && !holding) {
                return Err(M2dContractError::InvalidAmount);
            }
        }
        for holding in &self.physical_gold_holdings {
            if !matches!(holding.bar_size_gram, 100 | 1_000)
                || holding.bar_count == 0
                || holding.close_krw_per_gram <= 0
                || holding.market_value_krw <= 0
                || holding.bar_size_gram.checked_mul(holding.bar_count)
                    != Some(holding.total_quantity_gram)
            {
                return Err(M2dContractError::InvalidAmount);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BondOrderResponse {
    pub bond_order: BondOrderReceipt,
    pub snapshot: M2dAssetSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenGoldAccountResponse {
    pub account: OpenGoldAccountReceipt,
    pub snapshot: M2dAssetSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldOrderResponse {
    pub gold_order: GoldOrderReceipt,
    pub snapshot: M2dAssetSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoldWithdrawalResponse {
    pub gold_withdrawal: GoldWithdrawalReceipt,
    pub snapshot: M2dAssetSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum M2dAssetCommandResult<T> {
    Applied(T),
    Rejected(FinanceFailureCode),
}

const fn valid_rate(rate_ppm: i64) -> bool {
    rate_ppm >= 0 && rate_ppm <= 1_000_000
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn command_id() -> &'static str {
        "11111111-1111-4111-8111-111111111111"
    }

    fn cursor_json() -> serde_json::Value {
        json!({
            "expectedRunRevision": 1,
            "expectedStateRevision": 2,
            "expectedGameDay": 3
        })
    }

    mod canonical_date_rule {
        use super::*;

        mod context_strict_calendar_text {
            use super::*;

            #[test]
            fn given_canonical_leap_date_when_parsed_then_round_trip_is_exact() {
                let parsed =
                    CanonicalDate::parse("2028-02-29").expect("유효한 윤년 날짜를 해석해야 한다");

                let serialized =
                    serde_json::to_string(&parsed).expect("canonical 날짜를 직렬화해야 한다");

                assert_eq!(serialized, "\"2028-02-29\"");
            }

            #[test]
            fn given_non_canonical_or_invalid_date_when_parsed_then_error_is_returned() {
                let short = CanonicalDate::parse("2026-7-01");
                let invalid = CanonicalDate::parse("2026-02-29");

                assert_eq!(short, Err(M2dContractError::InvalidDate));
                assert_eq!(invalid, Err(M2dContractError::InvalidDate));
            }
        }
    }

    mod strict_command_rule {
        use super::*;

        mod context_bond_order_shape {
            use super::*;

            #[test]
            fn given_unknown_field_when_deserialized_then_command_is_rejected() {
                let value = json!({
                    "commandId": command_id(),
                    "cursor": cursor_json(),
                    "accountId": "1",
                    "seriesId": "2",
                    "side": "buy",
                    "bondUnits": 1,
                    "unexpected": true
                });

                let result = serde_json::from_value::<BondOrderCommand>(value);

                assert!(result.is_err());
            }

            #[test]
            fn given_non_canonical_resource_id_when_deserialized_then_command_is_rejected() {
                let value = json!({
                    "commandId": command_id(),
                    "cursor": cursor_json(),
                    "accountId": "01",
                    "seriesId": "2",
                    "side": "buy",
                    "bondUnits": 1
                });

                let result = serde_json::from_value::<BondOrderCommand>(value);

                assert!(result.is_err());
            }

            #[test]
            fn given_zero_or_above_limit_units_when_validated_then_quantity_error_is_returned() {
                let base = BondOrderCommand {
                    command_id: CommandId::parse(command_id()).expect("유효한 command ID여야 한다"),
                    cursor: CommandCursor {
                        expected_run_revision: 1,
                        expected_state_revision: 2,
                        expected_game_day: 3,
                    },
                    account_id: ResourceId::from_u64(1),
                    series_id: ResourceId::from_u64(2),
                    side: AssetOrderSide::Buy,
                    bond_units: 0,
                };
                let above = BondOrderCommand {
                    bond_units: 100_001,
                    ..base.clone()
                };

                assert_eq!(base.validate(), Err(M2dContractError::InvalidQuantity));
                assert_eq!(above.validate(), Err(M2dContractError::InvalidQuantity));
            }
        }

        mod context_gold_withdrawal_shape {
            use super::*;

            #[test]
            fn given_unsupported_bar_when_validated_then_quantity_error_is_returned() {
                let command = GoldWithdrawalCommand {
                    command_id: CommandId::parse(command_id()).expect("유효한 command ID여야 한다"),
                    cursor: CommandCursor {
                        expected_run_revision: 1,
                        expected_state_revision: 2,
                        expected_game_day: 3,
                    },
                    account_id: ResourceId::from_u64(1),
                    bar_size_gram: 500,
                    bar_count: 1,
                };

                let result = command.validate();

                assert_eq!(result, Err(M2dContractError::InvalidQuantity));
            }
        }
    }

    mod bounded_snapshot_rule {
        use super::*;

        mod context_snapshot_array_limit {
            use super::*;

            #[test]
            fn given_more_than_eight_pending_entitlements_when_validated_then_limit_error_is_returned()
             {
                let entitlement = LlxDistributionEntitlementSnapshot {
                    id: ResourceId::from_u64(1),
                    account_id: ResourceId::from_u64(2),
                    record_date: CanonicalDate::parse("2026-03-31")
                        .expect("기준일이 유효해야 한다"),
                    payment_date: CanonicalDate::parse("2026-04-02")
                        .expect("지급일이 유효해야 한다"),
                    quantity: 1,
                    gross_amount_krw: 500,
                    status: PendingEntitlementStatus::Pending,
                };
                let snapshot = M2dAssetSnapshot {
                    llx_distribution_entitlements: vec![entitlement; 9],
                    ..M2dAssetSnapshot::default()
                };

                let result = snapshot.validate();

                assert_eq!(result, Err(M2dContractError::SnapshotLimitExceeded));
            }
        }

        mod context_fixed_catalog_shape {
            use super::*;

            #[test]
            fn given_one_withdrawal_bar_when_deserialized_then_catalog_is_rejected() {
                let value = json!({
                    "marketVersion": "m2-2026-v4",
                    "products": [{
                        "id": "1",
                        "key": "krx-gold-2026-v1",
                        "displayName": "KRX 금시장 금 1g",
                        "unit": "gram",
                        "buyFeePpm": 0,
                        "sellFeePpm": 0,
                        "buyTaxPpm": 0,
                        "sellTaxPpm": 0,
                        "withdrawalBars": [{"barSizeGram": 100, "feeKrw": 20000}]
                    }]
                });

                let result = serde_json::from_value::<GoldCatalog>(value);

                assert!(result.is_err());
            }

            #[test]
            fn given_only_one_bond_product_when_validated_then_catalog_is_rejected() {
                let catalog = BondCatalog {
                    market_version: "m2-2026-v4".to_owned(),
                    products: vec![BondProductCatalogItem {
                        id: ResourceId::from_u64(1),
                        key: "kr-government-bond-3y-2026-v1".to_owned(),
                        display_name: "대한민국 국고채 3년".to_owned(),
                        term_years: 3,
                        face_value_krw: 10_000,
                        max_order_units: 100_000,
                        max_position_units: 100_000,
                        buy_fee_ppm: 0,
                        sell_fee_ppm: 0,
                    }],
                    series: Vec::new(),
                };

                let result = catalog.validate();

                assert_eq!(result, Err(M2dContractError::InvalidCatalog));
            }
        }
    }
}
