#!/usr/bin/env node
// SessionStart/UserPromptSubmit hook: injects the time-logging contract as
// additionalContext. Installed by install-hooks.mjs.
//
// Usage: node tt-contract-hook.mjs <session|session-card|prompt>
//
//   session      - the whole contract, once, at session start.
//   session-card - opt-in alternative to `session`: the operating card plus
//                  the full contract's path, once, at session start. Installed
//                  by `install-hooks.mjs --session-card`.
//   prompt       - only the operating card (everything above the `card:end`
//                  marker in SKILL.md), on every prompt.
//
// Every mode appends the harness's own session id, read from the hook payload on
// stdin. Re-injected every prompt, so it cannot go stale in a compacted context.
//
// Why the split: UserPromptSubmit fires on every turn, and re-injecting 15KB
// of unchanged reference text each time trains a "seen it, skip it" response —
// the reader learns the block is boilerplate, which is exactly when the one
// actionable line in it stops landing. The card is small enough to actually
// re-read, and it's the part with commands in it.
//
// The card is not a second file. It's the head of SKILL.md, cut at the marker,
// so there is nothing to keep in sync: editing the document's opening is
// editing the card.
//
// SKILL.md is read from this script's own directory — install-hooks.mjs copies
// both into ~/.claude/hooks/tt-time-logging/ so they travel together.
//
// A hook must never fail the harness event it's attached to: any error here
// emits an empty result and exits 0.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const CARD_MARKER = "<!-- card:end";

const EVENTS = {
  session: "SessionStart",
  "session-card": "SessionStart",
  prompt: "UserPromptSubmit",
};

// The harness's own session id, from the hook payload — the model is told it
// nowhere else, and `tt agent end|resolve|dismiss` need it to name a session.
// An unreadable or unparseable payload just leaves the line out.
function sessionLine() {
  let id;
  try {
    id = JSON.parse(readFileSync(0, "utf8"))?.session_id;
  } catch {
    return "";
  }
  return id
    ? `\n\nThis session's id is \`${id}\`. Pass it as \`--session <id>\` on ` +
        "`tt agent end`, and on `tt agent resolve`/`dismiss` for this session's own rows."
    : "";
}

function emit(hookEventName, additionalContext) {
  process.stdout.write(
    additionalContext
      ? JSON.stringify({ hookSpecificOutput: { hookEventName, additionalContext } })
      : "{}",
  );
  process.exit(0);
}

const mode = process.argv[2];
const hookEventName = EVENTS[mode];
if (!hookEventName) emit(null, null);

let contract;
let contractPath;
try {
  const scriptDir = dirname(fileURLToPath(import.meta.url));
  contractPath = join(scriptDir, "SKILL.md");
  contract = readFileSync(contractPath, "utf8");
} catch {
  emit(hookEventName, null);
}

const session = sessionLine();

if (mode === "session") emit(hookEventName, contract + session);

// Card: the head of the document, minus the skill frontmatter. A missing
// marker means someone edited SKILL.md without keeping one — fall back to the
// whole file rather than injecting nothing.
const cut = contract.indexOf(CARD_MARKER);
const head = cut === -1 ? contract : contract.slice(0, cut);
const card = head.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, "").trim();

// A path, not a skill name: a Read works whether or not the skill is
// registered with the harness. No marker means the card is the whole file
// already, so there is nothing left to point at.
if (mode === "session-card" && cut !== -1) {
  emit(
    hookEventName,
    `${card}\n\nFull contract (rules, parallel subagents, summaries, tags): read ` +
      `${contractPath.replace(/\\/g, "/")} when a case isn't covered above.` +
      session,
  );
}
emit(hookEventName, card + session);
