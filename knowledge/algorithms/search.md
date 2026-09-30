---
id: algorithm.search
kind: knowledge
depends_on:
  - algorithm.compare
---
# Linear search

Given a finite sequence and a key, inspect elements from the beginning until
the key is found or the sequence ends. Return the matching position when
found; otherwise report absence. An empty sequence produces absence without
reading memory.

The algorithm uses constant auxiliary storage and takes O(n) comparisons in
the worst case. Respect the caller's sequence bounds and element width.
