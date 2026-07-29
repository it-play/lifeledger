use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::time::Duration;

use anyhow::{Context, ensure};
use futures_util::stream::{self, StreamExt, TryStreamExt};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use super::config::{EcosDataset, KosisDataset, MarketDataConfig, Secret};
use super::types::{EquityCatalogInput, EquityInstrumentInput, EquityMarket};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DATA_GO_PAGE_SIZE: u32 = 1_000;
const DATA_GO_LISTED_URL: &str =
    "https://apis.data.go.kr/1160100/service/GetKrxListedInfoService/getItemInfo";
const DATA_GO_STOCK_PRICE_URL: &str =
    "https://apis.data.go.kr/1160100/service/GetStockSecuritiesInfoService/getStockPriceInfo";
const OPEN_DART_CORP_CODE_URL: &str = "https://opendart.fss.or.kr/api/corpCode.xml";
const OPEN_DART_COMPANY_URL: &str = "https://opendart.fss.or.kr/api/company.json";
const OPEN_DART_CONCURRENCY: usize = 4;

pub struct MarketDataProviders {
    client: Client,
    config: MarketDataConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderObservation {
    pub provider: &'static str,
    pub dataset: &'static str,
    pub status: ProviderObservationStatus,
    pub row_count: u32,
    pub content_sha256: Option<String>,
    pub source_as_of: Option<String>,
    pub failure_code: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DartCompanyMetadata {
    pub corp_code: String,
    pub industry_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderObservationStatus {
    Completed,
    NotConfigured,
    Failed,
}

impl ProviderObservationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::NotConfigured => "notConfigured",
            Self::Failed => "failed",
        }
    }
}

