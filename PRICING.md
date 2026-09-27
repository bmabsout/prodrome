# The roadmap's pricing scale

A price is a FULFILLMENT, from 0% to 100%, and LOW IS URGENT. It answers "how
well are things going if this stays as it is?", not "how important is this in
the abstract". Price against these anchors:

| Price | Anchor |
|---|---|
| 10% | stored data can be lost or corrupted, or there is a security hole |
| 30% | a core operation is broken for ordinary use (fold, verify, seal, sync) |
| 50% | a real bug with a workaround, or a spec ambiguity that bites implementers |
| 70% | a clearly useful feature that someone is waiting on |
| 90% | nice to have, with no one blocked |

- **Time:** a deadline is a decay toward its date, not a flat low number.
- **Dependencies:** an item that waits on another is priced as a reference to
  it, so it is exactly as urgent as the thing it waits on.
- **The bot:** its suggestions are claims; a maintainer's `/price` binds.
