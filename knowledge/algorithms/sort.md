---
id: algorithm.sort
kind: knowledge
depends_on:
  - algorithm.compare
---
# In-place insertion sort

Sort a finite mutable sequence in ascending order by inserting each element
into the already-sorted prefix. Preserve all input elements. The algorithm
uses constant auxiliary storage, takes O(n^2) comparisons and moves in the
worst case, and is stable when equal elements are not moved past one another.

An empty or one-element sequence is already sorted. The target-specific
implementation must account for the element width, address arithmetic, and
available memory.
