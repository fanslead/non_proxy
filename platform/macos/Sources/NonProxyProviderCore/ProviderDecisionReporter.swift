import Foundation
import NonProxyProviderContracts
import Synchronization

public final class ProviderDecisionReporter:
    ProviderDecisionSubmitting,
    Sendable
{
    private struct QueueState: Sendable {
        var pending: [Nonproxy_Provider_V1_DecisionRecord] = []
        var inFlight: [Nonproxy_Provider_V1_DecisionRecord] = []
        var inFlightBatchID: String?
        var droppedEvents: UInt64 = 0
        var inFlightDroppedEvents: UInt64 = 0
        var stopped = false
    }

    private final class SharedState: Sendable {
        let queue = Mutex(QueueState())
        let signal: AsyncStream<Void>.Continuation

        init(signal: AsyncStream<Void>.Continuation) {
            self.signal = signal
        }
    }

    private struct Batch: Sendable {
        let decisions: [Nonproxy_Provider_V1_DecisionRecord]
        let batchID: String
        let droppedEvents: UInt64
    }

    private let state: SharedState
    private let capacity: Int
    private let batchSize: Int
    private let worker: Task<Void, Never>

    public init(
        control: any ProviderDecisionReporting,
        capacity: Int = 4_096,
        batchSize: Int = 128,
        retryDelay: Duration = .seconds(1)
    ) {
        precondition(capacity > 0)
        precondition(batchSize > 0 && batchSize <= capacity)
        let (stream, continuation) = AsyncStream<Void>.makeStream(
            bufferingPolicy: .bufferingNewest(1)
        )
        let shared = SharedState(signal: continuation)
        state = shared
        self.capacity = capacity
        self.batchSize = batchSize
        worker = Task {
            await Self.run(
                state: shared,
                stream: stream,
                control: control,
                batchSize: batchSize,
                retryDelay: retryDelay
            )
        }
    }

    @discardableResult
    public func submit(
        _ decision: Nonproxy_Provider_V1_DecisionRecord
    ) -> Bool {
        let accepted = state.queue.withLock { queue -> Bool in
            guard !queue.stopped,
                  queue.pending.count + queue.inFlight.count < capacity
            else {
                Self.recordDrop(queue: &queue)
                return false
            }
            queue.pending.append(decision)
            return true
        }
        if accepted {
            state.signal.yield()
        }
        return accepted
    }

    public func recordUnreportable() {
        state.queue.withLock { queue in
            Self.recordDrop(queue: &queue)
        }
    }

    public func stop() {
        let shouldStop = state.queue.withLock { queue -> Bool in
            guard !queue.stopped else {
                return false
            }
            queue.stopped = true
            return true
        }
        guard shouldStop else {
            return
        }
        state.signal.finish()
        worker.cancel()
    }

    private static func run(
        state: SharedState,
        stream: AsyncStream<Void>,
        control: any ProviderDecisionReporting,
        batchSize: Int,
        retryDelay: Duration
    ) async {
        for await _ in stream {
            while !Task.isCancelled {
                guard let batch = nextBatch(
                    state: state,
                    batchSize: batchSize
                ) else {
                    break
                }
                do {
                    try await control.reportDecisionBatch(
                        batch.decisions,
                        batchID: batch.batchID,
                        droppedEvents: batch.droppedEvents
                    )
                    completeBatch(state: state)
                } catch {
                    do {
                        try await Task.sleep(for: retryDelay)
                    } catch {
                        return
                    }
                }
            }
        }
    }

    private static func nextBatch(
        state: SharedState,
        batchSize: Int
    ) -> Batch? {
        state.queue.withLock { queue in
            if queue.inFlight.isEmpty {
                let count = min(batchSize, queue.pending.count)
                guard count > 0 else {
                    return nil
                }
                queue.inFlight = Array(queue.pending.prefix(count))
                queue.pending.removeFirst(count)
                queue.inFlightBatchID = UUID().uuidString.lowercased()
                queue.inFlightDroppedEvents = queue.droppedEvents
                queue.droppedEvents = 0
            }
            guard let batchID = queue.inFlightBatchID else {
                return nil
            }
            return Batch(
                decisions: queue.inFlight,
                batchID: batchID,
                droppedEvents: queue.inFlightDroppedEvents
            )
        }
    }

    private static func completeBatch(state: SharedState) {
        state.queue.withLock { queue in
            queue.inFlight.removeAll(keepingCapacity: true)
            queue.inFlightBatchID = nil
            queue.inFlightDroppedEvents = 0
        }
    }

    private static func recordDrop(queue: inout QueueState) {
        queue.droppedEvents = queue.droppedEvents == UInt64.max
            ? UInt64.max
            : queue.droppedEvents + 1
    }
}