impl MarketDataProviders {
    pub fn from_env() -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("LifeLedger-market-data-sync/1")
            .build()
            .context("failed to create the market-data HTTP client")?;
        Ok(Self {
            client,
            config: MarketDataConfig::from_env()?,
        })
    }

    pub async fn load_catalog(&self) -> anyhow::Result<EquityCatalogInput> {
        let key = self.config.require_data_go_key()?;
        load_data_go_catalog(&self.client, key).await
    }

    pub async fn load_dart_companies(
        &self,
    ) -> anyhow::Result<Option<HashMap<String, DartCompanyMetadata>>> {
        match &self.config.open_dart_api_key {
            Some(key) => load_open_dart_companies(&self.client, key).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn observe_stock_prices(&self, source_as_of: &str) -> ProviderObservation {
        let Some(key) = self.config.data_go_kr_service_key.as_ref() else {
            return failed("dataGoKr", "krxStockPrices", "notConfigured");
        };
        observe_data_go_stock_prices(&self.client, key, source_as_of).await
    }

    pub async fn observe_optional_sources(
        &self,
        catalog: &EquityCatalogInput,
    ) -> Vec<ProviderObservation> {
        let mut observations = Vec::with_capacity(5);
        observations
            .push(observe_krx(&self.client, self.config.krx_auth_key.as_ref(), catalog).await);
        observations.push(observe_fmp(&self.client, self.config.fmp_api_key.as_ref()).await);
        observations.push(
            observe_ecos(
                &self.client,
                self.config.ecos_api_key.as_ref(),
                self.config.ecos_dataset.as_ref(),
            )
            .await,
        );
        observations.push(
            observe_kosis(
                &self.client,
                self.config.kosis_api_key.as_ref(),
                self.config.kosis_dataset.as_ref(),
            )
            .await,
        );
        observations
    }
}

#[derive(Deserialize)]
struct DataGoResponse {
    response: DataGoEnvelope,
}

#[derive(Deserialize)]
struct DataGoEnvelope {
    header: DataGoHeader,
    body: Option<DataGoBody>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataGoHeader {
    result_code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataGoBody {
    total_count: u32,
    items: Option<DataGoItems>,
}

#[derive(Deserialize)]
struct DataGoItems {
    #[serde(default)]
    item: Vec<DataGoListedItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataGoListedItem {
    bas_dt: String,
    srtn_cd: String,
    isin_cd: String,
    mrkt_ctg: String,
    itms_nm: String,
    crno: Option<String>,
    corp_nm: String,
}

async fn load_data_go_catalog(client: &Client, key: &Secret) -> anyhow::Result<EquityCatalogInput> {
    let mut page = 1_u32;
    let mut expected_count = None;
    let mut instruments = Vec::new();
    let mut source_as_of = String::new();

    loop {
        let page_value = page.to_string();
        let page_size_value = DATA_GO_PAGE_SIZE.to_string();
        let response = client
            .get(DATA_GO_LISTED_URL)
            .query(&[
                ("serviceKey", key.expose()),
                ("resultType", "json"),
                ("pageNo", page_value.as_str()),
                ("numOfRows", page_size_value.as_str()),
            ])
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("data.go.kr listed catalog request failed"))?;
        ensure!(
            response.status().is_success(),
            "data.go.kr listed catalog returned HTTP {}",
            response.status().as_u16()
        );
        let envelope: DataGoResponse = response
            .json()
            .await
            .map_err(|_| anyhow::anyhow!("data.go.kr listed catalog response was invalid JSON"))?;
        ensure!(
            envelope.response.header.result_code == "00",
            "data.go.kr listed catalog returned an unsuccessful result code"
        );
        let body = envelope
            .response
            .body
            .context("data.go.kr listed catalog response had no body")?;
        let total_count = *expected_count.get_or_insert(body.total_count);
        ensure!(
            body.total_count == total_count,
            "data.go.kr listed catalog count changed during pagination"
        );
        let items = body.items.map(|items| items.item).unwrap_or_default();
        for item in items {
            source_as_of = source_as_of.max(item.bas_dt.clone());
            instruments.push(normalize_data_go_item(item)?);
        }
        if instruments.len() >= total_count as usize {
            break;
        }
        ensure!(
            page < 100,
            "data.go.kr listed catalog exceeded the pagination safety limit"
        );
        page += 1;
    }

    ensure!(
        !instruments.is_empty(),
        "data.go.kr listed catalog was empty"
    );
    ensure!(
        instruments.len() == expected_count.unwrap_or_default() as usize,
        "data.go.kr listed catalog row count did not match totalCount"
    );
    instruments.sort_by(|left, right| left.short_code.cmp(&right.short_code));
    let instrument_count = instruments.len();
    instruments.dedup_by(|left, right| left.short_code == right.short_code);
    ensure!(
        instruments.len() == instrument_count,
        "data.go.kr listed catalog contained duplicate short codes"
    );
    ensure!(
        !source_as_of.is_empty(),
        "data.go.kr catalog had no base date"
    );

    Ok(EquityCatalogInput {
        source_as_of,
        instruments,
    })
}

fn normalize_data_go_item(item: DataGoListedItem) -> anyhow::Result<EquityInstrumentInput> {
    let short_code = item.srtn_cd.trim().to_ascii_uppercase();
    let isin = item.isin_cd.trim().to_ascii_uppercase();
    let display_name = item.itms_nm.trim().to_owned();
    let corporation_name = item.corp_nm.trim().to_owned();
    ensure!(
        (6..=12).contains(&short_code.len())
            && short_code
                .chars()
                .all(|value| value.is_ascii_alphanumeric()),
        "data.go.kr returned an invalid short code"
    );
    ensure!(
        isin.len() == 12 && isin.starts_with("KR") && isin.is_ascii(),
        "data.go.kr returned an invalid ISIN"
    );
    ensure!(
        !display_name.is_empty() && !corporation_name.is_empty(),
        "data.go.kr returned an empty instrument name"
    );
    Ok(EquityInstrumentInput {
        isin,
        short_code,
        market: EquityMarket::from_source(&item.mrkt_ctg),
        display_name,
        corporation_name,
        corporation_registration_number: item
            .crno
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        dart_corp_code: None,
        industry_code: None,
    })
}

async fn observe_data_go_stock_prices(
    client: &Client,
    key: &Secret,
    source_as_of: &str,
) -> ProviderObservation {
    let mut page = 1_u32;
    let mut expected_count = None;
    let mut items = Vec::new();
    loop {
        let page_value = page.to_string();
        let page_size_value = DATA_GO_PAGE_SIZE.to_string();
        let result = client
            .get(DATA_GO_STOCK_PRICE_URL)
            .query(&[
                ("serviceKey", key.expose()),
                ("resultType", "json"),
                ("basDt", source_as_of),
                ("pageNo", page_value.as_str()),
                ("numOfRows", page_size_value.as_str()),
            ])
            .send()
            .await;
        let Ok(response) = result else {
            return failed("dataGoKr", "krxStockPrices", "requestFailed");
        };
        if !response.status().is_success() {
            return failed("dataGoKr", "krxStockPrices", "httpFailure");
        }
        let Ok(value) = response.json::<Value>().await else {
            return failed("dataGoKr", "krxStockPrices", "invalidResponse");
        };
        if value
            .pointer("/response/header/resultCode")
            .and_then(Value::as_str)
            != Some("00")
        {
            return failed("dataGoKr", "krxStockPrices", "providerFailure");
        }
        let Some(body) = value.pointer("/response/body") else {
            return failed("dataGoKr", "krxStockPrices", "invalidResponse");
        };
        let Some(total_count) = json_u32(body.get("totalCount")) else {
            return failed("dataGoKr", "krxStockPrices", "invalidResponse");
        };
        let stable_count = *expected_count.get_or_insert(total_count);
        if total_count != stable_count {
            return failed("dataGoKr", "krxStockPrices", "countChanged");
        }
        let Some(rows) = body
            .pointer("/items/item")
            .and_then(Value::as_array)
            .cloned()
        else {
            return failed("dataGoKr", "krxStockPrices", "invalidResponse");
        };
        items.extend(rows);
        if items.len() >= stable_count as usize {
            break;
        }
        if page >= 100 {
            return failed("dataGoKr", "krxStockPrices", "pageLimit");
        }
        page += 1;
    }
    if items.is_empty() || items.len() != expected_count.unwrap_or_default() as usize {
        return failed("dataGoKr", "krxStockPrices", "countMismatch");
    }
    completed(
        "dataGoKr",
        "krxStockPrices",
        u32::try_from(items.len()).unwrap_or(u32::MAX),
        &items,
        Some(source_as_of.to_owned()),
    )
}

fn json_u32(value: Option<&Value>) -> Option<u32> {
    let value = value?;
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .or_else(|| value.as_str()?.parse().ok())
}

#[derive(Deserialize)]
struct DartCorpCodeDocument {
    #[serde(rename = "list", default)]
    rows: Vec<DartCorpCodeRow>,
}

#[derive(Deserialize)]
struct DartCorpCodeRow {
    corp_code: String,
    stock_code: String,
}

async fn load_open_dart_codes(
    client: &Client,
    key: &Secret,
) -> anyhow::Result<HashMap<String, String>> {
    let response = client
        .get(OPEN_DART_CORP_CODE_URL)
        .query(&[("crtfc_key", key.expose())])
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("OpenDART corporation-code request failed"))?;
    ensure!(
        response.status().is_success(),
        "OpenDART corporation-code request returned HTTP {}",
        response.status().as_u16()
    );
    let bytes = response
        .bytes()
        .await
        .map_err(|_| anyhow::anyhow!("OpenDART corporation-code response could not be read"))?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| anyhow::anyhow!("OpenDART corporation-code response was not a ZIP file"))?;
    let mut xml = String::new();
    archive
        .by_name("CORPCODE.xml")
        .map_err(|_| anyhow::anyhow!("OpenDART corporation-code ZIP had no CORPCODE.xml"))?
        .read_to_string(&mut xml)
        .map_err(|_| anyhow::anyhow!("OpenDART corporation-code XML could not be read"))?;
    parse_open_dart_codes(&xml)
}

#[derive(Deserialize)]
struct DartCompanyResponse {
    status: String,
    corp_code: Option<String>,
    stock_code: Option<String>,
    induty_code: Option<String>,
}

async fn load_open_dart_companies(
    client: &Client,
    key: &Secret,
) -> anyhow::Result<HashMap<String, DartCompanyMetadata>> {
    let codes = load_open_dart_codes(client, key).await?;
    let rows = stream::iter(codes)
        .map(|(stock_code, corp_code)| async move {
            load_open_dart_company(client, key, stock_code, corp_code).await
        })
        .buffer_unordered(OPEN_DART_CONCURRENCY)
        .try_collect::<Vec<_>>()
        .await?;
    Ok(rows.into_iter().collect())
}

async fn load_open_dart_company(
    client: &Client,
    key: &Secret,
    stock_code: String,
    corp_code: String,
) -> anyhow::Result<(String, DartCompanyMetadata)> {
    let response = client
        .get(OPEN_DART_COMPANY_URL)
        .query(&[
            ("crtfc_key", key.expose()),
            ("corp_code", corp_code.as_str()),
        ])
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("OpenDART company-overview request failed"))?;
    ensure!(
        response.status().is_success(),
        "OpenDART company-overview request returned HTTP {}",
        response.status().as_u16()
    );
    let overview: DartCompanyResponse = response
        .json()
        .await
        .map_err(|_| anyhow::anyhow!("OpenDART company-overview response was invalid JSON"))?;
    ensure!(
        overview.status == "000",
        "OpenDART company-overview returned an unsuccessful status"
    );
    ensure!(
        overview.corp_code.as_deref() == Some(corp_code.as_str())
            && overview.stock_code.as_deref().map(str::trim) == Some(stock_code.as_str()),
        "OpenDART company-overview identity did not match its request"
    );
    let industry_code = overview
        .induty_code
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    Ok((
        stock_code,
        DartCompanyMetadata {
            corp_code,
            industry_code,
        },
    ))
}

