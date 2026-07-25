//! MySQL access layer (§4). Callers see only the traits.

mod mysql;
mod types;
mod user;

pub use mysql::create_mysql_save_store;
pub use types::{AccountUser, SaveState, SaveStore, UserStore};
pub use user::create_mysql_user_store;
