use super::types::{
    AccountId, LLX_SYMBOL, MAX_TRADE_QUANTITY, OrderSide, Portfolio, PortfolioPosition,
    PositionState, TradeCharges, TradeFailure, TradeMutation, TradingMathError,
};

#[cfg(test)]
pub(crate) fn apply_trade(
    account_id: AccountId,
    account_cash_krw: i64,
    position: Option<&PositionState>,
    side: OrderSide,
    quantity: u32,
    price_krw: i64,
) -> Result<TradeMutation, TradeFailure> {
    apply_trade_with_charges(
        account_id,
        account_cash_krw,
        position,
        side,
        quantity,
        price_krw,
        TradeCharges::default(),
    )
}

pub(crate) fn apply_trade_with_charges(
    account_id: AccountId,
    account_cash_krw: i64,
    position: Option<&PositionState>,
    side: OrderSide,
    quantity: u32,
    price_krw: i64,
    charges: TradeCharges,
) -> Result<TradeMutation, TradeFailure> {
    if !(1..=MAX_TRADE_QUANTITY).contains(&quantity) || price_krw <= 0 {
        return Err(TradeFailure::invalid_order(
            "주문 수량이나 체결가가 올바르지 않습니다",
        ));
    }
    if charges.fee_krw < 0 || charges.tax_krw < 0 {
        return Err(arithmetic_failure());
    }
    validate_position_for_trade(account_id, position)?;

    let gross_amount_krw =
        checked_money_product(price_krw, quantity).map_err(|_| arithmetic_failure())?;

    match side {
        OrderSide::Buy => apply_buy(
            account_id,
            account_cash_krw,
            position,
            quantity,
            gross_amount_krw,
            charges,
        ),
        OrderSide::Sell => apply_sell(
            account_id,
            account_cash_krw,
            position,
            quantity,
            gross_amount_krw,
            charges,
        ),
    }
}

pub fn value_portfolio(
    positions: &[PositionState],
    current_price_krw: i64,
) -> Result<Portfolio, TradingMathError> {
    if current_price_krw <= 0 {
        return Err(TradingMathError::NonPositivePrice);
    }

    for position in positions {
        validate_position(position)?;
    }
    let mut ordered = positions.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.account_id
            .cmp(&right.account_id)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    if ordered
        .windows(2)
        .any(|pair| pair[0].account_id == pair[1].account_id && pair[0].symbol == pair[1].symbol)
    {
        return Err(TradingMathError::InvalidPosition);
    }

    let mut valued_positions = Vec::with_capacity(ordered.len());
    let mut total_market_value_krw = 0_i64;
    for position in ordered {
        let quantity = i64::from(position.quantity);
        let market_value_krw = checked_money_product(current_price_krw, position.quantity)?;
        let average_price_krw = position
            .cost_basis_krw
            .checked_div(quantity)
            .ok_or(TradingMathError::ArithmeticOverflow)?;
        total_market_value_krw = total_market_value_krw
            .checked_add(market_value_krw)
            .ok_or(TradingMathError::ArithmeticOverflow)?;
        valued_positions.push(PortfolioPosition {
            account_id: position.account_id,
            symbol: position.symbol.clone(),
            quantity: position.quantity,
            cost_basis_krw: position.cost_basis_krw,
            average_price_krw,
            current_price_krw,
            market_value_krw,
        });
    }

    Ok(Portfolio {
        positions: valued_positions,
        market_value_krw: total_market_value_krw,
    })
}

pub fn checked_net_worth_krw(
    cash_krw: i64,
    debt_krw: i64,
    market_value_krw: i64,
) -> Result<i64, TradingMathError> {
    cash_krw
        .checked_sub(debt_krw)
        .and_then(|value| value.checked_add(market_value_krw))
        .ok_or(TradingMathError::ArithmeticOverflow)
}