fn parse_open_dart_codes(xml: &str) -> anyhow::Result<HashMap<String, String>> {
    let document: DartCorpCodeDocument = quick_xml::de::from_str(xml)
        .context("OpenDART corporation-code XML did not match its contract")?;
    let mut codes = HashMap::new();
    for row in document.rows {
        let stock_code = row.stock_code.trim();
        let corp_code = row.corp_code.trim();
        if stock_code.len() == 6
            && stock_code
                .chars()
                .all(|value| value.is_ascii_alphanumeric())
            && corp_code.len() == 8
            && corp_code.chars().all(|value| value.is_ascii_digit())
        {
            codes.insert(stock_code.to_ascii_uppercase(), corp_code.to_owned());
        }
    }
    ensure!(!codes.is_empty(), "OpenDART corporation-code map was empty");
    Ok(codes)
}

async fn observe_krx(
    client: &Client,
    key: Option<&Secret>,
    catalog: &EquityCatalogInput,
) -> ProviderObservation {
    let Some(key) = key else {
        return not_configured("krx", "listedInstrumentValidation");
    };
    let endpoints = [
        "https://data-dbg.krx.co.kr/svc/apis/sto/stk_isu_base_info",
        "https://data-dbg.krx.co.kr/svc/apis/sto/ksq_isu_base_info",
        "https://data-dbg.krx.co.kr/svc/apis/sto/knx_isu_base_info",
    ];
    let mut values = Vec::with_capacity(endpoints.len());
    let mut krx_codes = HashSet::new();
    for endpoint in endpoints {
        let result = client
            .get(endpoint)
            .header("AUTH_KEY", key.expose())
            .send()
            .await;
        let Ok(response) = result else {
            return failed("krx", "listedInstrumentValidation", "requestFailed");
        };
        if !response.status().is_success() {
            return failed("krx", "listedInstrumentValidation", "httpFailure");
        }
        let Ok(value) = response.json::<Value>().await else {
            return failed("krx", "listedInstrumentValidation", "invalidResponse");
        };
        krx_codes.extend(krx_short_codes(&value));
        values.push(value);
    }
    if krx_codes.is_empty() {
        return failed("krx", "listedInstrumentValidation", "invalidResponse");
    }
    let catalog_codes = catalog
        .instruments
        .iter()
        .map(|instrument| instrument.short_code.as_str())
        .collect::<HashSet<_>>();
    if !krx_codes
        .iter()
        .all(|code| catalog_codes.contains(code.as_str()))
    {
        return failed("krx", "listedInstrumentValidation", "catalogMismatch");
    }
    completed(
        "krx",
        "listedInstrumentValidation",
        u32::try_from(krx_codes.len()).unwrap_or(u32::MAX),
        &values,
        None,
    )
}

