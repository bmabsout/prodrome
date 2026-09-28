/price 50

**Summary:** `verify` rehashes every object on each run, so it grows quadratically.
A cache of checked names would make it linear.

**Rationale:** a real bug with a workaround (run it less often), 50% on PRICING.md's scale.

**Duplicates:** none found.

**Labels:** bug, performance
