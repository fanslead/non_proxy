export interface ActiveLearningSession {
  readonly tabID: number;
  readonly sessionID: string;
  readonly browserContextID: string;
  readonly normalizedSite: string;
  readonly expiresAtUnixMilliseconds: number;
  readonly candidateDomains: ReadonlySet<string>;
}

export class LearningSessionStore {
  readonly #sessions = new Map<number, ActiveLearningSession>();

  get size(): number {
    return this.#sessions.size;
  }

  start(session: Omit<ActiveLearningSession, "candidateDomains">): void {
    if (this.#sessions.has(session.tabID)) {
      throw new Error("NP_EXTENSION_SESSION_ALREADY_ACTIVE");
    }
    this.#sessions.set(session.tabID, {
      ...session,
      candidateDomains: new Set(),
    });
  }

  get(
    tabID: number,
    nowUnixMilliseconds: number,
  ): ActiveLearningSession | null {
    const session = this.#sessions.get(tabID);
    if (
      !session ||
      nowUnixMilliseconds >= session.expiresAtUnixMilliseconds
    ) {
      return null;
    }
    return session;
  }

  addCandidate(tabID: number, normalizedDomain: string): void {
    const session = this.#sessions.get(tabID);
    if (!session) {
      return;
    }
    const candidateDomains = new Set(session.candidateDomains);
    candidateDomains.add(normalizedDomain);
    this.#sessions.set(tabID, {
      ...session,
      candidateDomains,
    });
  }

  remove(tabID: number): ActiveLearningSession | null {
    const session = this.#sessions.get(tabID) ?? null;
    this.#sessions.delete(tabID);
    return session;
  }

  removeExpired(
    nowUnixMilliseconds: number,
  ): readonly ActiveLearningSession[] {
    const removed: ActiveLearningSession[] = [];
    for (const session of this.#sessions.values()) {
      if (nowUnixMilliseconds >= session.expiresAtUnixMilliseconds) {
        this.#sessions.delete(session.tabID);
        removed.push(session);
      }
    }
    return removed;
  }
}
