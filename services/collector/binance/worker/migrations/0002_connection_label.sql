-- A person can connect several exchange accounts under one identity;
-- the label ("Main", "Long-term", ...) is only how they tell them apart.
ALTER TABLE binance_connections ADD COLUMN label TEXT;
