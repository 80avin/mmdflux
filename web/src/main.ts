import "../styles/main.css";

import { createEditorController } from "./editor";
import {
  DEFAULT_EXAMPLE_ID,
  findExampleById,
  PLAYGROUND_EXAMPLES,
} from "./examples";
import {
  createLiveUpdateController,
  type LiveUpdateDebounceSetting,
} from "./live-update";
import { createPreviewController } from "./preview";
import {
  DEFAULT_SHARE_RENDER_SETTINGS,
  decodeShareState,
  encodeShareState,
  normalizeShareRenderSettings,
  type ShareEdgePreset,
  type ShareGeometryLevel,
  type ShareLayoutEngine,
  type SharePathDetail,
  type ShareRenderSettings,
} from "./share";
import { createThemeController, type ThemePreference } from "./theme";
import type {
  WorkerOutputFormat,
  WorkerRequestMessage,
  WorkerResponseMessage,
} from "./worker-protocol";

export interface RenderRequest {
  seq: number;
  input: string;
  format: WorkerOutputFormat;
  configJson?: string;
}

export interface RenderResponse {
  seq: number;
  format: WorkerOutputFormat;
  output: string;
}

interface PendingRequest {
  resolve: (response: RenderResponse) => void;
  reject: (error: Error) => void;
}

export interface RenderWorkerClient {
  render: (request: RenderRequest) => Promise<RenderResponse>;
  terminate: () => void;
}

type PlaygroundFormat = "text" | "svg" | "mmds";
type StateStorage = Pick<Storage, "getItem" | "setItem">;

interface PersistedPlaygroundStateV1 {
  v: 1;
  input: string;
  format: PlaygroundFormat;
}

interface PersistedPlaygroundStateV2 {
  v: 2;
  input: string;
  format: PlaygroundFormat;
  renderSettings: ShareRenderSettings;
}

interface EffectivePlaygroundState {
  input: string;
  format: PlaygroundFormat;
  renderSettings: ShareRenderSettings;
}

type PersistedPlaygroundState =
  | PersistedPlaygroundStateV1
  | PersistedPlaygroundStateV2;

const PLAYGROUND_STATE_STORAGE_KEY = "mmdflux-playground-state";

export interface RenderAppOptions {
  renderClientFactory?: () => RenderWorkerClient | null;
  debounceMs?: LiveUpdateDebounceSetting;
  stateStorage?: StateStorage;
}

type SearchLocation = URL | Pick<Location, "search">;

function defaultAdaptiveDebounce(requestInput: string): number {
  const length = requestInput.length;
  if (length <= 2_500) {
    return 0;
  }
  if (length <= 8_000) {
    return 40;
  }
  if (length <= 16_000) {
    return 80;
  }
  return 120;
}

function resolveStateStorage(
  explicitStorage?: StateStorage,
): StateStorage | undefined {
  if (isStorageLike(explicitStorage)) {
    return explicitStorage;
  }

  try {
    return isStorageLike(window.localStorage) ? window.localStorage : undefined;
  } catch {
    return undefined;
  }
}

function isStorageLike(value: unknown): value is StateStorage {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as Pick<Storage, "getItem">).getItem === "function" &&
    typeof (value as Pick<Storage, "setItem">).setItem === "function"
  );
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

function parsePersistedPlaygroundState(
  rawValue: string | null,
): EffectivePlaygroundState | null {
  if (!rawValue) {
    return null;
  }

  try {
    const parsed = JSON.parse(rawValue) as Partial<PersistedPlaygroundState>;
    if (parsed.v !== 1 && parsed.v !== 2) {
      return null;
    }
    if (typeof parsed.input !== "string") {
      return null;
    }
    if (
      typeof parsed.format !== "string" ||
      !isPlaygroundFormat(parsed.format)
    ) {
      return null;
    }

    const renderSettings =
      parsed.v === 2
        ? normalizeShareRenderSettings(parsed.renderSettings)
        : DEFAULT_SHARE_RENDER_SETTINGS;

    return {
      input: parsed.input,
      format: parsed.format,
      renderSettings,
    };
  } catch {
    return null;
  }
}

