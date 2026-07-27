# Concepts

For integrators deciding where application responsibilities end and fxnode authority begins. Read in this order:

1. [Worker authority](./worker-authority): locate truth, work, and the asynchronous host boundary.
2. [Composition](./composition): model the application's graph language and live updates.
3. [Graph state and events](./graph-state-and-events): reason about documents, receipts, versions, and observation.
4. [State and persistence](./state-and-persistence): choose runtime replacement, canonical export, or replayable persistence.

Afterward you should be able to choose the correct API and concurrency domain, predict publication order, and design durable loading without treating fxnode as a graph evaluator. fxnode is an editor and presenter, not an evaluator.
