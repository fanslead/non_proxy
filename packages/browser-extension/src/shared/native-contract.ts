export const nativeProtocolVersion = 1 as const;
export const nativeHostName = "com.nonproxy.browser";

export type ObservationKind = "mainFrame" | "subresource" | "redirect";
export type ResourceType =
  | "mainFrame"
  | "subFrame"
  | "script"
  | "styleSheet"
  | "image"
  | "font"
  | "media"
  | "xmlHttpRequest"
  | "fetch"
  | "webSocket"
  | "other";

export interface StartLearningPayload {
  readonly normalizedSite: string;
  readonly browserContextID: string;
  readonly durationMilliseconds: number;
}

export interface ObserveLearningPayload {
  readonly sessionID: string;
  readonly observationID: string;
  readonly browserContextID: string;
  readonly kind: ObservationKind;
  readonly normalizedDomain: string;
  readonly initiatorDomain?: string;
  readonly resourceType: ResourceType;
}

export interface SessionPayload {
  readonly sessionID: string;
}

export interface ConfirmLearningPayload {
  readonly sessionID: string;
  readonly confirmationID: string;
  readonly selectedDomains: readonly string[];
}

export type NativeRequest =
  | NativeEnvelope<"hello", Record<string, never>>
  | NativeEnvelope<"startLearning", StartLearningPayload>
  | NativeEnvelope<"observeLearning", ObserveLearningPayload>
  | NativeEnvelope<"listCandidates", SessionPayload>
  | NativeEnvelope<"stopLearning", SessionPayload>
  | NativeEnvelope<"confirmLearning", ConfirmLearningPayload>;

export interface NativeEnvelope<Type extends string, Payload> {
  readonly protocolVersion: typeof nativeProtocolVersion;
  readonly requestID: string;
  readonly type: Type;
  readonly payload: Payload;
}

export interface NativeErrorPayload {
  readonly code: string;
  readonly message: string;
}

export interface NativeResponse<Payload = unknown> {
  readonly protocolVersion: number;
  readonly requestID: string;
  readonly ok: boolean;
  readonly payload?: Payload;
  readonly error?: NativeErrorPayload;
}

export interface StartLearningResult {
  readonly sessionID: string;
  readonly expiresAtUnixMilliseconds: number;
}

export interface CandidateResult {
  readonly normalizedDomain: string;
  readonly registrableDomain?: string;
  readonly kind:
    | "requiredFirstParty"
    | "likelyApi"
    | "likelyAuth"
    | "likelyCdn"
    | "thirdParty"
    | "unknown";
  readonly confidenceMillis: number;
  readonly requiresConfirmation: boolean;
  readonly evidenceCount: number;
  readonly firstSeenAtUnixMilliseconds: number;
  readonly lastSeenAtUnixMilliseconds: number;
  readonly mainFrameCount: number;
  readonly subresourceCount: number;
  readonly redirectCount: number;
}

export interface ObservationResult {
  readonly candidate: CandidateResult;
  readonly duplicate: boolean;
}

export interface SessionResult {
  readonly sessionID: string;
  readonly state: "active" | "stopped" | "expired";
  readonly normalizedSite?: string;
  readonly browserContextID?: string;
  readonly startedAtUnixMilliseconds: number;
  readonly expiresAtUnixMilliseconds: number;
  readonly stoppedAtUnixMilliseconds?: number;
}

export interface CandidateListResult {
  readonly session: SessionResult;
  readonly candidates: readonly CandidateResult[];
}

export interface StopLearningResult {
  readonly session: SessionResult;
  readonly candidateCount: number;
}

export interface ConfirmedPolicyResult {
  readonly normalizedDomain: string;
  readonly policyID: string;
}

export interface ConfirmLearningResult {
  readonly policies: readonly ConfirmedPolicyResult[];
  readonly snapshotVersion: number;
  readonly snapshotState:
    | "draft"
    | "pendingAck"
    | "active"
    | "rejected"
    | "rolledBack"
    | "superseded";
  readonly replayed: boolean;
}

export function createNativeRequest(
  type: NativeRequest["type"],
  payload: NativeRequest["payload"],
): NativeRequest {
  const requestID = `req-${crypto.randomUUID()}`;
  return {
    protocolVersion: nativeProtocolVersion,
    requestID,
    type,
    payload,
  } as NativeRequest;
}

export function parseNativeResponse<Payload>(
  value: unknown,
  expectedRequestID: string,
): Payload {
  if (!isRecord(value)) {
    throw new Error("NP_EXTENSION_NATIVE_RESPONSE_INVALID");
  }
  if (
    value.protocolVersion !== nativeProtocolVersion ||
    value.requestID !== expectedRequestID ||
    typeof value.ok !== "boolean"
  ) {
    throw new Error("NP_EXTENSION_NATIVE_RESPONSE_INVALID");
  }
  if (!value.ok) {
    const error = isRecord(value.error) ? value.error : undefined;
    const code =
      typeof error?.code === "string"
        ? error.code
        : "NP_EXTENSION_NATIVE_REJECTED";
    throw new Error(code);
  }
  if (!("payload" in value)) {
    throw new Error("NP_EXTENSION_NATIVE_RESPONSE_INVALID");
  }
  return value.payload as Payload;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
