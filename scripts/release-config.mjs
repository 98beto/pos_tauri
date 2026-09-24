import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { minisignPublicKeyValue } from "./tauri-signature-format.mjs";

export const UPDATER_ENDPOINT =
  "https://github.com/98beto/pos_tauri/releases/latest/download/latest.json";

export function createReleaseConfig(publicKey, createUpdaterArtifacts = true) {
  return {
    build: { beforeBuildCommand: "" },
    bundle: { createUpdaterArtifacts },
    plugins: {
      updater: {
        endpoints: [UPDATER_ENDPOINT],
        pubkey: minisignPublicKeyValue(publicKey),
      },
    },
  };
}

export async function writeReleaseConfig(
  outputPath,
  publicKey,
  createUpdaterArtifacts = true,
) {
  if (!outputPath) {
    throw new Error("--output is required");
  }

  const config = createReleaseConfig(publicKey, createUpdaterArtifacts);
  await mkdir(path.dirname(path.resolve(outputPath)), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(config, null, 2)}\n`, {
    mode: 0o600,
  });
  return config;
}

function optionValue(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

const isMain = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  try {
    await writeReleaseConfig(
      optionValue("--output"),
      process.env.TAURI_UPDATER_PUBLIC_KEY,
      !process.argv.includes("--disable-updater-artifacts"),
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