fn apply_buy(
    account_id: AccountId,
    account_cash_krw: i64,
    position: Option<&PositionState>,
    quantity: u32,
    gross_amount_krw: i64,
    charges: TradeCharges,
) -> Result<TradeMutation, TradeFailure> {
    let current_quantity = position.map_or(0, |held| held.quantity);
    let next_quantity = current_quantity
        .checked_add(quantity)
        .ok_or_else(TradeFailure::position_limit)?;
    if next_quantity > MAX_TRADE_QUANTITY {
        return Err(TradeFailure::position_limit());
    }
    let cash_outflow_krw = gross_amount_krw
        .checked_add(charges.fee_krw)
        .and_then(|amount| amount.checked_add(charges.tax_krw))
        .ok_or_else(arithmetic_failure)?;
    if account_cash_krw < cash_outflow_krw {
        return Err(TradeFailure::insufficient_account_cash());
    }

    let next_cash_krw = account_cash_krw
        .checked_sub(cash_outflow_krw)
        .ok_or_else(arithmetic_failure)?;
    let current_cost_basis_krw = position.map_or(0, |held| held.cost_basis_krw);
    let next_cost_basis_krw = current_cost_basis_krw
        .checked_add(cash_outflow_krw)
        .ok_or_else(arithmetic_failure)?;

    Ok(TradeMutation {
        account_id,
        account_cash_krw: next_cash_krw,
        position: Some(PositionState {
            account_id,
            symbol: LLX_SYMBOL.to_owned(),
            quantity: next_quantity,
            cost_basis_krw: next_cost_basis_krw,
        }),
        gross_amount_krw,
        fee_krw: charges.fee_krw,
        tax_krw: charges.tax_krw,
        removed_cost_basis_krw: 0,
        realized_gain_loss_krw: 0,
    })
}

fn apply_sell(
    account_id: AccountId,
    account_cash_krw: i64,
    position: Option<&PositionState>,
    quantity: u32,
    gross_amount_krw: i64,
    charges: TradeCharges,
) -> Result<TradeMutation, TradeFailure> {
    let Some(position) = position else {
        return Err(TradeFailure::insufficient_quantity());
    };
    if position.quantity < quantity {
        return Err(TradeFailure::insufficient_quantity());
    }

    let net_proceeds_krw = gross_amount_krw
        .checked_sub(charges.fee_krw)
        .and_then(|amount| amount.checked_sub(charges.tax_krw))
        .ok_or_else(arithmetic_failure)?;
    if net_proceeds_krw < 0 {
        return Err(arithmetic_failure());
    }
    let next_cash_krw = account_cash_krw
        .checked_add(net_proceeds_krw)
        .ok_or_else(arithmetic_failure)?;
    let removed_cost_basis_krw = if quantity == position.quantity {
        position.cost_basis_krw
    } else {
        let weighted = i128::from(position.cost_basis_krw)
            .checked_mul(i128::from(quantity))
            .ok_or_else(arithmetic_failure)?;
        let released = weighted
            .checked_div(i128::from(position.quantity))
            .ok_or_else(arithmetic_failure)?;
        i64::try_from(released).map_err(|_| arithmetic_failure())?
    };
    let next_quantity = position.quantity - quantity;
    let next_cost_basis_krw = position
        .cost_basis_krw
        .checked_sub(removed_cost_basis_krw)
        .ok_or_else(arithmetic_failure)?;
    let next_position = (next_quantity > 0).then(|| PositionState {
        account_id,
        symbol: LLX_SYMBOL.to_owned(),
        quantity: next_quantity,
        cost_basis_krw: next_cost_basis_krw,
    });
    let realized_gain_loss_krw = net_proceeds_krw
        .checked_sub(removed_cost_basis_krw)
        .ok_or_else(arithmetic_failure)?;

    Ok(TradeMutation {
        account_id,
        account_cash_krw: next_cash_krw,
        position: next_position,
        gross_amount_krw,
        fee_krw: charges.fee_krw,
        tax_krw: charges.tax_krw,
        removed_cost_basis_krw,
        realized_gain_loss_krw,
    })
}

