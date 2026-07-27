# Graph state and events

Runtime graph state contains `graphId`, `catalogVersion`, nodes, links, and metadata. It is not the persistence envelope. The worker commits commands atomically and checks optional optimistic `expectedVersion` values.

Committed graph changes emit mutations before snapshots, in version order. Subscribers are isolated and return an unsubscribe function. Composition changes use a separate revision domain and emit `onCompositionChanges`; when rebinding changes a graph, the composition event precedes the matching mutation and snapshot.

Command and composition calls resolve with receipts only after authoritative validation and publication. Use their returned versions/revisions for the next compare-and-swap rather than inferring them from event timing. Structured validation/protocol failures reject without partial mutation; a `noop` receipt means no graph publication.

| Domain                 | Meaning                                | Advances on                        |
| ---------------------- | -------------------------------------- | ---------------------------------- |
| Graph `version`        | runtime document concurrency           | graph-changing command/load/rebind |
| Composition `revision` | live authority concurrency             | committed composition update       |
| `catalogVersion`       | bound composition version in documents | normalization/binding; persisted   |

Do not compare or substitute these values. Gesture previews remain worker-local until one commit.
