public protocol NativeLearningServing: Sendable {
    func start(
        _ payload: StartLearningPayload
    ) async throws -> StartLearningResult

    func observe(
        _ payload: ObserveLearningPayload
    ) async throws -> ObservationResult

    func list(
        _ payload: SessionPayload
    ) async throws -> CandidateListResult

    func stop(
        _ payload: SessionPayload
    ) async throws -> StopLearningResult

    func shutdown()
}
