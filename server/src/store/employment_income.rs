use anyhow::{Context, Result, ensure};
use sqlx::{MySql, Transaction};
use time::Date;

use super::types::VerifiedIncomeSourceState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VerifiedAnnualIncomeState {
    pub annual_income_krw: i64,
    pub source: VerifiedIncomeSourceState,
}

pub(super) async fn read_verified_annual_income_in_tx(
    tx: &mut Transaction<'_, MySql>,
    save_id: u64,
    run_revision: u32,
    game_day: u32,
) -> Result<Option<VerifiedAnnualIncomeState>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT annual_salary_krw
         FROM employment_contract
         WHERE save_id = ? AND run_revision = ?
           AND status = 'active'
           AND start_game_day <= ?
           AND (end_game_day IS NULL OR end_game_day > ?)
         ORDER BY id
         LIMIT 2
         FOR SHARE",
    )
    .bind(save_id)
    .bind(run_revision)
    .bind(game_day)
    .bind(game_day)
    .fetch_all(&mut **tx)
    .await?;
    ensure!(
        rows.len() <= 1,
        "run has multiple active employment contracts"
    );
    rows.into_iter()
        .next()
        .map(|(annual_income_krw,)| {
            ensure!(
                annual_income_krw > 0,
                "active employment salary is not positive"
            );
            Ok(VerifiedAnnualIncomeState {
                annual_income_krw,
                source: VerifiedIncomeSourceState::ActiveEmploymentContract,
            })
        })
        .transpose()
}

