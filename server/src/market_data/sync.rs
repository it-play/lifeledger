use anyhow::{Context, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{MySql, MySqlPool, Transaction};

use super::providers::{MarketDataProviders, ProviderObservation, ProviderObservationStatus};
use super::types::{EquityCatalogInput, EquityInstrumentInput};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketDataSyncReport {
    catalog_version: String,
    source_as_of: String,
    instrument_count: usize,
    catalog_changed: bool,
    sources: Vec<MarketDataSourceReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketDataSourceReport {
    provider: &'static str,
    dataset: &'static str,
    status: &'static str,
    row_count: u32,
    failure_code: Option<&'static str>,
}

pub(crate) async fn synchronize_market_data(pool: MySqlPool) -> anyhow::Result<()> {
    let providers = MarketDataProviders::from_env()?;
    let started_at = database_time(&pool).await?;
    let mut catalog = providers.load_catalog().await?;
    let mut observations = vec![completed_catalog_observation(&catalog)?];
    observations.push(providers.observe_stock_prices(&catalog.source_as_of).await);

    match providers.load_dart_companies().await {
        Ok(Some(companies)) => {
            for instrument in &mut catalog.instruments {
                if let Some(company) = companies.get(&instrument.short_code) {
                    instrument.dart_corp_code = Some(company.corp_code.clone());
                    instrument.industry_code = company.industry_code.clone();
                }
            }
            observations.push(ProviderObservation {
                provider: "openDart",
                dataset: "corporationCodes",
                status: ProviderObservationStatus::Completed,
                row_count: u32::try_from(companies.len()).unwrap_or(u32::MAX),
                content_sha256: Some(hash_serializable(&companies)?),
                source_as_of: None,
                failure_code: None,
            });
        }
        Ok(None) => observations.push(ProviderObservation {
            provider: "openDart",
            dataset: "corporationCodes",
            status: ProviderObservationStatus::NotConfigured,
            row_count: 0,
            content_sha256: None,
            source_as_of: None,
            failure_code: None,
        }),
        Err(_) => observations.push(ProviderObservation {
            provider: "openDart",
            dataset: "corporationCodes",
            status: ProviderObservationStatus::Failed,
            row_count: 0,
            content_sha256: None,
            source_as_of: None,
            failure_code: Some("requestFailed"),
        }),
    }
    observations.extend(providers.observe_optional_sources(&catalog).await);

    validate_catalog(&catalog)?;
    let content_sha256 = hash_serializable(&catalog.instruments)?;
    let version = format!(
        "krx-listed-{}-{}",
        catalog.source_as_of,
        &content_sha256[..12]
    );
    let completed_at = database_time(&pool).await?;
    let mut transaction = pool.begin().await?;
    let (catalog_id, changed) =
        publish_catalog(&mut transaction, &version, &content_sha256, &catalog).await?;
    assign_catalog(&mut transaction, catalog_id).await?;
    for observation in &observations {
        insert_observation(&mut transaction, observation, &started_at, &completed_at).await?;
    }
    transaction.commit().await?;

    let report = MarketDataSyncReport {
        catalog_version: version,
        source_as_of: catalog.source_as_of,
        instrument_count: catalog.instruments.len(),
        catalog_changed: changed,
        sources: observations
            .into_iter()
            .map(|observation| MarketDataSourceReport {
                provider: observation.provider,
                dataset: observation.dataset,
                status: observation.status.as_str(),
                row_count: observation.row_count,
                failure_code: observation.failure_code,
            })
            .collect(),
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}

fn completed_catalog_observation(
    catalog: &EquityCatalogInput,
) -> anyhow::Result<ProviderObservation> {
    Ok(ProviderObservation {
        provider: "dataGoKr",
        dataset: "krxListedInstruments",
        status: ProviderObservationStatus::Completed,
        row_count: u32::try_from(catalog.instruments.len()).unwrap_or(u32::MAX),
        content_sha256: Some(hash_serializable(&catalog.instruments)?),
        source_as_of: Some(catalog.source_as_of.clone()),
        failure_code: None,
    })
}

fn validate_catalog(catalog: &EquityCatalogInput) -> anyhow::Result<()> {
    ensure!(
        catalog.source_as_of.len() == 8
            && catalog
                .source_as_of
                .chars()
                .all(|value| value.is_ascii_digit()),
        "catalog source date must use YYYYMMDD"
    );
    ensure!(
        !catalog.instruments.is_empty(),
        "catalog must contain at least one instrument"
    );
    for window in catalog.instruments.windows(2) {
        ensure!(
            window[0].short_code < window[1].short_code,
            "catalog short codes must be unique and sorted"
        );
    }
    Ok(())
}

async fn publish_catalog(
    transaction: &mut Transaction<'_, MySql>,
    version: &str,
    content_sha256: &str,
    catalog: &EquityCatalogInput,
) -> anyhow::Result<(u64, bool)> {
    let existing: Option<u64> = sqlx::query_scalar(
        "SELECT id
         FROM equity_catalog_version
         WHERE BINARY content_sha256 = BINARY ? AND published_at IS NOT NULL",
    )
    .bind(content_sha256)
    .fetch_optional(&mut **transaction)
    .await?;
    if let Some(id) = existing {
        return Ok((id, false));
    }

    let result = sqlx::query(
        "INSERT INTO equity_catalog_version
            (version, source, source_as_of, content_sha256, instrument_count)
         VALUES (?, 'dataGoKr', STR_TO_DATE(?, '%Y%m%d'), ?, ?)",
    )
    .bind(version)
    .bind(&catalog.source_as_of)
    .bind(content_sha256)
    .bind(u32::try_from(catalog.instruments.len()).context("catalog contains too many rows")?)
    .execute(&mut **transaction)
    .await?;
    let catalog_id = result.last_insert_id();
    for instrument in &catalog.instruments {
        insert_instrument(transaction, catalog_id, instrument).await?;
    }
    let published = sqlx::query(
        "UPDATE equity_catalog_version
         SET published_at = UTC_TIMESTAMP(3)
         WHERE id = ? AND published_at IS NULL",
    )
    .bind(catalog_id)
    .execute(&mut **transaction)
    .await?;
    ensure!(
        published.rows_affected() == 1,
        "equity catalog version was not published exactly once"
    );
    Ok((catalog_id, true))
}

async fn insert_instrument(
    transaction: &mut Transaction<'_, MySql>,
    catalog_id: u64,
    instrument: &EquityInstrumentInput,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO equity_instrument_version
            (equity_catalog_version_id, isin, short_code, market, display_name,
             corporation_name, corporation_registration_number, dart_corp_code, industry_code)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(catalog_id)
    .bind(&instrument.isin)
    .bind(&instrument.short_code)
    .bind(instrument.market.as_str())
    .bind(&instrument.display_name)
    .bind(&instrument.corporation_name)
    .bind(&instrument.corporation_registration_number)
    .bind(&instrument.dart_corp_code)
    .bind(&instrument.industry_code)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn assign_catalog(
    transaction: &mut Transaction<'_, MySql>,
    catalog_id: u64,
) -> anyhow::Result<()> {
    let current: Option<u64> = sqlx::query_scalar(
        "SELECT equity_catalog_version_id
         FROM equity_catalog_assignment
         WHERE BINARY assignment_key = BINARY 'active'
         FOR UPDATE",
    )
    .fetch_optional(&mut **transaction)
    .await?;
    match current {
        Some(current_id) if current_id == catalog_id => {}
        Some(_) => {
            sqlx::query(
                "UPDATE equity_catalog_assignment
                 SET equity_catalog_version_id = ?
                 WHERE BINARY assignment_key = BINARY 'active'",
            )
            .bind(catalog_id)
            .execute(&mut **transaction)
            .await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO equity_catalog_assignment
                    (assignment_key, equity_catalog_version_id)
                 VALUES ('active', ?)",
            )
            .bind(catalog_id)
            .execute(&mut **transaction)
            .await?;
        }
    }
    Ok(())
}

async fn insert_observation(
    transaction: &mut Transaction<'_, MySql>,
    observation: &ProviderObservation,
    started_at: &str,
    completed_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO market_data_sync_run
            (provider, dataset, status, row_count, content_sha256, source_as_of,
             failure_code, started_at, completed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(observation.provider)
    .bind(observation.dataset)
    .bind(observation.status.as_str())
    .bind(observation.row_count)
    .bind(&observation.content_sha256)
    .bind(&observation.source_as_of)
    .bind(observation.failure_code)
    .bind(started_at)
    .bind(completed_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn database_time(pool: &MySqlPool) -> anyhow::Result<String> {
    sqlx::query_scalar("SELECT DATE_FORMAT(UTC_TIMESTAMP(3), '%Y-%m-%d %H:%i:%s.%f')")
        .fetch_one(pool)
        .await
        .context("failed to read database time for market-data sync")
}

fn hash_serializable<T: Serialize>(value: &T) -> anyhow::Result<String> {
    let canonical = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::types::EquityMarket;

    mod context_catalog_version_validation {
        use super::*;

        #[test]
        fn given_duplicate_short_codes_when_validated_then_catalog_is_rejected() {
            let catalog = EquityCatalogInput {
                source_as_of: "20260729".to_owned(),
                instruments: vec![instrument("005930"), instrument("005930")],
            };

            let result = validate_catalog(&catalog);

            assert!(result.is_err());
        }

        fn instrument(short_code: &str) -> EquityInstrumentInput {
            EquityInstrumentInput {
                isin: "KR7005930003".to_owned(),
                short_code: short_code.to_owned(),
                market: EquityMarket::Kospi,
                display_name: "삼성전자".to_owned(),
                corporation_name: "삼성전자".to_owned(),
                corporation_registration_number: None,
                dart_corp_code: None,
                industry_code: None,
            }
        }
    }
}
