import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import process from "node:process";

if (process.argv.length !== 2) {
  console.error("This command accepts no paths or arguments; it always uses a new isolated temporary directory.");
  process.exit(2);
}

const artifactDir = await mkdtemp(join(tmpdir(), "pos-tauri-v8-v12-qa-"));
const result = spawnSync(
  "cargo",
  [
    "test",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "db::tests::migration_qa::real_v8_to_v12_migration_qa",
    "--",
    "--ignored",
    "--exact",
    "--nocapture",
  ],
  {
    cwd: new URL("..", import.meta.url),
    env: { ...process.env, POS_MIGRATION_QA_DIR: artifactDir },
    stdio: "inherit",
  },
);

console.log(`QA artifacts preserved at: ${artifactDir}`);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
