import type { CandidateResult } from "./native-contract.js";

export interface LearningReviewState {
  readonly normalizedSite: string;
  readonly candidates: readonly CandidateResult[];
}

export interface LearningConfirmationState {
  readonly policyCount: number;
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

export interface LearningState {
  readonly active: boolean;
  readonly normalizedSite?: string;
  readonly expiresAtUnixMilliseconds?: number;
  readonly candidateCount: number;
  readonly review?: LearningReviewState;
  readonly confirmation?: LearningConfirmationState;
}
