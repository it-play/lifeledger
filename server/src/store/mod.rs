//! MySQL 접근 계층 (§4). 상위 계층은 `SaveStore` 만 보고 구현은 보지 않는다.

mod mysql;
mod types;

pub use mysql::create_mysql_save_store;
pub use types::{SaveState, SaveStore};
