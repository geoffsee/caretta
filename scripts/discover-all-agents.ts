#!/usr/bin/env bun
/**
 * Agent CLI Discovery
 * 
 * This script runs the discover-help tool against every supported agent CLI
 * found in the PATH.
 * 
 * Requirement:
 *   OPENAI_API_KEY must be set in the environment.
 *   The 'discover-help' binary must be available in 'scripts/'.
 */
import { $ } from "bun";
import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const AGENTS = ["claude", "cline", "codex", "copilot", "gemini", "junie", "xai"];
const OUTPUT_DIR = "agent_discovery_results";
const DISCOVER_HELP_BIN = "./scripts/discover-help";

if (!process.env.OPENAI_API_KEY) {
  console.error("Error: OPENAI_API_KEY environment variable is not set.");
  process.exit(1);
}

if (!existsSync(DISCOVER_HELP_BIN)) {
  console.error(`Error: '${DISCOVER_HELP_BIN}' not found. Please compile it first:`);
  console.error("  bun build ./scripts/discover-help.ts --compile --outfile ./scripts/discover-help");
  process.exit(1);
}

if (!existsSync(OUTPUT_DIR)) {
  mkdirSync(OUTPUT_DIR);
}

console.log(`\nStarting discovery for ${AGENTS.length} agents...`);
console.log(`Results will be stored in: ${OUTPUT_DIR}/\n`);

for (const agent of AGENTS) {
  console.log(`${"=".repeat(60)}`);
  console.log(`AGENT: ${agent}`);
  console.log(`${"=".repeat(60)}`);

  try {
    const proc = await $`command -v ${agent}`.quiet();
    const hasCommand = proc.exitCode === 0;
    if (!hasCommand) {
      console.warn(`[SKIP] '${agent}' not found in PATH.`);
      continue;
    }

    console.log(`[RUN] Discovering help for '${agent}'...`);
    // Run the discover-help binary
    await $`${DISCOVER_HELP_BIN} ${agent}`;

    // The binary creates a file named help_discovery_${agent}.txt in the current directory
    const expectedFile = `help_discovery_${agent}.txt`;
    if (existsSync(expectedFile)) {
      const targetPath = join(OUTPUT_DIR, expectedFile);
      await $`mv ${expectedFile} ${targetPath}`;
      console.log(`[OK] Results saved to ${targetPath}`);
    } else {
        console.warn(`[WARN] No output file found for ${agent}.`);
    }
  } catch (error) {
    console.error(`[ERROR] Failed to process '${agent}':`, error);
  }
  console.log("");
}

console.log(`${"=".repeat(60)}`);
console.log("Discovery run complete!");
console.log(`${"=".repeat(60)}\n`);
