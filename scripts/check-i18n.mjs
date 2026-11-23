/**
 * Locale parity: every catalog matches English keys, placeholders, and ** markup.
 * English is the source of truth. Brand names and about.hello stay verbatim.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "src", "i18n", "locales");
const files = [
  "en",
  "es",
  "fr",
  "de",
  "pt-BR",
  "it",
  "ru",
  "pl",
  "ja",
  "ko",
  "zh-CN",
];
const keepExact = new Set(["about.hello"]);
const brandTokens = ["SecretFolder", "GitHub", "Patreon", "Ahmi", "MIT", "Windows"];

function placeholders(value) {
  const found = new Set();
  for (const match of value.matchAll(/\{([A-Za-z0-9_]+)\}/g)) found.add(match[1]);
  return found;
}

function starPairs(value) {
  return (value.match(/\*\*/g) ?? []).length;
}

function load(name) {
  const path = join(root, `${name}.json`);
  let parsed;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch (err) {
    console.error(`${name}.json: invalid JSON (${err.message})`);
    process.exit(1);
  }
  if (parsed == null || typeof parsed !== "object" || Array.isArray(parsed)) {
    console.error(`${name}.json: expected a JSON object`);
    process.exit(1);
  }
  const flat = {};
  const walk = (node, prefix) => {
    for (const [key, value] of Object.entries(node)) {
      const next = prefix ? `${prefix}.${key}` : key;
      if (value && typeof value === "object" && !Array.isArray(value)) walk(value, next);
      else flat[next] = value;
    }
  };
  walk(parsed, "");
  return flat;
}

const catalogs = Object.fromEntries(files.map((name) => [name, load(name)]));
const english = catalogs.en;
const englishKeys = Object.keys(english).sort();
let failed = 0;

function fail(message) {
  console.error(message);
  failed += 1;
}

for (const key of englishKeys) {
  const value = english[key];
  if (typeof value !== "string" || value.length === 0) {
    fail(`en.json: ${key} is empty or not a string`);
  }
}

for (const name of files) {
  if (name === "en") continue;
  const catalog = catalogs[name];
  const keys = Object.keys(catalog).sort();
  const missing = englishKeys.filter((key) => !Object.prototype.hasOwnProperty.call(catalog, key));
  const extra = keys.filter((key) => !Object.prototype.hasOwnProperty.call(english, key));
  if (missing.length) fail(`${name}.json missing keys: ${missing.join(", ")}`);
  if (extra.length) fail(`${name}.json extra keys: ${extra.join(", ")}`);
  for (const key of englishKeys) {
    const value = catalog[key];
    const source = english[key];
    if (typeof value !== "string" || value.length === 0) {
      fail(`${name}.json: ${key} is empty or not a string`);
      continue;
    }
    if (typeof source !== "string") continue;
    const want = [...placeholders(source)].sort().join(",");
    const got = [...placeholders(value)].sort().join(",");
    if (want !== got) {
      fail(`${name}.json: ${key} placeholders {${got}} != en {${want}}`);
    }
    if (starPairs(value) !== starPairs(source)) {
      fail(
        `${name}.json: ${key} has ${starPairs(value)} ** pairs, en has ${starPairs(source)}`,
      );
    }
    if (keepExact.has(key) && value !== source) {
      fail(`${name}.json: ${key} must stay exactly ${JSON.stringify(source)}`);
    }
    for (const token of brandTokens) {
      if (source.includes(token) && !value.includes(token)) {
        fail(`${name}.json: ${key} dropped brand token ${token}`);
      }
    }
  }
}

if (failed) {
  console.error(`i18n check failed (${failed})`);
  process.exit(1);
}

console.log(`i18n ok: ${englishKeys.length} keys × ${files.length} locales`);
