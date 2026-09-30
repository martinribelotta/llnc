---
id: algorithm.compare
kind: knowledge
---
# Comparison

Define comparisons in terms of the values and ordering required by the caller.
For unsigned integers, compare as unsigned values; for signed integers, use
signed ordering. Equality is true only when the represented values are equal.
Handle equality explicitly in algorithms that require a strict ordering.

Before selecting instructions, use the target description to determine the
available integer widths and signed/unsigned comparison operations.
