import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  APP_NOTIFICATIONS,
  APP_REQUESTS,
  PROGRAM_NOTIFICATIONS,
  PROGRAM_REQUESTS,
} from "../dist/index.js";

const schemaPath = fileURLToPath(new URL("../../schema/krad-extension-api.json", import.meta.url));
const schema = JSON.parse(readFileSync(schemaPath, "utf8"));

const sides = {
  program: { requests: PROGRAM_REQUESTS, notifications: PROGRAM_NOTIFICATIONS },
  app: { requests: APP_REQUESTS, notifications: APP_NOTIFICATIONS },
};

const problems = [];
for (const [method, entry] of Object.entries(schema.requests)) {
  if (!sides[entry.answered_by].requests.includes(method)) {
    problems.push(`request ${method} answered by ${entry.answered_by} is missing from the SDK`);
  }
}
for (const [method, entry] of Object.entries(schema.notifications)) {
  if (!sides[entry.received_by].notifications.includes(method)) {
    problems.push(`notification ${method} received by ${entry.received_by} is missing from the SDK`);
  }
}
const known = new Set([...Object.keys(schema.requests), ...Object.keys(schema.notifications)]);
for (const method of [...PROGRAM_REQUESTS, ...APP_REQUESTS, ...PROGRAM_NOTIFICATIONS, ...APP_NOTIFICATIONS]) {
  if (!known.has(method)) {
    problems.push(`the SDK names ${method}, which the schema does not have`);
  }
}
if (problems.length > 0) {
  console.error(problems.join("\n"));
  process.exit(1);
}
console.log(`the SDK covers all ${known.size} methods of API ${schema.api}`);
