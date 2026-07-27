# Accessibility

The editor is bitmap Canvas. Its nodes, sockets, labels, controls, and relationships are not semantic accessibility-tree objects. fxnode makes **no WCAG conformance claim** and must not be the only interface when assistive access is required.

Applications should provide an equivalent semantic DOM workflow, status announcements, instructions, and controls. Keyboard support alone is not accessibility. An application-owned DOM add-node dialog can implement combobox/listbox semantics and focus restoration, but that does not make the Canvas editor accessible.
