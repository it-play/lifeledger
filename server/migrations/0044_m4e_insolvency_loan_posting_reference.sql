-- The loan posting guard runs before the insolvency guard, so it must preserve a valid
-- insolvency distribution reference for the final domain-specific guard to validate.
DROP TRIGGER tr_ledger_posting_loan_reference_insert;

CREATE TRIGGER tr_ledger_posting_loan_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_life_reference_insert
SET NEW.account_code = IF(
    (
        NEW.account_code IN (
            'loanPrincipalLiability', 'loanInterestExpense',
            'loanInterestLiability', 'loanFeeExpense'
        )
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN loan_contract AS contract
                ON contract.id = NEW.loan_contract_id
               AND contract.save_id = ledger.save_id
               AND contract.run_revision = ledger.run_revision
            LEFT JOIN loan_payment AS payment
                ON payment.loan_contract_id = contract.id
               AND (
                   BINARY CAST(payment.id AS CHAR) = BINARY ledger.source_id
                   OR (
                       ledger.source_kind = 'propertySale'
                       AND BINARY CAST(payment.property_sale_execution_id AS CHAR)
                             = BINARY ledger.source_id
                   )
               )
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND (
                  (
                      ledger.source_kind IN ('loanOrigination', 'debtAuthorityBridge')
                      AND BINARY ledger.source_id = BINARY CAST(contract.id AS CHAR)
                  )
                  OR (
                      ledger.source_kind IN ('loanInstallment', 'loanPrepayment')
                      AND payment.status = 'prepared'
                  )
                  OR (
                      ledger.source_kind = 'insolvencyDistribution'
                      AND payment.payment_kind = 'insolvencyDistribution'
                      AND payment.status = 'prepared'
                      AND payment.insolvency_case_id IS NOT NULL
                  )
                  OR (
                      ledger.source_kind = 'leaseMove'
                      AND NEW.account_code = 'loanPrincipalLiability'
                      AND (
                          (
                              contract.origin_kind = 'leaseDepositExecution'
                              AND contract.product_kind = 'leaseDepositLoan'
                              AND BINARY contract.origin_command_id
                                    = BINARY ledger.source_id
                              AND contract.activated_game_day = ledger.game_day
                              AND NEW.amount_krw = -contract.original_principal_krw
                          )
                          OR EXISTS (
                              SELECT 1 FROM loan_payment AS payoff
                              WHERE payoff.loan_contract_id = contract.id
                                AND payoff.save_id = ledger.save_id
                                AND payoff.run_revision = ledger.run_revision
                                AND payoff.payment_kind = 'leaseMovePayoff'
                                AND payoff.status = 'prepared'
                                AND payoff.game_day = ledger.game_day
                                AND BINARY payoff.command_id = BINARY ledger.source_id
                                AND NEW.amount_krw = payoff.amount_krw
                          )
                      )
                  )
                  OR (
                      ledger.source_kind = 'propertyPurchase'
                      AND NEW.account_code = 'loanPrincipalLiability'
                      AND (
                          (
                              contract.origin_kind = 'mortgagePurchaseExecution'
                              AND contract.product_kind = 'mortgage'
                              AND BINARY contract.origin_command_id
                                    = BINARY ledger.source_id
                              AND contract.activated_game_day = ledger.game_day
                              AND NEW.amount_krw = -contract.original_principal_krw
                          )
                          OR EXISTS (
                              SELECT 1 FROM loan_payment AS payoff
                              WHERE payoff.loan_contract_id = contract.id
                                AND payoff.save_id = ledger.save_id
                                AND payoff.run_revision = ledger.run_revision
                                AND payoff.payment_kind = 'leaseMovePayoff'
                                AND payoff.status = 'prepared'
                                AND payoff.game_day = ledger.game_day
                                AND BINARY payoff.command_id = BINARY ledger.source_id
                                AND NEW.amount_krw = payoff.amount_krw
                          )
                      )
                  )
                  OR (
                      ledger.source_kind = 'propertySale'
                      AND NEW.account_code IN (
                          'loanPrincipalLiability', 'loanFeeExpense'
                      )
                      AND payment.payment_kind = 'propertySalePayoff'
                      AND payment.status = 'prepared'
                      AND EXISTS (
                          SELECT 1
                          FROM property_sale_execution AS execution
                          WHERE execution.id = payment.property_sale_execution_id
                            AND execution.save_id = ledger.save_id
                            AND execution.run_revision = ledger.run_revision
                            AND execution.status = 'prepared'
                            AND BINARY ledger.source_id
                                  = BINARY CAST(execution.id AS CHAR)
                            AND (
                                (
                                    NEW.account_code = 'loanPrincipalLiability'
                                    AND NEW.amount_krw
                                          = execution.mortgage_principal_krw
                                )
                                OR (
                                    NEW.account_code = 'loanFeeExpense'
                                    AND NEW.amount_krw
                                          = execution.mortgage_prepayment_fee_krw
                                )
                            )
                      )
                  )
              )
        )
    )
    OR (
        NEW.account_code = 'taxObligationLiability'
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN tax_obligation AS obligation
                ON obligation.id = NEW.tax_obligation_id
               AND obligation.save_id = ledger.save_id
               AND obligation.run_revision = ledger.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND obligation.status IN ('prepared', 'outstanding')
        )
    )
    OR (
        NEW.account_code NOT IN (
            'loanPrincipalLiability', 'loanInterestExpense',
            'loanInterestLiability', 'loanFeeExpense', 'taxObligationLiability'
        )
        AND NEW.loan_contract_id IS NULL
        AND NEW.tax_obligation_id IS NULL
    ),
    NEW.account_code,
    NULL
);
