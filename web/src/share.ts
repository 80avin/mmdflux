export type ShareFormat = "text" | "svg" | "mmds";
export type ShareLayoutEngine = "auto" | "flux-layered" | "mermaid-layered";
export type ShareEdgePreset = "auto" | "straight" | "step" | "smoothstep" | "bezier";
export type ShareGeometryLevel = "layout" | "routed";
export type SharePathDetail = "full" | "compact" | "simplified" | "endpoints";

export interface ShareRenderSettings {
  layoutEngine: ShareLayoutEngine;
  edgePreset: ShareEdgePreset;
  geometryLevel: ShareGeometryLevel;
  pathDetail: SharePathDetail;
}

export const DEFAULT_SHARE_RENDER_SETTINGS: ShareRenderSettings = {
  layoutEngine: "auto",
  edgePreset: "auto",
  geometryLevel: "layout",
  pathDetail: "full",
};

export interface ShareState {
  input: string;
  format: ShareFormat;
  renderSettings: ShareRenderSettings;
}

interface ShareWireStateV1 {
  v: 1;
  input: string;
  format: ShareFormat;
}

interface ShareWireStateV2 {
  v: 2;
  input: string;
  format: ShareFormat;
  renderSettings?: Partial<ShareRenderSettings>;
}

type ShareWireState = ShareWireStateV1 | ShareWireStateV2;

function isShareFormat(value: string): value is ShareFormat {
  return value === "text" || value === "svg" || value === "mmds";
}

function isLayoutEngine(value: string): value is ShareLayoutEngine {
  return (
    value === "auto" || value === "flux-layered" || value === "mermaid-layered"
  );
}

function isEdgePreset(value: string): value is ShareEdgePreset {
  return (
    value === "auto" ||
    value === "straight" ||
    value === "step" ||
    value === "smoothstep" ||
    value === "bezier"
  );
}

function isGeometryLevel(value: string): value is ShareGeometryLevel {
  return value === "layout" || value === "routed";
}

function isPathDetail(value: string): value is SharePathDetail {
  return (
    value === "full" ||
    value === "compact" ||
    value === "simplified" ||
    value === "endpoints"
  );
}

export function normalizeShareRenderSettings(
  value: unknown,
): ShareRenderSettings {
  const settings =
    typeof value === "object" && value !== null
      ? (value as Partial<ShareRenderSettings>)
      : {};

  const layoutEngine =
    typeof settings.layoutEngine === "string" &&
    isLayoutEngine(settings.layoutEngine)
    ? settings.layoutEngine
    : DEFAULT_SHARE_RENDER_SETTINGS.layoutEngine;
  const edgePreset =
    typeof settings.edgePreset === "string" && isEdgePreset(settings.edgePreset)
    ? settings.edgePreset
    : DEFAULT_SHARE_RENDER_SETTINGS.edgePreset;
  const geometryLevel =
    typeof settings.geometryLevel === "string" &&
    isGeometryLevel(settings.geometryLevel)
    ? settings.geometryLevel
    : DEFAULT_SHARE_RENDER_SETTINGS.geometryLevel;
  const pathDetail =
    typeof settings.pathDetail === "string" && isPathDetail(settings.pathDetail)
    ? settings.pathDetail
    : DEFAULT_SHARE_RENDER_SETTINGS.pathDetail;

  return {
    layoutEngine,
    edgePreset,
    geometryLevel,
    pathDetail,
  };
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
  const wireState: ShareWireStateV2 = {
    v: 2,
    input: state.input,
    format: state.format,
    renderSettings: state.renderSettings,
  };
  return base64UrlEncode(JSON.stringify(wireState));
}

export function decodeShareState(value: string): ShareState | null {
  const normalized = value.startsWith("#") ? value.slice(1) : value;
  if (!normalized) {
    return null;
  }

  try {
    const decoded = JSON.parse(
      base64UrlDecode(normalized),
    ) as Partial<ShareWireState>;
    if (decoded.v !== 1 && decoded.v !== 2) {
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
      format: decoded.format,
      renderSettings:
        decoded.v === 2
          ? normalizeShareRenderSettings(decoded.renderSettings)
          : DEFAULT_SHARE_RENDER_SETTINGS,
    };
  } catch {
    return null;
  }
}
