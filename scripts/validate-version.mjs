import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const TAG_PATTERN = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, "utf8"));
}

function cargoPackageVersion(contents) {
  const packageSection = contents.match(
    /^\[package\]\s*$([\s\S]*?)(?=^\[|$(?![\s\S]))/m,
  )?.[1];
  const version = packageSection?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];

  if (!version) {
    throw new Error("Could not read the package version from src-tauri/Cargo.toml");
  }

  return version;
}

export async function validateVersion(tag, rootDirectory = process.cwd()) {
  const tagMatch = TAG_PATTERN.exec(tag ?? "");
  if (!tagMatch) {
    throw new Error(`Invalid release tag "${tag ?? ""}"; expected vX.Y.Z`);
  }

  const expectedVersion = tag.slice(1);
  const packageJsonPath = path.join(rootDirectory, "package.json");
  const cargoTomlPath = path.join(rootDirectory, "src-tauri", "Cargo.toml");
  const tauriConfigPath = path.join(rootDirectory, "src-tauri", "tauri.conf.json");

  const [packageJson, cargoToml, tauriConfig] = await Promise.all([
    readJson(packageJsonPath),
    readFile(cargoTomlPath, "utf8"),
    readJson(tauriConfigPath),
  ]);
  const versions = {
    "package.json": packageJson.version,
    "src-tauri/Cargo.toml": cargoPackageVersion(cargoToml),
    "src-tauri/tauri.conf.json": tauriConfig.version,
  };
  const mismatches = Object.entries(versions).filter(
    ([, version]) => version !== expectedVersion,
  );

  if (mismatches.length > 0) {
    const details = mismatches
      .map(([file, version]) => `${file} has ${JSON.stringify(version)}`)
      .join(", ");
    throw new Error(`Tag ${tag} requires version ${expectedVersion}; ${details}`);
  }

  return expectedVersion;
}

const isMain = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  const tag = process.argv[2] ?? process.env.GITHUB_REF_NAME;

  try {
    const version = await validateVersion(tag);
    console.log(`Release version ${version} is consistent.`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
