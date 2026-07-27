# Interactions

The host translates DOM input; the worker owns gesture state and commits. Supported editor gestures include movement, box selection, resize, link creation/replacement, Ctrl-right-click cutting, Ctrl-Alt-right-click muting, `M` node mute, `H` collapse, `G` modal move, and undo/redo.

Plain right-click on eligible empty canvas may request an add-node menu. The host owns its HTML, search, grouping, focus, dismissal, and calls `addNode`. Controls follow composition `ui` order. Scrubbing modifiers, text commit/cancel, and reset all become atomic commands.

Do not derive behavior from projected pixels or retain a parallel graph. Subscribe to committed events when application UI needs updates.
