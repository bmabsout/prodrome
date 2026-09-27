# The Prodrome's roadmap, as a Prodrome

This branch is DATA, kept apart from the code on `main` so that each has its
own history. `roadmap/` is a store like any other: a directory of objects,
each named by the hash of its bytes, with the heads derived from them
(SPEC §3, since 0.9.0). Nothing here is ever rewritten.

- **Read it:** check this branch out and run `prodrome list`. The CLI defaults
  to `./roadmap`, and the list is ascending in fulfillment, so the top of it is
  the most urgent thing. A browser view is coming, served from this
  repository's GitHub Pages.
- **Issues become items.** An issue opened on this repository is mirrored here
  as a todo, and its page links back to it. Closing the issue completes the
  todo, and reopening it reopens the todo.
- **Pricing.** A maintainer prices an item with FPL (a flat value, a deadline,
  or a dependency on another item). A suggested price from the pricing bot is
  stored as a CLAIM: it is shown beside the item's value, and binds nothing
  until a maintainer accepts it. The scale is in [`PRICING.md`](PRICING.md).
- **Contributing.** Objects arrive by pull request, like code. A pull
  request's diff is exactly the objects it adds, and merging one is a
  maintainer's act.
