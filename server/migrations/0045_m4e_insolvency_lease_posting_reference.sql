-- The lease posting guard also protects loan principal references. Preserve an insolvency
-- distribution after the loan guard has validated its contract and payment authority.
DROP TRIGGER tr_ledger_posting_lease_reference_insert;

CREATE TRIGGER tr_ledger_posting_lease_reference_insert
BEFORE INSERT ON ledger_posting
FOR EACH ROW
FOLLOWS tr_ledger_posting_loan_reference_insert
SET NEW.account_code = IF(
    (
        NEW.account_code = 'leaseDepositAsset'
        AND EXISTS (
            SELECT 1
            FROM ledger_transaction AS ledger
            INNER JOIN lease_contract AS contract
                ON contract.id = NEW.lease_contract_id
               AND contract.save_id = ledger.save_id
               AND contract.run_revision = ledger.run_revision
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND (
                  (
                      ledger.source_kind = 'leaseMove'
                      AND (
                          (
                              NEW.amount_krw = contract.deposit_krw
                              AND contract.effective_from_game_day = ledger.game_day
                              AND BINARY contract.command_id = BINARY ledger.source_id
                          )
                          OR (
                              NEW.amount_krw = -contract.deposit_krw
                              AND contract.effective_to_game_day = ledger.game_day
                          )
                      )
                  )
                  OR (
                      ledger.source_kind = 'propertyPurchase'
                      AND NEW.amount_krw = -contract.deposit_krw
                      AND contract.effective_to_game_day = ledger.game_day
                  )
              )
        )
    )
    OR (
        NEW.account_code = 'movingExpense'
        AND NEW.lease_contract_id IS NULL
        AND (
            EXISTS (
                SELECT 1
                FROM ledger_transaction AS ledger
                INNER JOIN lease_contract AS contract
                    ON contract.save_id = ledger.save_id
                   AND contract.run_revision = ledger.run_revision
                   AND BINARY contract.command_id = BINARY ledger.source_id
                INNER JOIN real_estate_region_moving_cost AS moving_cost
                    ON moving_cost.real_estate_model_version_id
                            = contract.real_estate_model_version_id
                   AND BINARY moving_cost.region_key = BINARY contract.region_key
                WHERE ledger.id = NEW.ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.source_kind = 'leaseMove'
                  AND NEW.amount_krw = moving_cost.moving_cost_krw
            )
            OR EXISTS (
                SELECT 1
                FROM ledger_transaction AS ledger
                INNER JOIN property_holding AS holding
                    ON holding.save_id = ledger.save_id
                   AND holding.run_revision = ledger.run_revision
                   AND BINARY holding.acquisition_command_id = BINARY ledger.source_id
                INNER JOIN real_estate_region_moving_cost AS moving_cost
                    ON moving_cost.real_estate_model_version_id
                            = holding.real_estate_model_version_id
                   AND BINARY moving_cost.region_key = BINARY holding.region_key
                WHERE ledger.id = NEW.ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.source_kind = 'propertyPurchase'
                  AND NEW.amount_krw = moving_cost.moving_cost_krw
            )
        )
    )
    OR (
        NEW.account_code = 'wallet'
        AND NEW.lease_contract_id IS NULL
        AND NEW.loan_contract_id IS NULL
        AND (
            EXISTS (
                SELECT 1
                FROM ledger_transaction AS ledger
                INNER JOIN lease_contract AS started_contract
                    ON started_contract.save_id = ledger.save_id
                   AND started_contract.run_revision = ledger.run_revision
                   AND BINARY started_contract.command_id = BINARY ledger.source_id
                INNER JOIN real_estate_region_moving_cost AS moving_cost
                    ON moving_cost.real_estate_model_version_id
                            = started_contract.real_estate_model_version_id
                   AND BINARY moving_cost.region_key = BINARY started_contract.region_key
                LEFT JOIN loan_contract AS originated_loan
                    ON originated_loan.save_id = started_contract.save_id
                   AND originated_loan.run_revision = started_contract.run_revision
                   AND originated_loan.lease_contract_id = started_contract.id
                   AND originated_loan.origin_kind = 'leaseDepositExecution'
                WHERE ledger.id = NEW.ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.source_kind = 'leaseMove'
                  AND (
                      NEW.amount_krw = -(
                          started_contract.deposit_krw
                          - COALESCE(originated_loan.original_principal_krw, 0)
                      )
                      OR NEW.amount_krw = -moving_cost.moving_cost_krw
                      OR EXISTS (
                          SELECT 1
                          FROM lease_contract AS ended_contract
                          LEFT JOIN loan_contract AS ended_loan
                              ON ended_loan.save_id = ended_contract.save_id
                             AND ended_loan.run_revision = ended_contract.run_revision
                             AND ended_loan.lease_contract_id = ended_contract.id
                          LEFT JOIN loan_payment AS payoff
                              ON payoff.loan_contract_id = ended_loan.id
                             AND payoff.save_id = ended_contract.save_id
                             AND payoff.run_revision = ended_contract.run_revision
                             AND payoff.payment_kind = 'leaseMovePayoff'
                             AND payoff.status = 'prepared'
                             AND BINARY payoff.command_id = BINARY ledger.source_id
                          WHERE ended_contract.save_id = ledger.save_id
                            AND ended_contract.run_revision = ledger.run_revision
                            AND ended_contract.household_id
                                  = started_contract.household_id
                            AND ended_contract.effective_to_game_day = ledger.game_day
                            AND NEW.amount_krw = ended_contract.deposit_krw
                                  - COALESCE(payoff.amount_krw, 0)
                      )
                  )
            )
            OR EXISTS (
                SELECT 1 FROM ledger_transaction AS ledger
                WHERE ledger.id = NEW.ledger_transaction_id
                  AND ledger.save_id = NEW.save_id
                  AND ledger.run_revision = NEW.run_revision
                  AND ledger.source_kind = 'propertyPurchase'
            )
        )
    )
    OR (
        NEW.account_code = 'loanPrincipalLiability'
        AND NEW.lease_contract_id IS NULL
        AND NEW.loan_contract_id IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind IN (
                  'loanOrigination', 'debtAuthorityBridge',
                  'loanInstallment', 'loanPrepayment',
                  'leaseMove', 'propertyPurchase', 'propertySale',
                  'insolvencyDistribution'
              )
        )
    )
    OR (
        NEW.account_code NOT IN (
            'leaseDepositAsset', 'movingExpense', 'loanPrincipalLiability'
        )
        AND NOT EXISTS (
            SELECT 1 FROM ledger_transaction AS ledger
            WHERE ledger.id = NEW.ledger_transaction_id
              AND ledger.save_id = NEW.save_id
              AND ledger.run_revision = NEW.run_revision
              AND ledger.source_kind = 'leaseMove'
        )
    ),
    NEW.account_code,
    NULL
);
