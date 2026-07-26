use std::collections::BTreeSet;

use time::{Date, Month, Weekday};

use super::types::MarketError;

pub const KRX_MARKET_CALENDAR_ID: &str = "krx-equity-2026-rules-v1";

const SNAPSHOT: &str = include_str!("data/krx_equity_2026_rules_v1.csv");
const COVERAGE_START_YEAR: i32 = 2026;
const COVERAGE_END_YEAR: i32 = 2060;
const CSV_HEADER: &str = "date,provenance,label";

pub(crate) trait MarketCalendar: Send + Sync {
    fn is_market_open(&self, date: Date) -> Result<bool, MarketError>;
}

pub(crate) struct WeekendOnlyCalendar;

impl MarketCalendar for WeekendOnlyCalendar {
    fn is_market_open(&self, date: Date) -> Result<bool, MarketError> {
        Ok(!is_weekend(date))
    }
}

pub(crate) struct KrxClosureCalendar {
    closures: BTreeSet<Date>,
}

impl KrxClosureCalendar {
    pub(crate) fn from_embedded_snapshot() -> Result<Self, MarketError> {
        parse_snapshot(SNAPSHOT)
    }
}

impl MarketCalendar for KrxClosureCalendar {
    fn is_market_open(&self, date: Date) -> Result<bool, MarketError> {
        if !is_covered(date) {
            return Err(MarketError::DateOutOfRange);
        }

        Ok(!is_weekend(date) && !self.closures.contains(&date))
    }
}

fn parse_snapshot(source: &str) -> Result<KrxClosureCalendar, MarketError> {
    let mut calendar_id = None;
    let mut coverage_start = None;
    let mut coverage_end = None;
    let mut verified_through = None;
    let mut projected_range = None;
    let mut rule_set_version = None;
    let mut lunar_anchor_version = None;
    let mut generated_by = None;
    let mut saw_header = false;
    let mut closures = BTreeSet::new();
    let mut previous_date = None;

    for raw_line in source.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(metadata) = line.strip_prefix("# ") {
            if let Some((key, value)) = metadata.split_once('=') {
                match key {
                    "calendarId" => calendar_id = Some(value),
                    "coverageStart" => coverage_start = Some(value),
                    "coverageEnd" => coverage_end = Some(value),
                    "verifiedThrough" => verified_through = Some(value),
                    "projectedRange" => projected_range = Some(value),
                    "ruleSetVersion" => rule_set_version = Some(value),
                    "lunarAnchorVersion" => lunar_anchor_version = Some(value),
                    "generatedBy" => generated_by = Some(value),
                    _ => {}
                }
            }
            continue;
        }
        if !saw_header {
            if line != CSV_HEADER {
                return Err(MarketError::InvalidCalendar(
                    "closure snapshot header is invalid",
                ));
            }
            saw_header = true;
            continue;
        }

        let mut columns = line.split(',');
        let raw_date = columns.next().ok_or(MarketError::InvalidCalendar(
            "closure snapshot row has no date",
        ))?;
        let provenance = columns.next().ok_or(MarketError::InvalidCalendar(
            "closure snapshot row has no provenance",
        ))?;
        let label = columns.next().ok_or(MarketError::InvalidCalendar(
            "closure snapshot row has no label",
        ))?;
        if columns.next().is_some() || label.trim().is_empty() {
            return Err(MarketError::InvalidCalendar(
                "closure snapshot row shape is invalid",
            ));
        }
        let date = parse_iso_date(raw_date)?;
        validate_provenance(date, provenance)?;
        if !is_covered(date) {
            return Err(MarketError::InvalidCalendar(
                "closure snapshot date is outside its coverage",
            ));
        }
        if previous_date.is_some_and(|previous| date < previous) {
            return Err(MarketError::InvalidCalendar(
                "closure snapshot rows are not sorted by date",
            ));
        }
        previous_date = Some(date);
        closures.insert(date);
    }

    if !saw_header || closures.is_empty() {
        return Err(MarketError::InvalidCalendar(
            "closure snapshot contains no rows",
        ));
    }
    if calendar_id != Some(KRX_MARKET_CALENDAR_ID)
        || coverage_start != Some("2026-01-01")
        || coverage_end != Some("2060-12-31")
        || verified_through != Some("2026-12-31")
        || projected_range != Some("2027-01-01/2060-12-31")
        || rule_set_version != Some("kr-public-holidays-and-krx-closures-2026-v1")
        || lunar_anchor_version != Some("korean-lunisolar-anchors-2026-2060-v1")
        || generated_by != Some("generate_krx_calendar.py")
    {
        return Err(MarketError::InvalidCalendar(
            "closure snapshot metadata does not match the registered calendar",
        ));
    }

    Ok(KrxClosureCalendar { closures })
}

fn validate_provenance(date: Date, provenance: &str) -> Result<(), MarketError> {
    if (date.year() == COVERAGE_START_YEAR && provenance != "krxPublished")
        || (date.year() > COVERAGE_START_YEAR && provenance == "krxPublished")
    {
        return Err(MarketError::InvalidCalendar(
            "closure snapshot provenance does not match its verification range",
        ));
    }

    match provenance {
        "krxPublished" | "statutoryProjected" | "krxRuleProjected" | "manualKrxOverride" => Ok(()),
        _ => Err(MarketError::InvalidCalendar(
            "closure snapshot provenance is invalid",
        )),
    }
}

fn parse_iso_date(raw: &str) -> Result<Date, MarketError> {
    let mut parts = raw.split('-');
    let year = parse_date_component(parts.next())?;
    let month_number = parse_date_component(parts.next())?;
    let day = parse_date_component(parts.next())?;
    if parts.next().is_some() {
        return Err(MarketError::InvalidCalendar(
            "closure snapshot date is invalid",
        ));
    }
    let month = u8::try_from(month_number)
        .ok()
        .and_then(|number| Month::try_from(number).ok())
        .ok_or(MarketError::InvalidCalendar(
            "closure snapshot month is invalid",
        ))?;
    let day = u8::try_from(day)
        .map_err(|_| MarketError::InvalidCalendar("closure snapshot day is invalid"))?;
    Date::from_calendar_date(year, month, day)
        .map_err(|_| MarketError::InvalidCalendar("closure snapshot date is invalid"))
}

fn parse_date_component(raw: Option<&str>) -> Result<i32, MarketError> {
    raw.ok_or(MarketError::InvalidCalendar(
        "closure snapshot date is incomplete",
    ))?
    .parse()
    .map_err(|_| MarketError::InvalidCalendar("closure snapshot date is invalid"))
}

pub(crate) const fn is_weekend(date: Date) -> bool {
    matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday)
}

const fn is_covered(date: Date) -> bool {
    date.year() >= COVERAGE_START_YEAR && date.year() <= COVERAGE_END_YEAR
}
