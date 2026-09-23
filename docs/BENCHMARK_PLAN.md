# Benchmark baseline plan

Criterion is deferred to avoid a new dependency and benchmark harness before adapter traffic is measured. The first baseline should run these fixed scenarios on one host and record toolchain, CPU, target and allocation counts alongside time:

1. Lifecycle dispatch: 0, 1, 8 and 64 observers; forward and reverse events; include snapshot allocation.
2. Back dispatch: 1, 8 and 64 handlers, mixed priorities and disabled states; ordinary back and gesture progress.
3. Instance lookup: hit and miss at 1, 8 and 64 retained keys, with type downcast on hits.
4. State save: 1, 8 and 64 providers with fixed 32-byte payloads; measure restored carry-forward separately.
5. FFI adapter comparison later: one batched event/result crossing versus equivalent fine-grained calls on Android, Apple and Web.

No container or callback representation should change solely because one synthetic baseline is faster. First correlate measurements with a real integration workload.