#[derive(Debug, Clone, Copy)]
pub(super) enum EmploymentIncomeEventSource {
    EmploymentPayroll {
        payroll_record_id: u64,
        period_no: u64,
    },
    MilitaryPay {
        military_service_id: u64,
        period_no: u64,
    },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EmploymentIncomeAmounts {
    pub(super) gross_employment_income_krw: i64,
    pub(super) employee_national_pension_krw: i64,
    pub(super) employee_health_insurance_krw: i64,
    pub(super) employee_long_term_care_krw: i64,
    pub(super) employee_employment_insurance_krw: i64,
    pub(super) employee_insurance_total_krw: i64,
    pub(super) withheld_income_tax_krw: i64,
    pub(super) withheld_local_income_tax_krw: i64,
    pub(super) net_pay_krw: i64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EmploymentIncomeEventWrite {
    pub(super) save_id: u64,
    pub(super) run_revision: u32,
    pub(super) employment_policy_set_id: u64,
    pub(super) source: EmploymentIncomeEventSource,
    pub(super) scheduled_settlement_id: u64,
    pub(super) ledger_transaction_id: Option<u64>,
    pub(super) paid_game_day: u32,
    pub(super) paid_date: Date,
    pub(super) amounts: EmploymentIncomeAmounts,
}

pub(super) async fn record_employment_income_event_in_tx(
    tx: &mut Transaction<'_, MySql>,
    write: EmploymentIncomeEventWrite,
) -> Result<u64> {
    let tax_year = u16::try_from(write.paid_date.year())
        .context("employment income tax year is outside the supported range")?;
    let existing_year: Option<(u64, String)> = sqlx::query_as(
        "SELECT employment_policy_set_id, status
         FROM employment_income_year
         WHERE save_id = ? AND run_revision = ? AND tax_year = ? FOR UPDATE",
    )
    .bind(write.save_id)
    .bind(write.run_revision)
    .bind(tax_year)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((policy_set_id, status)) = &existing_year {
        ensure!(
            *policy_set_id == write.employment_policy_set_id && status == "open",
            "employment income event targets a closed or mismatched tax year"
        );
    }

    let (source_kind, source_id, occurrence, payroll_record_id, military_service_id) =
        match write.source {
            EmploymentIncomeEventSource::EmploymentPayroll {
                payroll_record_id,
                period_no,
            } => (
                "employmentPayroll",
                payroll_record_id,
                period_no,
                Some(payroll_record_id),
                None,
            ),
            EmploymentIncomeEventSource::MilitaryPay {
                military_service_id,
                period_no,
            } => (
                "militaryPay",
                military_service_id,
                period_no,
                None,
                Some(military_service_id),
            ),
        };
    let amounts = write.amounts;
    let insert = sqlx::query(
        "INSERT INTO employment_income_event
             (save_id, run_revision, employment_policy_set_id, source_kind, source_id,
              occurrence, payroll_record_id, military_service_id,
              scheduled_settlement_id, ledger_transaction_id, paid_game_day, paid_date,
              tax_year, gross_employment_income_krw, employee_national_pension_krw,
              employee_health_insurance_krw, employee_long_term_care_krw,
              employee_employment_insurance_krw, employee_insurance_total_krw,
              withheld_income_tax_krw, withheld_local_income_tax_krw, net_pay_krw)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(write.save_id)
    .bind(write.run_revision)
    .bind(write.employment_policy_set_id)
    .bind(source_kind)
    .bind(source_id)
    .bind(occurrence)
    .bind(payroll_record_id)
    .bind(military_service_id)
    .bind(write.scheduled_settlement_id)
    .bind(write.ledger_transaction_id)
    .bind(write.paid_game_day)
    .bind(write.paid_date)
    .bind(tax_year)
    .bind(amounts.gross_employment_income_krw)
    .bind(amounts.employee_national_pension_krw)
    .bind(amounts.employee_health_insurance_krw)
    .bind(amounts.employee_long_term_care_krw)
    .bind(amounts.employee_employment_insurance_krw)
    .bind(amounts.employee_insurance_total_krw)
    .bind(amounts.withheld_income_tax_krw)
    .bind(amounts.withheld_local_income_tax_krw)
    .bind(amounts.net_pay_krw)
    .execute(&mut **tx)
    .await?;
    let event_id = insert.last_insert_id();

    if existing_year.is_some() {
        let update = sqlx::query(
            "UPDATE employment_income_year AS income_year
             SET income_year.gross_employment_income_krw = (
                     SELECT COALESCE(SUM(event.gross_employment_income_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.employee_national_pension_krw = (
                     SELECT COALESCE(SUM(event.employee_national_pension_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.employee_health_insurance_krw = (
                     SELECT COALESCE(SUM(event.employee_health_insurance_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.employee_long_term_care_krw = (
                     SELECT COALESCE(SUM(event.employee_long_term_care_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.employee_employment_insurance_krw = (
                     SELECT COALESCE(SUM(event.employee_employment_insurance_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.employee_insurance_total_krw = (
                     SELECT COALESCE(SUM(event.employee_insurance_total_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.withheld_income_tax_krw = (
                     SELECT COALESCE(SUM(event.withheld_income_tax_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.withheld_local_income_tax_krw = (
                     SELECT COALESCE(SUM(event.withheld_local_income_tax_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.net_salary_pay_krw = (
                     SELECT COALESCE(SUM(event.net_pay_krw), 0)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.income_event_count = (
                     SELECT COUNT(*) FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year),
                 income_year.last_income_event_id = ?,
                 income_year.payroll_count = (
                     SELECT COUNT(*) FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year
                       AND event.source_kind = 'employmentPayroll'),
                 income_year.last_payroll_record_id = (
                     SELECT MAX(event.payroll_record_id)
                     FROM employment_income_event AS event
                     WHERE event.save_id = income_year.save_id
                       AND event.run_revision = income_year.run_revision
                       AND event.tax_year = income_year.tax_year)
             WHERE income_year.save_id = ? AND income_year.run_revision = ?
               AND income_year.tax_year = ?
               AND income_year.employment_policy_set_id = ? AND income_year.status = 'open'",
        )
        .bind(event_id)
        .bind(write.save_id)
        .bind(write.run_revision)
        .bind(tax_year)
        .bind(write.employment_policy_set_id)
        .execute(&mut **tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "employment income event lost its annual aggregate lock"
        );
    } else {
        let insert_year = sqlx::query(
            "INSERT INTO employment_income_year
                 (save_id, run_revision, tax_year, employment_policy_set_id, status,
                  gross_employment_income_krw, employee_national_pension_krw,
                  employee_health_insurance_krw, employee_long_term_care_krw,
                  employee_employment_insurance_krw, employee_insurance_total_krw,
                  withheld_income_tax_krw, withheld_local_income_tax_krw,
                  net_salary_pay_krw, income_event_count, last_income_event_id,
                  payroll_count, last_payroll_record_id)
             SELECT ?, ?, ?, ?, 'open',
                    SUM(event.gross_employment_income_krw),
                    SUM(event.employee_national_pension_krw),
                    SUM(event.employee_health_insurance_krw),
                    SUM(event.employee_long_term_care_krw),
                    SUM(event.employee_employment_insurance_krw),
                    SUM(event.employee_insurance_total_krw),
                    SUM(event.withheld_income_tax_krw),
                    SUM(event.withheld_local_income_tax_krw), SUM(event.net_pay_krw),
                    COUNT(*), MAX(event.id),
                    SUM(event.source_kind = 'employmentPayroll'),
                    MAX(event.payroll_record_id)
             FROM employment_income_event AS event
             WHERE event.save_id = ? AND event.run_revision = ? AND event.tax_year = ?",
        )
        .bind(write.save_id)
        .bind(write.run_revision)
        .bind(tax_year)
        .bind(write.employment_policy_set_id)
        .bind(write.save_id)
        .bind(write.run_revision)
        .bind(tax_year)
        .execute(&mut **tx)
        .await?;
        ensure!(
            insert_year.rows_affected() == 1,
            "employment income year was not created from its first event"
        );
    }
    Ok(event_id)
}
