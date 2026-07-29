use anyhow::{Context, ensure};

pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

pub struct EcosDataset {
    pub statistic_code: String,
    pub period_code: String,
    pub item_codes: Vec<String>,
}

pub struct KosisDataset {
    pub organization_id: String,
    pub table_id: String,
}

pub struct MarketDataConfig {
    pub data_go_kr_service_key: Option<Secret>,
    pub krx_auth_key: Option<Secret>,
    pub open_dart_api_key: Option<Secret>,
    pub fmp_api_key: Option<Secret>,
    pub ecos_api_key: Option<Secret>,
    pub ecos_dataset: Option<EcosDataset>,
    pub kosis_api_key: Option<Secret>,
    pub kosis_dataset: Option<KosisDataset>,
}

impl MarketDataConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let ecos_api_key = optional_secret("ECOS_API_KEY");
        let ecos_statistic_code = optional_value("ECOS_STATISTIC_CODE");
        let ecos_period_code = optional_value("ECOS_PERIOD_CODE");
        let ecos_item_codes = optional_value("ECOS_ITEM_CODES");
        ensure_same_presence(
            "ECOS_STATISTIC_CODE, ECOS_PERIOD_CODE and ECOS_ITEM_CODES",
            &[
                ecos_statistic_code.is_some(),
                ecos_period_code.is_some(),
                ecos_item_codes.is_some(),
            ],
        )?;
        let ecos_dataset = match (ecos_statistic_code, ecos_period_code, ecos_item_codes) {
            (Some(statistic_code), Some(period_code), Some(item_codes)) => {
                let item_codes = item_codes
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                ensure!(
                    !item_codes.is_empty(),
                    "ECOS_ITEM_CODES contains no item code"
                );
                Some(EcosDataset {
                    statistic_code,
                    period_code,
                    item_codes,
                })
            }
            _ => None,
        };

        let kosis_api_key = optional_secret("KOSIS_API_KEY");
        let kosis_organization_id = optional_value("KOSIS_ORG_ID");
        let kosis_table_id = optional_value("KOSIS_TABLE_ID");
        ensure_same_presence(
            "KOSIS_ORG_ID and KOSIS_TABLE_ID",
            &[kosis_organization_id.is_some(), kosis_table_id.is_some()],
        )?;
        let kosis_dataset = match (kosis_organization_id, kosis_table_id) {
            (Some(organization_id), Some(table_id)) => Some(KosisDataset {
                organization_id,
                table_id,
            }),
            _ => None,
        };

        Ok(Self {
            data_go_kr_service_key: optional_secret("DATA_GO_KR_SERVICE_KEY"),
            krx_auth_key: optional_secret("KRX_AUTH_KEY"),
            open_dart_api_key: optional_secret("OPENDART_API_KEY"),
            fmp_api_key: optional_secret("FMP_API_KEY"),
            ecos_api_key,
            ecos_dataset,
            kosis_api_key,
            kosis_dataset,
        })
    }

    pub fn require_data_go_key(&self) -> anyhow::Result<&Secret> {
        self.data_go_kr_service_key
            .as_ref()
            .context("DATA_GO_KR_SERVICE_KEY is not set - see plan-docs/market-data-keys.md")
    }
}

fn optional_secret(name: &str) -> Option<Secret> {
    optional_value(name).map(Secret)
}

fn optional_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn ensure_same_presence(label: &str, values: &[bool]) -> anyhow::Result<()> {
    let configured = values.iter().filter(|value| **value).count();
    ensure!(
        configured == 0 || configured == values.len(),
        "{label} must be configured together"
    );
    Ok(())
}