fn validate_position_for_trade(
    account_id: AccountId,
    position: Option<&PositionState>,
) -> Result<(), TradeFailure> {
    match position {
        Some(position)
            if position.account_id != account_id || validate_position(position).is_err() =>
        {
            Err(TradeFailure::invalid_order(
                "저장된 보유 상태가 올바르지 않습니다",
            ))
        }
        _ => Ok(()),
    }
}

fn validate_position(position: &PositionState) -> Result<(), TradingMathError> {
    if position.symbol != LLX_SYMBOL
        || !(1..=MAX_TRADE_QUANTITY).contains(&position.quantity)
        || position.cost_basis_krw <= 0
    {
        return Err(TradingMathError::InvalidPosition);
    }

    Ok(())
}

const fn arithmetic_failure() -> TradeFailure {
    TradeFailure::invalid_order("주문 금액이 처리 범위를 초과했습니다")
}

fn checked_money_product(value_krw: i64, quantity: u32) -> Result<i64, TradingMathError> {
    let product = i128::from(value_krw)
        .checked_mul(i128::from(quantity))
        .ok_or(TradingMathError::ArithmeticOverflow)?;

    i64::try_from(product).map_err(|_| TradingMathError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading::TradeFailureCode;

    fn given_account_id(value: u64) -> AccountId {
        AccountId::from_u64(value).expect("0이 아닌 계좌 ID여야 한다")
    }

    fn given_position(quantity: u32, cost_basis_krw: i64) -> PositionState {
        given_position_for(given_account_id(7), quantity, cost_basis_krw)
    }

    fn given_position_for(
        account_id: AccountId,
        quantity: u32,
        cost_basis_krw: i64,
    ) -> PositionState {
        PositionState {
            account_id,
            symbol: LLX_SYMBOL.to_owned(),
            quantity,
            cost_basis_krw,
        }
    }

    mod context_a_buy_order_is_applied {
        use super::*;

        #[test]
        fn given_enough_cash_when_buying_then_cash_and_cost_basis_move_by_the_gross_amount() {
            let position = given_position(2, 210);

            let mutation = apply_trade(
                given_account_id(7),
                1_000,
                Some(&position),
                OrderSide::Buy,
                3,
                100,
            )
            .expect("매수할 수 있어야 한다");

            assert_eq!(mutation.account_cash_krw, 700);
            assert_eq!(mutation.gross_amount_krw, 300);
            assert_eq!(mutation.position, Some(given_position(5, 510)));
        }

        #[test]
        fn given_insufficient_account_cash_when_buying_then_the_order_is_rejected() {
            let failure = apply_trade(given_account_id(7), 299, None, OrderSide::Buy, 3, 100)
                .expect_err("현금보다 큰 주문이어야 한다");

            assert_eq!(
                failure.code,
                super::TradeFailureCode::InsufficientAccountCash
            );
        }

        #[test]
        fn given_the_position_limit_when_buying_more_then_the_order_is_rejected() {
            let position = given_position(MAX_TRADE_QUANTITY, 1_000_000);

            let failure = apply_trade(
                given_account_id(7),
                i64::MAX,
                Some(&position),
                OrderSide::Buy,
                1,
                1,
            )
            .expect_err("보유 한도를 넘겨야 한다");

            assert_eq!(failure.code, super::TradeFailureCode::PositionLimit);
        }

        #[test]
        fn given_no_position_when_buying_then_the_selected_account_id_is_attached() {
            let account_id = given_account_id(91);

            let mutation = apply_trade(account_id, 1_000, None, OrderSide::Buy, 1, 100)
                .expect("새 포지션을 만들 수 있어야 한다");

            assert_eq!(
                (
                    mutation.account_id,
                    mutation.position.map(|position| position.account_id),
                ),
                (account_id, Some(account_id))
            );
        }

        #[test]
        fn given_a_position_from_another_account_when_buying_then_the_state_is_rejected() {
            let position = given_position_for(given_account_id(8), 1, 100);

            let failure = apply_trade(
                given_account_id(7),
                1_000,
                Some(&position),
                OrderSide::Buy,
                1,
                100,
            )
            .expect_err("다른 계좌 포지션을 바꾸면 안 된다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }
    }

    mod context_a_sell_order_is_applied {
        use super::*;

        #[test]
        fn given_a_partial_sale_when_selling_then_cost_basis_is_released_with_floor_rounding() {
            let position = given_position(3, 100);

            let mutation = apply_trade(
                given_account_id(7),
                50,
                Some(&position),
                OrderSide::Sell,
                1,
                120,
            )
            .expect("부분 매도할 수 있어야 한다");

            assert_eq!(mutation.removed_cost_basis_krw, 33);
            assert_eq!(mutation.position, Some(given_position(2, 67)));
            assert_eq!(mutation.account_cash_krw, 170);
        }

        #[test]
        fn given_a_full_sale_when_selling_then_the_whole_basis_and_position_are_removed() {
            let position = given_position(3, 100);

            let mutation = apply_trade(
                given_account_id(7),
                50,
                Some(&position),
                OrderSide::Sell,
                3,
                120,
            )
            .expect("전량 매도할 수 있어야 한다");

            assert_eq!(mutation.removed_cost_basis_krw, 100);
            assert_eq!(mutation.position, None);
        }

        #[test]
        fn given_too_few_shares_when_selling_then_the_order_is_rejected() {
            let position = given_position(2, 200);

            let failure = apply_trade(
                given_account_id(7),
                0,
                Some(&position),
                OrderSide::Sell,
                3,
                100,
            )
            .expect_err("보유수량보다 많이 팔 수 없어야 한다");

            assert_eq!(failure.code, super::TradeFailureCode::InsufficientQuantity);
        }
    }

    mod context_상품_비용이_있는_llx_주문을_적용할_때 {
        use super::*;

        #[test]
        fn given_매수_수수료와_세금_when_매수하면_then_현금유출과_취득원가에_모두_포함한다() {
            let mutation = apply_trade_with_charges(
                given_account_id(7),
                1_000,
                None,
                OrderSide::Buy,
                2,
                100,
                TradeCharges {
                    fee_krw: 3,
                    tax_krw: 2,
                },
            )
            .expect("비용을 포함한 매수가 가능해야 한다");

            assert_eq!(mutation.account_cash_krw, 795);
            assert_eq!(mutation.position, Some(given_position(2, 205)));
            assert_eq!((mutation.fee_krw, mutation.tax_krw), (3, 2));
        }

        #[test]
        fn given_매도_수수료와_세금_when_매도하면_then_순입금과_실현손익에서_각각_차감한다() {
            let position = given_position(2, 160);

            let mutation = apply_trade_with_charges(
                given_account_id(7),
                10,
                Some(&position),
                OrderSide::Sell,
                1,
                100,
                TradeCharges {
                    fee_krw: 3,
                    tax_krw: 2,
                },
            )
            .expect("비용을 포함한 매도가 가능해야 한다");

            assert_eq!(mutation.account_cash_krw, 105);
            assert_eq!(mutation.removed_cost_basis_krw, 80);
            assert_eq!(mutation.realized_gain_loss_krw, 15);
        }

        #[test]
        fn given_음수_비용_when_주문하면_then_잘못된_주문으로_거절한다() {
            let failure = apply_trade_with_charges(
                given_account_id(7),
                1_000,
                None,
                OrderSide::Buy,
                1,
                100,
                TradeCharges {
                    fee_krw: -1,
                    tax_krw: 0,
                },
            )
            .expect_err("음수 비용은 거절해야 한다");

            assert_eq!(failure.code, TradeFailureCode::InvalidOrder);
        }
    }

    mod context_trading_arithmetic_exceeds_the_money_range {
        use super::*;

        #[test]
        fn given_an_overflowing_gross_when_applied_then_the_order_is_rejected() {
            let failure = apply_trade(
                given_account_id(7),
                i64::MAX,
                None,
                OrderSide::Buy,
                2,
                i64::MAX,
            )
            .expect_err("총액이 범위를 넘어야 한다");

            assert_eq!(failure.code, super::TradeFailureCode::InvalidOrder);
        }

        #[test]
        fn given_an_i64_overflowing_intermediate_when_selling_then_i128_preserves_the_valid_quotient()
         {
            let position = given_position(3, i64::MAX);

            let mutation = apply_trade(
                given_account_id(7),
                0,
                Some(&position),
                OrderSide::Sell,
                2,
                1,
            )
            .expect("최종 몫이 i64 범위이면 성공해야 한다");

            assert_eq!(
                mutation.removed_cost_basis_krw,
                i64::try_from(i128::from(i64::MAX) * 2 / 3).expect("i64 범위여야 한다")
            );
        }
    }

    mod context_a_portfolio_is_valued {
        use super::*;

        #[test]
        fn given_a_position_when_valued_then_integer_average_and_market_value_are_returned() {
            let position = given_position(3, 100);

            let portfolio = value_portfolio(&[position], 120).expect("평가할 수 있어야 한다");

            assert_eq!(portfolio.market_value_krw, 360);
            assert_eq!(portfolio.positions[0].average_price_krw, 33);
            assert_eq!(portfolio.positions[0].market_value_krw, 360);
        }

        #[test]
        fn given_positions_from_several_accounts_when_valued_then_output_is_ordered_by_account_id()
        {
            let positions = [
                given_position_for(given_account_id(20), 2, 220),
                given_position_for(given_account_id(3), 1, 90),
            ];

            let portfolio = value_portfolio(&positions, 100).expect("여러 계좌를 평가해야 한다");

            let account_ids = portfolio
                .positions
                .iter()
                .map(|position| position.account_id.get())
                .collect::<Vec<_>>();

            assert_eq!(
                (account_ids, portfolio.market_value_krw),
                (vec![3, 20], 300)
            );
        }

        #[test]
        fn given_no_positions_when_valued_then_the_empty_portfolio_is_preserved() {
            let portfolio = value_portfolio(&[], 100).expect("빈 포트폴리오도 평가해야 한다");

            assert_eq!(
                portfolio,
                Portfolio {
                    positions: vec![],
                    market_value_krw: 0,
                }
            );
        }

        #[test]
        fn given_money_components_when_summed_then_net_worth_uses_checked_arithmetic() {
            let net_worth =
                checked_net_worth_krw(1_000, 300, 250).expect("순자산을 계산할 수 있어야 한다");

            assert_eq!(net_worth, 950);
        }

        #[test]
        fn given_an_overflowing_market_value_when_valued_then_the_snapshot_math_fails() {
            let position = given_position(2, 2);

            let portfolio = value_portfolio(&[position], i64::MAX);

            assert_eq!(portfolio, Err(TradingMathError::ArithmeticOverflow));
        }

        #[test]
        fn given_individually_valid_values_when_their_sum_overflows_then_valuation_fails() {
            let positions = [
                given_position_for(given_account_id(1), 1, 1),
                given_position_for(given_account_id(2), 1, 1),
            ];

            let portfolio = value_portfolio(&positions, i64::MAX);

            assert_eq!(portfolio, Err(TradingMathError::ArithmeticOverflow));
        }

        #[test]
        fn given_duplicate_account_positions_when_valued_then_the_invalid_state_is_rejected() {
            let positions = [given_position(1, 1), given_position(2, 2)];

            let portfolio = value_portfolio(&positions, 100);

            assert_eq!(portfolio, Err(TradingMathError::InvalidPosition));
        }

        #[test]
        fn given_an_overflowing_net_worth_when_summed_then_the_snapshot_math_fails() {
            let net_worth = checked_net_worth_krw(i64::MAX, 0, 1);

            assert_eq!(net_worth, Err(TradingMathError::ArithmeticOverflow));
        }
    }
}
