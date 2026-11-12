import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

if (process.argv.length !== 2) {
  console.error("This command accepts no paths or arguments.");
  process.exit(2);
}
// Retain the harness's exact safe temporary-directory namespace.
const artifactDir = await mkdtemp(join(tmpdir(), "pos-tauri-v8-v13-qa-"));
const result = spawnSync("cargo", ["test", "--locked", "--manifest-path", "src-tauri/Cargo.toml",
  "db::tests::migration_qa::real_sources_to_v16_migration_qa", "--", "--ignored", "--exact", "--nocapture"], {
  cwd: new URL("..", import.meta.url), env: { ...process.env, POS_MIGRATION_QA_DIR: artifactDir }, stdio: "inherit",
});
console.log(`QA artifacts preserved at: ${artifactDir}`);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
