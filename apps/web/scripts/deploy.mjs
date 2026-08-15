// Deploy the built web client where the daemon serves it.
//
// The daemon serves {data_dir}/web, with data_dir resolved the same way
// locaryn-config does: LOCARYN_DATA_DIR wins when set (for tests), then the
// storage root, then ~/.locaryn/data, falling back to the legacy ~/.lochor.
import { cpSync, existsSync, mkdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

function globalDir() {
  const home = homedir();
  const current = join(home, ".locaryn");
  if (existsSync(current)) return current;
  const legacy = join(home, ".lochor");
  if (existsSync(legacy) && statSync(legacy).isDirectory()) return legacy;
  return current;
}

const dataDir = process.env.LOCARYN_DATA_DIR ?? join(globalDir(), "data");
const target = join(dataDir, "web");

mkdirSync(target, { recursive: true });
cpSync(fileURLToPath(new URL("../dist", import.meta.url)), target, { recursive: true });
console.log(`Client web déployé vers ${target}`);