fn krx_short_codes(value: &Value) -> HashSet<String> {
    value
        .get("OutBlock_1")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("ISU_SRT_CD").and_then(Value::as_str))
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn observe_fmp(client: &Client, key: Option<&Secret>) -> ProviderObservation {
    let Some(key) = key else {
        return not_configured("fmp", "stockDirectoryCalibration");
    };
    let result = client
        .get("https://financialmodelingprep.com/stable/stock-list")
        .header("apikey", key.expose())
        .send()
        .await;
    let Ok(response) = result else {
        return failed("fmp", "stockDirectoryCalibration", "requestFailed");
    };
    if !response.status().is_success() {
        return failed("fmp", "stockDirectoryCalibration", "httpFailure");
    }
    let Ok(value) = response.json::<Value>().await else {
        return failed("fmp", "stockDirectoryCalibration", "invalidResponse");
    };
    completed(
        "fmp",
        "stockDirectoryCalibration",
        json_row_count(&value),
        &value,
        None,
    )
}

async fn observe_ecos(
    client: &Client,
    key: Option<&Secret>,
    dataset: Option<&EcosDataset>,
) -> ProviderObservation {
    let (Some(key), Some(dataset)) = (key, dataset) else {
        return not_configured("ecos", "macroCalibration");
    };
    let Some((start, end)) = ecos_bounds(&dataset.period_code) else {
        return failed("ecos", "macroCalibration", "unsupportedPeriod");
    };
    let mut values = Vec::with_capacity(dataset.item_codes.len());
    let mut row_count = 0_u32;
    for item_code in &dataset.item_codes {
        let url = format!(
            "https://ecos.bok.or.kr/api/StatisticSearch/{}/json/kr/1/100/{}/{}/{}/{}/{}",
            key.expose(),
            dataset.statistic_code,
            dataset.period_code,
            start,
            end,
            item_code
        );
        let Ok(response) = client.get(url).send().await else {
            return failed("ecos", "macroCalibration", "requestFailed");
        };
        if !response.status().is_success() {
            return failed("ecos", "macroCalibration", "httpFailure");
        }
        let Ok(value) = response.json::<Value>().await else {
            return failed("ecos", "macroCalibration", "invalidResponse");
        };
        row_count = row_count.saturating_add(json_row_count(&value));
        values.push(value);
    }
    completed("ecos", "macroCalibration", row_count, &values, None)
}