function readPersistedPlaygroundState(
  storage: StateStorage | undefined,
): EffectivePlaygroundState | null {
  if (!storage) {
    return null;
  }

  return parsePersistedPlaygroundState(
    storage.getItem(PLAYGROUND_STATE_STORAGE_KEY),
  );
}

function persistPlaygroundState(
  storage: StateStorage | undefined,
  state: EffectivePlaygroundState,
): void {
  if (!storage) {
    return;
  }

  const persisted: PersistedPlaygroundStateV2 = {
    v: 2,
    input: state.input,
    format: state.format,
    renderSettings: state.renderSettings,
  };
  storage.setItem(PLAYGROUND_STATE_STORAGE_KEY, JSON.stringify(persisted));
}

function createDefaultWorker(): Worker {
  return new Worker(new URL("./worker.ts", import.meta.url), {
    type: "module",
  });
}

function isPlaygroundFormat(value: string): value is PlaygroundFormat {
  return value === "text" || value === "svg" || value === "mmds";
}

function toMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function nextThemePreference(current: ThemePreference): ThemePreference {
  if (current === "system") {
    return "light";
  }
  if (current === "light") {
    return "dark";
  }
  return "system";
}

function formatThemeLabel(preference: ThemePreference): string {
  if (preference === "system") {
    return "Theme: System";
  }
  if (preference === "light") {
    return "Theme: Light";
  }
  return "Theme: Dark";
}

