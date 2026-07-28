-- A defaulted contract cancels future installments before insolvency can distribute cash.
-- Keep those unmaterialized claim components explicit without inventing obligation buckets.
ALTER TABLE loan_payment_allocation
    DROP CHECK ck_loan_allocation_kind,
    ADD CONSTRAINT ck_loan_allocation_kind CHECK (
        allocation_kind IN (
            'overdueFee', 'overdueInterest', 'overduePrincipal',
            'currentFee', 'currentInterest', 'currentPrincipal',
            'prepaymentFee', 'prepaymentPrincipal'
        )
        AND (
            (
                allocation_kind IN (
                    'overdueFee', 'overdueInterest', 'overduePrincipal',
                    'currentFee', 'currentInterest', 'currentPrincipal'
                )
                AND loan_obligation_bucket_id IS NOT NULL
            )
            OR (
                allocation_kind IN (
                    'currentFee', 'currentInterest', 'currentPrincipal',
                    'prepaymentFee', 'prepaymentPrincipal'
                )
                AND loan_obligation_bucket_id IS NULL
            )
        )
    );

DROP TRIGGER tr_loan_allocation_valid_insert;

CREATE TRIGGER tr_loan_allocation_valid_insert
BEFORE INSERT ON loan_payment_allocation
FOR EACH ROW
SET NEW.save_id = IF(
    EXISTS (
        SELECT 1
        FROM loan_payment AS payment
        WHERE payment.id = NEW.loan_payment_id
          AND payment.save_id = NEW.save_id
          AND payment.run_revision = NEW.run_revision
          AND payment.loan_contract_id = NEW.loan_contract_id
          AND payment.status = 'prepared'
          AND (
              (
                  NEW.loan_obligation_bucket_id IS NOT NULL
                  AND NEW.allocation_kind IN (
                      'overdueFee', 'overdueInterest', 'overduePrincipal',
                      'currentFee', 'currentInterest', 'currentPrincipal'
                  )
                  AND EXISTS (
                      SELECT 1
                      FROM loan_obligation_bucket AS bucket
                      WHERE bucket.id = NEW.loan_obligation_bucket_id
                        AND bucket.save_id = NEW.save_id
                        AND bucket.run_revision = NEW.run_revision
                        AND bucket.loan_contract_id = NEW.loan_contract_id
                        AND bucket.status IN ('pending', 'delinquent')
                  )
              )
              OR (
                  NEW.loan_obligation_bucket_id IS NULL
                  AND NEW.allocation_kind IN ('prepaymentFee', 'prepaymentPrincipal')
                  AND payment.payment_kind IN (
                      'manualPrepayment', 'leaseMovePayoff', 'propertySalePayoff'
                  )
              )
              OR (
                  NEW.loan_obligation_bucket_id IS NULL
                  AND NEW.allocation_kind IN (
                      'currentFee', 'currentInterest', 'currentPrincipal'
                  )
                  AND payment.payment_kind = 'insolvencyDistribution'
              )
          )
    ),
    NEW.save_id,
    NULL
);
