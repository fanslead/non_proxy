import type { LearningState } from "./learning-state.js";

export const learningOriginPattern = "*://*/*";

export type ExtensionRequest =
  | { readonly type: "getState"; readonly tabID: number }
  | { readonly type: "start"; readonly tabID: number }
  | { readonly type: "stop"; readonly tabID: number }
  | {
      readonly type: "confirm";
      readonly tabID: number;
      readonly selectedDomains: readonly string[];
    }
  | { readonly type: "discard"; readonly tabID: number };

export interface ExtensionResponse {
  readonly ok: boolean;
  readonly state?: LearningState;
  readonly error?: string;
}

export function parseExtensionRequest(
  value: unknown,
): ExtensionRequest | null {
  if (
    typeof value !== "object" ||
    value === null ||
    !("type" in value) ||
    !("tabID" in value) ||
    typeof value.type !== "string" ||
    typeof value.tabID !== "number" ||
    !Number.isSafeInteger(value.tabID) ||
    value.tabID < 0
  ) {
    return null;
  }
  switch (value.type) {
    case "getState":
    case "start":
    case "stop":
    case "discard":
      return { type: value.type, tabID: value.tabID };
    case "confirm": {
      if (
        !("selectedDomains" in value) ||
        !validSelectedDomains(value.selectedDomains)
      ) {
        return null;
      }
      return {
        type: "confirm",
        tabID: value.tabID,
        selectedDomains: [...value.selectedDomains],
      };
    }
    default:
      return null;
  }
}

function validSelectedDomains(
  value: unknown,
): value is readonly string[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > 256) {
    return false;
  }
  const domains = new Set<string>();
  for (const domain of value) {
    if (
      typeof domain !== "string" ||
      domain.length === 0 ||
      domain.length > 253 ||
      domains.has(domain)
    ) {
      return false;
    }
    domains.add(domain);
  }
  return true;
}