async fn observe_kosis(
    client: &Client,
    key: Option<&Secret>,
    dataset: Option<&KosisDataset>,
) -> ProviderObservation {
    let (Some(key), Some(dataset)) = (key, dataset) else {
        return not_configured("kosis", "populationIncomeCalibration");
    };
    let result = client
        .get("https://kosis.kr/openapi/statisticsData.do")
        .query(&[
            ("method", "getMeta"),
            ("type", "TBL"),
            ("apiKey", key.expose()),
            ("orgId", dataset.organization_id.as_str()),
            ("tblId", dataset.table_id.as_str()),
            ("format", "json"),
        ])
        .send()
        .await;
    let Ok(response) = result else {
        return failed("kosis", "populationIncomeCalibration", "requestFailed");
    };
    if !response.status().is_success() {
        return failed("kosis", "populationIncomeCalibration", "httpFailure");
    }
    let Ok(value) = response.json::<Value>().await else {
        return failed("kosis", "populationIncomeCalibration", "invalidResponse");
    };
    completed(
        "kosis",
        "populationIncomeCalibration",
        json_row_count(&value),
        &value,
        None,
    )
}

fn ecos_bounds(period_code: &str) -> Option<(&'static str, &'static str)> {
    match period_code {
        "A" | "Y" => Some(("2000", "2099")),
        "Q" => Some(("2000Q1", "2099Q4")),
        "M" => Some(("200001", "209912")),
        "D" => Some(("20000101", "20991231")),
        _ => None,
    }
}

