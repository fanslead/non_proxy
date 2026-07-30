import type {
  CandidateResult,
  ConfirmLearningResult,
} from "../shared/native-contract.js";
import type { LearningConfirmationState } from "../shared/learning-state.js";

export interface PendingLearningReview {
  readonly tabID: number;
  readonly sessionID: string;
  readonly confirmationID: string;
  readonly normalizedSite: string;
  readonly candidates: readonly CandidateResult[] | null;
}

export interface CompletedLearningReview {
  readonly normalizedSite: string;
  readonly confirmation: LearningConfirmationState;
}

interface ReviewSource {
  readonly tabID: number;
  readonly sessionID: string;
  readonly normalizedSite: string;
}

export class LearningReviewStore {
  readonly #pending = new Map<number, PendingLearningReview>();
  readonly #completed = new Map<number, CompletedLearningReview>();

  get pendingCount(): number {
    return this.#pending.size;
  }

  begin(source: ReviewSource): PendingLearningReview {
    const current = this.#pending.get(source.tabID);
    if (current) {
      if (current.sessionID === source.sessionID) {
        return current;
      }
      throw new Error("NP_EXTENSION_REVIEW_PENDING");
    }
    this.#completed.delete(source.tabID);
    const review: PendingLearningReview = {
      ...source,
      confirmationID: `confirm-${crypto.randomUUID()}`,
      candidates: null,
    };
    this.#pending.set(source.tabID, review);
    return review;
  }

  pending(tabID: number): PendingLearningReview | null {
    return this.#pending.get(tabID) ?? null;
  }

  completed(tabID: number): CompletedLearningReview | null {
    return this.#completed.get(tabID) ?? null;
  }

  setCandidates(
    tabID: number,
    sessionID: string,
    candidates: readonly CandidateResult[],
  ): PendingLearningReview | null {
    const current = this.#pending.get(tabID);
    if (!current || current.sessionID !== sessionID) {
      return null;
    }
    const updated = { ...current, candidates: [...candidates] };
    this.#pending.set(tabID, updated);
    return updated;
  }

  complete(
    tabID: number,
    confirmationID: string,
    result: ConfirmLearningResult,
  ): CompletedLearningReview | null {
    const pending = this.#pending.get(tabID);
    if (!pending || pending.confirmationID !== confirmationID) {
      return null;
    }
    const completed: CompletedLearningReview = {
      normalizedSite: pending.normalizedSite,
      confirmation: {
        policyCount: result.policies.length,
        snapshotVersion: result.snapshotVersion,
        snapshotState: result.snapshotState,
        replayed: result.replayed,
      },
    };
    this.#pending.delete(tabID);
    this.#completed.set(tabID, completed);
    return completed;
  }

  clearCompleted(tabID: number): boolean {
    return this.#completed.delete(tabID);
  }

  discard(tabID: number): boolean {
    const pending = this.#pending.delete(tabID);
    const completed = this.#completed.delete(tabID);
    return pending || completed;
  }
}
