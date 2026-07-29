mod accrual;
mod types;

use std::sync::Arc;

pub use types::{OfflineAccrualInput, OfflineAccrualPlan, OfflineRuleError, OfflineRules};

pub fn create_offline_rules() -> Arc<dyn OfflineRules> {
    Arc::new(accrual::DefaultOfflineRules)
}
