-- M2-C cash products may settle through taxable, ISA, and pension accounts.

DROP TRIGGER tr_cash_product_contract_valid_insert;

-- Contracts may only draw from an allowed open parent account, and all catalog terms are copied.
CREATE TRIGGER tr_cash_product_contract_valid_insert
BEFORE INSERT ON cash_product_contract
FOR EACH ROW
SET NEW.product_version_id = IF(
    NEW.status = 'active'
        AND NEW.closed_game_day IS NULL
        AND NEW.closing_ledger_transaction_id IS NULL
        AND NEW.cancellation_reason IS NULL
        AND EXISTS (
            SELECT 1
            FROM financial_account AS account
            WHERE account.id = NEW.financial_account_id
              AND account.save_id = NEW.save_id
              AND account.run_revision = NEW.run_revision
              AND account.account_type IN (
                  'taxableBrokerage',
                  'isaGeneral',
                  'isaLowIncome',
                  'pensionSavings',
                  'irp'
              )
              AND account.status = 'open'
        )
        AND EXISTS (
            SELECT 1
            FROM cash_product_version AS product
            WHERE product.id = NEW.product_version_id
              AND BINARY product.product_kind = BINARY NEW.contract_kind
              AND product.early_termination_rate_bp = NEW.early_termination_rate_bp
              AND product.day_count_denominator = NEW.day_count_denominator
              AND (
                  (
                      NEW.contract_kind = 'termDeposit'
                      AND NEW.principal_krw BETWEEN
                          product.minimum_amount_krw AND product.maximum_amount_krw
                      AND product.term_days = NEW.term_days
                      AND NEW.maturity_game_day = NEW.opened_game_day + NEW.term_days
                  )
                  OR
                  (
                      NEW.contract_kind = 'installmentSavings'
                      AND NEW.installment_amount_krw BETWEEN
                          product.minimum_amount_krw AND product.maximum_amount_krw
                      AND product.term_months = NEW.term_months
                      AND product.installment_count = NEW.installment_count
                  )
              )
        ),
    NEW.product_version_id,
    NULL
);
