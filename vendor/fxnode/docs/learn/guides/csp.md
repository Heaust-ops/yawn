# Content Security Policy

fxnode starts a same-origin ES module worker with no blob, classic-worker, or main-thread fallback. Permit it with an appropriate `worker-src 'self'` and `script-src`, and serve JavaScript with the correct MIME type. Pass `workerUrl` if assets move independently.

For optional shared-memory input use `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`; embedded cross-origin resources must satisfy CORS or CORP. Otherwise fxnode automatically uses messages. Worker construction blocked synchronously reports `worker.construct`; a worker script/network/module load failure reports `worker.load`; failure to complete startup in time reports `worker.timeout`. None silently downgrade to main-thread execution.
