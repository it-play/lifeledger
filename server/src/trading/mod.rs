//! Account-scoped LLX validation, checked settlement arithmetic, and valuation (M2 §3.2, §9).

mod rules;
mod types;

pub(crate) use rules::apply_trade_with_charges;
pub use rules::{checked_net_worth_krw, value_portfolio};
pub(crate) use types::TradeCharges;
pub use types::{
    AccountId, LLX_SYMBOL, MAX_TRADE_QUANTITY, OrderId, OrderSide, Portfolio, PortfolioPosition,
    PositionState, TradeExecution, TradeFailure, TradeFailureCode, TradeOrder, TradeOrderRequest,
    TradingMathError,
};
