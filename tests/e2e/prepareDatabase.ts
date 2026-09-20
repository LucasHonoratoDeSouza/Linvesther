// Creates this run's database. Playwright starts the API before globalSetup
// runs, and the API needs its database at start-up, so the API's start command
// runs this first.
import { createThrowawayDatabase } from "./testDatabase.js";

const name = process.env.E2E_DATABASE_NAME;
if (!name) throw new Error("E2E_DATABASE_NAME is not set");
try {
  await createThrowawayDatabase("linvesther_e2e", name);
} catch (error) {
  // Already there: another start of the same run.
  if (!String((error as Error).message).includes("already exists")) throw error;
}
