#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfflineAccrualInput {
    pub db_now_unix_micros: i64,
    pub accrued_through_unix_micros: i64,
    pub accrual_limit_unix_micros: i64,
    pub cadence_seconds: u32,
    pub absence_window_cap_days: u32,
    pub window_accrued_days: u32,
    pub remaining_target_days: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfflineAccrualPlan {
    pub days_to_accrue: u32,
    pub accrued_through_advance_micros: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineRuleError {
    InvalidCadence,
    InvalidWindowState,
    ArithmeticOverflow,
}

impl std::fmt::Display for OfflineRuleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidCadence => "offline cadence must be positive",
            Self::InvalidWindowState => "offline window state is invalid",
            Self::ArithmeticOverflow => "offline accrual arithmetic overflowed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for OfflineRuleError {}

pub trait OfflineRules: Send + Sync + 'static {
    fn plan_accrual(
        &self,
        input: OfflineAccrualInput,
    ) -> Result<OfflineAccrualPlan, OfflineRuleError>;
}
