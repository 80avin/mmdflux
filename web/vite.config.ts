import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";

const DEFAULT_BASE_PATH = "/";

function normalizeBasePath(basePath: string): string {
  const trimmed = basePath.trim();
  if (trimmed.length === 0) {
    return DEFAULT_BASE_PATH;
  }

  const withLeadingSlash = trimmed.startsWith("/") ? trimmed : `/${trimmed}`;
  return withLeadingSlash.endsWith("/")
    ? withLeadingSlash
    : `${withLeadingSlash}/`;
}

export function resolveViteBasePath(
  env: NodeJS.ProcessEnv = process.env,
): string {
  const explicitBasePath = env.VITE_BASE_PATH;
  if (typeof explicitBasePath === "string" && explicitBasePath.trim() !== "") {
    return normalizeBasePath(explicitBasePath);
  }

  const isGithubActions = env.GITHUB_ACTIONS === "true";
  const repository = env.GITHUB_REPOSITORY;
  if (!isGithubActions || typeof repository !== "string") {
    return DEFAULT_BASE_PATH;
  }

  const [, repositoryName] = repository.split("/", 2);
  if (!repositoryName) {
    return DEFAULT_BASE_PATH;
  }

  return normalizeBasePath(repositoryName);
}

export default defineConfig({
  base: resolveViteBasePath(),
  plugins: [wasm()],
  worker: {
    format: "es",
    plugins: () => [wasm()],
  },
  test: {
    environment: "jsdom",
  },
});
