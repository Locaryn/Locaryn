#!/usr/bin/env node
process.stdin.resume();
process.stdout.write(
  `${JSON.stringify({ jsonrpc: "2.0", id: null, result: { capabilities: {} } })}\n`,
);
