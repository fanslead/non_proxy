import type { LearningState } from "../background/learning-controller.js";

export const learningOriginPattern = "*://*/*";

export type ExtensionRequest =
  | { readonly type: "getState"; readonly tabID: number }
  | { readonly type: "start"; readonly tabID: number }
  | { readonly type: "stop"; readonly tabID: number };

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
  if (
    value.type !== "getState" &&
    value.type !== "start" &&
    value.type !== "stop"
  ) {
    return null;
  }
  return { type: value.type, tabID: value.tabID };
}
