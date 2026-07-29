use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const MAX_EQUITY_SEARCH_LIMIT: u8 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum EquityMarket {
    Kospi,
    Kosdaq,
    Konex,
    Other,
}

impl EquityMarket {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kospi => "kospi",
            Self::Kosdaq => "kosdaq",
            Self::Konex => "konex",
            Self::Other => "other",
        }
    }

    pub fn from_source(value: &str) -> Self {
        let normalized = value.trim().to_ascii_uppercase();
        if normalized.contains("KOSPI") || normalized.contains("유가증권") {
            Self::Kospi
        } else if normalized.contains("KOSDAQ") || normalized.contains("코스닥") {
            Self::Kosdaq
        } else if normalized.contains("KONEX") || normalized.contains("코넥스") {
            Self::Konex
        } else {
            Self::Other
        }
    }
}

impl std::str::FromStr for EquityMarket {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "kospi" => Ok(Self::Kospi),
            "kosdaq" => Ok(Self::Kosdaq),
            "konex" => Ok(Self::Konex),
            "other" => Ok(Self::Other),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquityInstrumentInput {
    pub isin: String,
    pub short_code: String,
    pub market: EquityMarket,
    pub display_name: String,
    pub corporation_name: String,
    pub corporation_registration_number: Option<String>,
    pub dart_corp_code: Option<String>,
    pub industry_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquityCatalogInput {
    pub source_as_of: String,
    pub instruments: Vec<EquityInstrumentInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquitySearchQuery {
    pub text: String,
    pub market: Option<EquityMarket>,
    pub limit: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquitySearchItem {
    pub isin: String,
    pub short_code: String,
    pub market: EquityMarket,
    pub display_name: String,
    pub corporation_name: String,
    pub dart_corp_code: Option<String>,
    pub industry_code: Option<String>,
    pub tradable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum EquityCatalogAvailability {
    Available,
    NotSynced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquitySearchResult {
    pub availability: EquityCatalogAvailability,
    pub catalog_version: Option<String>,
    pub source_as_of: Option<String>,
    pub source: Option<String>,
    pub simulation_notice: String,
    pub items: Vec<EquitySearchItem>,
}

impl EquitySearchResult {
    pub fn not_synced() -> Self {
        Self {
            availability: EquityCatalogAvailability::NotSynced,
            catalog_version: None,
            source_as_of: None,
            source: None,
            simulation_notice: simulation_notice().to_owned(),
            items: Vec::new(),
        }
    }
}

pub fn simulation_notice() -> &'static str {
    "실제 종목 식별자를 사용하지만 게임 가격은 실제 시세가 아닌 시뮬레이션 값입니다."
}

pub fn normalize_search_text(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let character_count = normalized.chars().count();
    (character_count > 0 && character_count <= 80).then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_market_name_is_received_from_a_provider {
        use super::*;

        #[test]
        fn given_korean_krx_market_when_normalized_then_maps_to_market_enum() {
            let market = EquityMarket::from_source("유가증권시장");

            assert_eq!(market, EquityMarket::Kospi);
        }
    }

    mod context_search_text_contains_repeated_whitespace {
        use super::*;

        #[test]
        fn given_repeated_whitespace_when_search_normalized_then_collapses_whitespace() {
            let normalized = normalize_search_text("  삼성   전자  ");

            assert_eq!(normalized.as_deref(), Some("삼성 전자"));
        }
    }
}
