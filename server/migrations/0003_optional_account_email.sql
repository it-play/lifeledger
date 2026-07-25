-- A provider is not obliged to return an email: Google omits it when the `email` scope is
-- declined, and a DataGSM account may have none. The column carried NOT NULL over from the
-- password-login schema, where the address was the login itself.

ALTER TABLE user MODIFY COLUMN email VARCHAR(254) NULL;
