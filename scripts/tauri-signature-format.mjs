function decodeCanonicalBase64(value, description) {
  if (
    typeof value !== "string"
    || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)
  ) {
    throw new Error(`${description} must be canonical Base64`);
  }
  const decoded = Buffer.from(value, "base64");
  if (!decoded.length || decoded.toString("base64") !== value) {
    throw new Error(`${description} must be canonical Base64`);
  }
  return decoded;
}

function decodeUtf8(buffer, description) {
  const value = new TextDecoder("utf-8", { fatal: true }).decode(buffer);
  if (Buffer.from(value, "utf8").compare(buffer) !== 0) {
    throw new Error(`${description} is not valid UTF-8`);
  }
  return value;
}

export function decodeTauriPublicKey(value) {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY repository variable is required");
  }
  const decoded = decodeCanonicalBase64(value, "TAURI_UPDATER_PUBLIC_KEY");
  const lines = decodeUtf8(decoded, "TAURI_UPDATER_PUBLIC_KEY").split("\n");
  if (
    lines.length !== 3
    || lines[2] !== ""
    || !/^untrusted comment: minisign public key: [0-9A-F]{16}$/.test(lines[0])
  ) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY does not encode a canonical minisign public key file");
  }
  const key = decodeCanonicalBase64(lines[1], "minisign public key line");
  if (key.length !== 42 || key[0] !== 0x45 || key[1] !== 0x64) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY does not encode a valid minisign public key");
  }
  return decoded;
}

export function minisignPublicKeyValue(value) {
  decodeTauriPublicKey(value);
  return value;
}

export function decodeTauriSignature(value) {
  const decoded = decodeCanonicalBase64(value, "Tauri updater signature");
  const contents = decodeUtf8(decoded, "Tauri updater signature");
  const lines = contents.split("\n");
  if (
    lines.length !== 5
    || lines[0] !== "untrusted comment: signature from tauri secret key"
    || !lines[2].startsWith("trusted comment: ")
    || lines[2] === "trusted comment: "
    || lines[4] !== ""
  ) {
    throw new Error("Tauri updater signature does not encode a canonical minisign signature file");
  }
  const signature = decodeCanonicalBase64(lines[1], "minisign signature line");
  const globalSignature = decodeCanonicalBase64(lines[3], "minisign global signature line");
  if (signature.length !== 74 || signature[0] !== 0x45 || signature[1] !== 0x44) {
    throw new Error("Tauri updater signature contains an invalid minisign signature");
  }
  if (globalSignature.length !== 64) {
    throw new Error("Tauri updater signature contains an invalid minisign global signature");
  }
  return decoded;
}
