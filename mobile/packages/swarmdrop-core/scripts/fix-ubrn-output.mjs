#!/usr/bin/env node
// Workaround for uniffi-bindgen-react-native generator output on async constructors.

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const generatedDir = join(__dirname, "..", "src", "generated");
const pattern = /\basync\s+static\b/g;

function walk(dir) {
  const entries = readdirSync(dir);
  for (const entry of entries) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      walk(full);
    } else if (entry.endsWith(".ts")) {
      fix(full);
    }
  }
}

function fix(file) {
  const before = readFileSync(file, "utf8");
  if (!pattern.test(before)) {
    return;
  }
  writeFileSync(file, before.replace(pattern, "static async"));
  console.log(`[fix-ubrn] patched async-static in ${file}`);
}

if (existsSync(generatedDir)) {
  walk(generatedDir);
}
