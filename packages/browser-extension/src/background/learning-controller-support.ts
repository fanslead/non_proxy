import type { LearningState } from "../shared/learning-state.js";
import { normalizeWebURL } from "../shared/domain.js";
import type {
  CandidateListResult,
  CandidateResult,
  ResourceType,
} from "../shared/native-contract.js";
import type { PendingLearningReview } from "./learning-review-store.js";

export function idleLearningState(): LearningState {
  return { active: false, candidateCount: 0 };
}

export function pendingReviewState(
  pending: PendingLearningReview,
): LearningState {
  const candidates = pending.candidates ?? [];
  return {
    active: false,
    normalizedSite: pending.normalizedSite,
    candidateCount: candidates.length,
    review: {
      normalizedSite: pending.normalizedSite,
      candidates,
    },
  };
}

export function validateCandidateSelection(
  pending: PendingLearningReview,
  candidates: NonNullable<PendingLearningReview["candidates"]>,
  selectedDomains: readonly string[],
): void {
  if (selectedDomains.length === 0 || selectedDomains.length > 256) {
    throw new Error("NP_EXTENSION_SELECTION_INVALID");
  }
  const selected = new Set(selectedDomains);
  if (selected.size !== selectedDomains.length) {
    throw new Error("NP_EXTENSION_SELECTION_INVALID");
  }
  const candidateDomains = new Set(
    candidates.map((candidate) => candidate.normalizedDomain),
  );
  if (
    !selected.has(pending.normalizedSite) ||
    selectedDomains.some((domain) => !candidateDomains.has(domain))
  ) {
    throw new Error("NP_EXTENSION_SELECTION_INVALID");
  }
}

export function validateCandidateList(
  pending: PendingLearningReview,
  result: CandidateListResult,
): void {
  if (
    result.session.sessionID !== pending.sessionID ||
    result.candidates.length === 0 ||
    result.candidates.length > 256
  ) {
    throw new Error("NP_EXTENSION_NATIVE_RESPONSE_INVALID");
  }
  const domains = new Set<string>();
  for (const candidate of result.candidates) {
    const domain = candidate.normalizedDomain;
    if (
      normalizeWebURL(`https://${domain}/`) !== domain ||
      domains.has(domain) ||
      !validCandidateMetadata(candidate)
    ) {
      throw new Error("NP_EXTENSION_NATIVE_RESPONSE_INVALID");
    }
    domains.add(domain);
  }
  if (!domains.has(pending.normalizedSite)) {
    throw new Error("NP_EXTENSION_MAIN_CANDIDATE_MISSING");
  }
}

function validCandidateMetadata(candidate: CandidateResult): boolean {
  const kinds: readonly CandidateResult["kind"][] = [
    "requiredFirstParty",
    "likelyApi",
    "likelyAuth",
    "likelyCdn",
    "thirdParty",
    "unknown",
  ];
  const counts = [
    candidate.evidenceCount,
    candidate.mainFrameCount,
    candidate.subresourceCount,
    candidate.redirectCount,
  ];
  return (
    kinds.includes(candidate.kind) &&
    typeof candidate.requiresConfirmation === "boolean" &&
    Number.isInteger(candidate.confidenceMillis) &&
    candidate.confidenceMillis >= 0 &&
    candidate.confidenceMillis <= 1_000 &&
    counts.every(
      (value) => Number.isSafeInteger(value) && value >= 0,
    ) &&
    candidate.evidenceCount > 0 &&
    candidate.mainFrameCount +
      candidate.subresourceCount +
      candidate.redirectCount ===
      candidate.evidenceCount &&
    Number.isSafeInteger(candidate.firstSeenAtUnixMilliseconds) &&
    Number.isSafeInteger(candidate.lastSeenAtUnixMilliseconds) &&
    candidate.firstSeenAtUnixMilliseconds >= 0 &&
    candidate.firstSeenAtUnixMilliseconds <=
      candidate.lastSeenAtUnixMilliseconds
  );
}

export function resourceType(value: string): ResourceType {
  switch (value) {
    case "main_frame":
      return "mainFrame";
    case "sub_frame":
      return "subFrame";
    case "script":
      return "script";
    case "stylesheet":
      return "styleSheet";
    case "image":
      return "image";
    case "font":
      return "font";
    case "media":
      return "media";
    case "xmlhttprequest":
      return "xmlHttpRequest";
    case "websocket":
      return "webSocket";
    default:
      return "other";
  }
}
