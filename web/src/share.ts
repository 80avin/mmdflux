export type ShareFormat = "text" | "svg" | "mmds";

export interface ShareState {
  input: string;
  format: ShareFormat;
}

interface ShareWireState {
  v: 1;
  input: string;
  format: ShareFormat;
}

function isShareFormat(value: string): value is ShareFormat {
  return value === "text" || value === "svg" || value === "mmds";
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

function base64ToBytes(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function base64UrlEncode(value: string): string {
  const encoded = bytesToBase64(new TextEncoder().encode(value));
  return encoded.replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

function base64UrlDecode(value: string): string {
  const normalized = value
    .replaceAll("-", "+")
    .replaceAll("_", "/")
    .padEnd(Math.ceil(value.length / 4) * 4, "=");
  const bytes = base64ToBytes(normalized);
  return new TextDecoder().decode(bytes);
}

export function encodeShareState(state: ShareState): string {
  const wireState: ShareWireState = {
    v: 1,
    input: state.input,
    format: state.format
  };
  return base64UrlEncode(JSON.stringify(wireState));
}

export function decodeShareState(value: string): ShareState | null {
  const normalized = value.startsWith("#") ? value.slice(1) : value;
  if (!normalized) {
    return null;
  }

  try {
    const decoded = JSON.parse(base64UrlDecode(normalized)) as Partial<ShareWireState>;
    if (decoded.v !== 1) {
      return null;
    }
    if (typeof decoded.input !== "string") {
      return null;
    }
    if (typeof decoded.format !== "string" || !isShareFormat(decoded.format)) {
      return null;
    }

    return {
      input: decoded.input,
      format: decoded.format
    };
  } catch {
    return null;
  }
}