fn json_row_count(value: &Value) -> u32 {
    if let Some(rows) = value.as_array() {
        return u32::try_from(rows.len()).unwrap_or(u32::MAX);
    }
    for key in ["OutBlock_1", "row"] {
        if let Some(rows) = value.get(key).and_then(Value::as_array) {
            return u32::try_from(rows.len()).unwrap_or(u32::MAX);
        }
    }
    value
        .as_object()
        .and_then(|object| object.values().find_map(|nested| nested.get("row")))
        .and_then(Value::as_array)
        .and_then(|rows| u32::try_from(rows.len()).ok())
        .unwrap_or(0)
}

fn completed<T: serde::Serialize>(
    provider: &'static str,
    dataset: &'static str,
    row_count: u32,
    value: &T,
    source_as_of: Option<String>,
) -> ProviderObservation {
    use sha2::{Digest, Sha256};

    let hash = serde_json::to_vec(value)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)));
    ProviderObservation {
        provider,
        dataset,
        status: ProviderObservationStatus::Completed,
        row_count,
        content_sha256: hash,
        source_as_of,
        failure_code: None,
    }
}

fn not_configured(provider: &'static str, dataset: &'static str) -> ProviderObservation {
    ProviderObservation {
        provider,
        dataset,
        status: ProviderObservationStatus::NotConfigured,
        row_count: 0,
        content_sha256: None,
        source_as_of: None,
        failure_code: None,
    }
}

fn failed(
    provider: &'static str,
    dataset: &'static str,
    failure_code: &'static str,
) -> ProviderObservation {
    ProviderObservation {
        provider,
        dataset,
        status: ProviderObservationStatus::Failed,
        row_count: 0,
        content_sha256: None,
        source_as_of: None,
        failure_code: Some(failure_code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_open_dart_corporation_codes {
        use super::*;

        #[test]
        fn given_listed_and_unlisted_rows_when_parsed_then_only_listed_code_is_mapped() {
            let xml = "<result><list><corp_code>00126380</corp_code><stock_code>005930</stock_code></list><list><corp_code>00434003</corp_code><stock_code> </stock_code></list></result>";

            let codes = parse_open_dart_codes(xml).expect("valid fixture");

            assert_eq!(codes.get("005930").map(String::as_str), Some("00126380"));
            assert_eq!(codes.len(), 1);
        }
    }

    mod context_data_go_listed_item {
        use super::*;

        #[test]
        fn given_kosdaq_item_when_normalized_then_contract_fields_are_trimmed() {
            let item = DataGoListedItem {
                bas_dt: "20260729".to_owned(),
                srtn_cd: " 035720 ".to_owned(),
                isin_cd: " KR7035720002 ".to_owned(),
                mrkt_ctg: "KOSDAQ".to_owned(),
                itms_nm: " 카카오 ".to_owned(),
                crno: Some("1101111122334".to_owned()),
                corp_nm: " 카카오 ".to_owned(),
            };

            let normalized = normalize_data_go_item(item).expect("valid fixture");

            assert_eq!(normalized.short_code, "035720");
            assert_eq!(normalized.market, EquityMarket::Kosdaq);
            assert_eq!(normalized.display_name, "카카오");
        }
    }

    mod context_data_go_count_changes_json_representation {
        use super::*;

        #[test]
        fn given_numeric_and_string_counts_when_parsed_then_both_keep_the_same_integer() {
            let numeric = Value::from(3_000_u32);
            let string = Value::from("3000");

            let counts = (json_u32(Some(&numeric)), json_u32(Some(&string)));

            assert_eq!(counts, (Some(3_000), Some(3_000)));
        }
    }
}
