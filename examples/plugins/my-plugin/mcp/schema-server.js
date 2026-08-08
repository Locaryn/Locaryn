#!/usr/bin/env node
// Example MCP server stub (stdio). A real implementation speaks the
// MCP 2026-07-28 protocol over stdio. This stub just keeps the process
// alive so `locaryn mcp start schema-introspect` succeeds in the demo.
"use strict";
process.stdin.resume();
process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: null, result: { capabilities: {} } }) + "\n");