async function copyToClipboard(text: string): Promise<boolean> {
  if (!navigator.clipboard?.writeText) {
    return false;
  }

  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export function createRenderWorkerClient(
  worker: Worker = createDefaultWorker(),
): RenderWorkerClient {
  const pending = new Map<number, PendingRequest>();

  worker.onmessage = (event: MessageEvent<WorkerResponseMessage>) => {
    const response = event.data;
    const pendingRequest = pending.get(response.seq);
    if (!pendingRequest) {
      return;
    }

    pending.delete(response.seq);

    if (response.type === "result") {
      pendingRequest.resolve({
        seq: response.seq,
        format: response.format,
        output: response.output,
      });
      return;
    }

    pendingRequest.reject(new Error(response.error));
  };

  return {
    render: (request: RenderRequest) => {
      const currentSeq = request.seq;

      return new Promise<RenderResponse>((resolve, reject) => {
        const message: WorkerRequestMessage = {
          type: "render",
          seq: currentSeq,
          input: request.input,
          format: request.format,
          configJson: request.configJson ?? "{}",
        };

        pending.set(currentSeq, { resolve, reject });

        try {
          worker.postMessage(message);
        } catch (error) {
          pending.delete(currentSeq);
          reject(
            new Error(`failed to post render request: ${toMessage(error)}`),
          );
        }
      });
    },
    terminate: () => {
      worker.terminate();
      for (const request of pending.values()) {
        request.reject(new Error("render worker terminated"));
      }
      pending.clear();
    },
  };
}

export function renderApp(
  root: HTMLElement,
  options: RenderAppOptions = {},
): void {
  const stateStorage = resolveStateStorage(options.stateStorage);
  const restoredShareState = decodeShareState(window.location.hash);
  const restoredLocalState = readPersistedPlaygroundState(stateStorage);
  const defaultExample =
    findExampleById(DEFAULT_EXAMPLE_ID) ?? PLAYGROUND_EXAMPLES[0];
  const initialInput =
    restoredShareState?.input ??
    restoredLocalState?.input ??
    defaultExample?.input ??
    "";
  const initialFormat =
    restoredShareState?.format ?? restoredLocalState?.format ?? "text";
  const initialRenderSettings =
    restoredShareState?.renderSettings ??
    restoredLocalState?.renderSettings ??
    DEFAULT_SHARE_RENDER_SETTINGS;

  root.innerHTML = `
    <main class="playground">
      <header class="toolbar">
        <h1>mmdflux playground <a href="https://github.com/kevinswiber/mmdflux" target="_blank" rel="noopener noreferrer" class="repo-link"><svg class="repo-link-icon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>kevinswiber/mmdflux</a></h1>
        <div class="toolbar-actions">
          <label class="example-picker">
            <span>Example</span>
            <select data-example-select></select>
          </label>
          <div class="format-tabs" role="tablist" aria-label="Output format">
            <button type="button" role="tab" data-format="text" aria-selected="true" class="is-active">Text</button>
            <button type="button" role="tab" data-format="svg" aria-selected="false">SVG</button>
            <button type="button" role="tab" data-format="mmds" aria-selected="false">MMDS</button>
          </div>
          <label class="render-control">
            <span>Engine</span>
            <select data-layout-engine>
              <option value="auto">Auto</option>
              <option value="flux-layered">flux-layered</option>
              <option value="mermaid-layered">mermaid-layered</option>
            </select>
          </label>
          <label class="render-control">
            <span>Edge Preset</span>
            <select data-edge-preset>
              <option value="auto">Auto</option>
              <option value="straight">straight</option>
              <option value="step">step</option>
              <option value="smoothstep">smoothstep</option>
              <option value="bezier">bezier</option>
            </select>
          </label>
          <label class="render-control">
            <span>Geometry</span>
            <select data-geometry-level>
              <option value="layout">layout</option>
              <option value="routed">routed</option>
            </select>
          </label>
          <label class="render-control">
            <span>Path Detail</span>
            <select data-path-detail>
              <option value="full">full</option>
              <option value="compact">compact</option>
              <option value="simplified">simplified</option>
              <option value="endpoints">endpoints</option>
            </select>
          </label>
          <button type="button" class="toolbar-button" data-theme-toggle>Theme: System</button>
          <button type="button" class="toolbar-button" data-share>Copy Share URL</button>
        </div>
      </header>
      <section class="workspace">
        <div class="panel">
          <h2>Input</h2>
          <div data-editor-root></div>
        </div>
        <div class="panel">
          <h2>Preview</h2>
          <p class="share-status" data-share-status hidden></p>
          <p class="preview-error" data-preview-error hidden></p>
          <div class="preview-output" data-preview-output></div>
        </div>
      </section>
    </main>
  `;

  const editorRoot = root.querySelector<HTMLElement>("[data-editor-root]");
  const previewOutput = root.querySelector<HTMLElement>(
    "[data-preview-output]",
  );
  const previewError = root.querySelector<HTMLElement>("[data-preview-error]");
  const shareStatus = root.querySelector<HTMLElement>("[data-share-status]");
  const shareButton = root.querySelector<HTMLButtonElement>("[data-share]");
  const themeToggleButton = root.querySelector<HTMLButtonElement>(
    "[data-theme-toggle]",
  );
  const exampleSelect = root.querySelector<HTMLSelectElement>(
    "[data-example-select]",
  );
  const formatButtons = root.querySelectorAll<HTMLButtonElement>(
    ".format-tabs button[data-format]",
  );
  const layoutEngineSelect = root.querySelector<HTMLSelectElement>(
    "[data-layout-engine]",
  );
  const edgePresetSelect =
    root.querySelector<HTMLSelectElement>("[data-edge-preset]");
  const geometryLevelSelect = root.querySelector<HTMLSelectElement>(
    "[data-geometry-level]",
  );
  const pathDetailSelect =
    root.querySelector<HTMLSelectElement>("[data-path-detail]");

  if (
    !editorRoot ||
    !previewOutput ||
    !previewError ||
    !shareStatus ||
    !shareButton ||
    !themeToggleButton ||
    !exampleSelect ||
    !layoutEngineSelect ||
    !edgePresetSelect ||
    !geometryLevelSelect ||
    !pathDetailSelect
  ) {
    return;
  }

  const preview = createPreviewController({
    output: previewOutput,
    error: previewError,
  });
  const editor = createEditorController({
    root: editorRoot,
    initialValue: initialInput,
  });

  for (const example of PLAYGROUND_EXAMPLES) {
    const option = document.createElement("option");
    option.value = example.id;
    option.textContent = `${example.name} · ${example.description}`;
    exampleSelect.append(option);
  }

  const matchedExample = PLAYGROUND_EXAMPLES.find(
    (example) => example.input === initialInput,
  );
  if (matchedExample) {
    exampleSelect.value = matchedExample.id;
  } else {
    const customOption = document.createElement("option");
    customOption.value = "__custom__";
    customOption.textContent = "Custom from URL";
    exampleSelect.prepend(customOption);
    exampleSelect.value = "__custom__";
  }

  const matchMedia =
    typeof window.matchMedia === "function"
      ? window.matchMedia.bind(window)
      : undefined;
  const themeStorage = (() => {
    try {
      return isStorageLike(window.localStorage)
        ? window.localStorage
        : undefined;
    } catch {
      return undefined;
    }
  })();
  const themeController = createThemeController({
    root: document.documentElement,
    storage: themeStorage,
    matchMedia,
  });
  themeController.apply();
  themeToggleButton.textContent = formatThemeLabel(
    themeController.getPreference(),
  );

  let selectedFormat: PlaygroundFormat = initialFormat;
  let renderSettings: ShareRenderSettings = normalizeShareRenderSettings(
    initialRenderSettings,
  );
  const workerClient = options.renderClientFactory
    ? options.renderClientFactory()
    : typeof Worker === "undefined"
      ? null
      : createRenderWorkerClient();

  const applyRenderSettingsToControls = (): void => {
    layoutEngineSelect.value = renderSettings.layoutEngine;
    edgePresetSelect.value = renderSettings.edgePreset;
    geometryLevelSelect.value = renderSettings.geometryLevel;
    pathDetailSelect.value = renderSettings.pathDetail;
  };

  const setControlsEnabledForFormat = (format: PlaygroundFormat): void => {
    const isSvg = format === "svg";
    const isMmds = format === "mmds";
    edgePresetSelect.disabled = !isSvg;
    geometryLevelSelect.disabled = !isMmds;
    pathDetailSelect.disabled = !(isSvg || isMmds);
  };

  const currentConfigJson = (): string => {
    const config: Record<string, string> = {};
    if (renderSettings.layoutEngine !== "auto") {
      config.layoutEngine = renderSettings.layoutEngine;
    }

    if (selectedFormat === "svg") {
      if (renderSettings.edgePreset !== "auto") {
        config.edgePreset = renderSettings.edgePreset;
      }
      if (renderSettings.pathDetail !== "full") {
        config.pathDetail = renderSettings.pathDetail;
      }
    }

    if (selectedFormat === "mmds") {
      config.geometryLevel = renderSettings.geometryLevel;
      if (renderSettings.pathDetail !== "full") {
        config.pathDetail = renderSettings.pathDetail;
      }
    }

    return JSON.stringify(config);
  };

  const setFormat = (format: PlaygroundFormat): void => {
    selectedFormat = format;
    for (const button of formatButtons) {
      const active = button.dataset.format === format;
      button.classList.toggle("is-active", active);
      button.setAttribute("aria-selected", String(active));
    }
    setControlsEnabledForFormat(format);
  };

  const updateShareStatus = (message: string): void => {
    shareStatus.hidden = false;
    shareStatus.textContent = message;
  };

  const persistCurrentState = (): void => {
    persistPlaygroundState(stateStorage, {
      input: editor.getValue(),
      format: selectedFormat,
      renderSettings,
    });
  };

  if (!workerClient) {
    preview.showError("Web Worker support is unavailable in this environment.");
    return;
  }

  const liveUpdate = createLiveUpdateController({
    debounceMs:
      options.debounceMs ??
      ((request) => defaultAdaptiveDebounce(request.input)),
    render: (request) => workerClient.render(request),
    onResult: (response) => {
      preview.showResult({
        format: response.format,
        output: response.output,
      });
    },
    onError: (message) => {
      preview.showError(message);
    },
  });

  const scheduleRender = (): void => {
    liveUpdate.schedule({
      input: editor.getValue(),
      format: selectedFormat,
      configJson: currentConfigJson(),
    });
  };

  for (const button of formatButtons) {
    button.addEventListener("click", () => {
      const format = button.dataset.format;
      if (!format || !isPlaygroundFormat(format)) {
        return;
      }

      setFormat(format);
      persistCurrentState();
      scheduleRender();
    });
  }

  exampleSelect.addEventListener("change", () => {
    const nextExample = findExampleById(exampleSelect.value);
    if (!nextExample) {
      return;
    }

    editor.setValue(nextExample.input);
    persistCurrentState();
    scheduleRender();
  });

  layoutEngineSelect.addEventListener("change", () => {
    if (isLayoutEngine(layoutEngineSelect.value)) {
      renderSettings = {
        ...renderSettings,
        layoutEngine: layoutEngineSelect.value,
      };
      persistCurrentState();
      scheduleRender();
    }
  });

  edgePresetSelect.addEventListener("change", () => {
    if (isEdgePreset(edgePresetSelect.value)) {
      renderSettings = {
        ...renderSettings,
        edgePreset: edgePresetSelect.value,
      };
      persistCurrentState();
      scheduleRender();
    }
  });

  geometryLevelSelect.addEventListener("change", () => {
    if (isGeometryLevel(geometryLevelSelect.value)) {
      renderSettings = {
        ...renderSettings,
        geometryLevel: geometryLevelSelect.value,
      };
      persistCurrentState();
      scheduleRender();
    }
  });

  pathDetailSelect.addEventListener("change", () => {
    if (isPathDetail(pathDetailSelect.value)) {
      renderSettings = {
        ...renderSettings,
        pathDetail: pathDetailSelect.value,
      };
      persistCurrentState();
      scheduleRender();
    }
  });

  themeToggleButton.addEventListener("click", () => {
    const nextPreference = nextThemePreference(themeController.getPreference());
    themeController.setPreference(nextPreference);
    themeToggleButton.textContent = formatThemeLabel(
      themeController.getPreference(),
    );
  });

  shareButton.addEventListener("click", () => {
    const shareState = {
      input: editor.getValue(),
      format: selectedFormat,
      renderSettings,
    };
    const hash = encodeShareState(shareState);
    const shareUrl = `${window.location.origin}${window.location.pathname}#${hash}`;

    history.replaceState(null, "", `#${hash}`);

    void copyToClipboard(shareUrl).then((copied) => {
      if (copied) {
        updateShareStatus("Share URL copied to clipboard.");
        return;
      }
      updateShareStatus("Share URL updated in address bar.");
    });
  });

  editor.onChange(() => {
    persistCurrentState();
    scheduleRender();
  });

  applyRenderSettingsToControls();
  setFormat(selectedFormat);
  persistCurrentState();
  scheduleRender();
}

function searchFromLocation(locationValue: SearchLocation): string {
  if (locationValue instanceof URL) {
    return locationValue.search;
  }

  return locationValue.search;
}

export function isBenchmarkModeEnabled(
  locationValue: SearchLocation = window.location,
): boolean {
  const params = new URLSearchParams(searchFromLocation(locationValue));
  const rawValue = params.get("benchmark");
  if (rawValue === null) {
    return false;
  }

  const normalized = rawValue.trim().toLowerCase();
  return normalized === "" || normalized === "1" || normalized === "true";
}

async function mountApp(root: HTMLElement): Promise<void> {
  if (isBenchmarkModeEnabled(window.location)) {
    const { renderBenchmarkApp } = await import("./benchmark");
    await renderBenchmarkApp(root);
    return;
  }

  renderApp(root);
}

const root = document.querySelector<HTMLElement>("#app");
if (root) {
  void mountApp(root);
}
