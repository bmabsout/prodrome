# The roadmap is a Prodrome

This crate's roadmap is a Prodrome store. It lives on its own branch,
[`roadmap-data`](https://github.com/bmabsout/prodrome/tree/roadmap-data),
apart from the code: a store of objects like any other, holding what this
crate has left to do. The [viewer](viewer.md) shows it in the browser.

## Read it

Check the branch out, and `prodrome list` folds it at any instant you ask
for. The list is ascending in fulfillment, so the most urgent item is at the
top.

```console
$ git switch roadmap-data
$ prodrome list
$ prodrome list --at 2026-09-09
$ prodrome list --untrusted pricing-bot   # the bot's suggestions as claims
```

CI on `main` checks the branch out and runs `verify` and `list` over it on
every change. So the code cannot stop reading its own data.

## Issues are items

An issue opened on this repository becomes item `gh-<number>` on the
roadmap branch, at a neutral 80%. Its page is
`https://bmabsout.github.io/prodrome/#/todo/gh-<number>`. Closing the issue
completes the item, and reopening the issue reopens it.

A maintainer prices an item with a comment, on the scale in the branch's
[`PRICING.md`](https://github.com/bmabsout/prodrome/blob/roadmap-data/PRICING.md):

```
/price 40
/price 30 --deadline 2026-10-15
/price ref gh-7
```

A pricing bot (Claude) suggests a price on every new issue. Its suggestion
is stored as a CLAIM by `pricing-bot`. The viewer shows it beside the item,
and it binds nothing until a maintainer replies `/price accept`.

The workflows are `.github/workflows/mirror.yml` and `price.yml`. Both run
one deterministic, offline binary, `prodrome-github`, from its own crate,
`github/`. It is an optional integration, like the viewer: nothing in
`core/` or `cli/` knows GitHub. [`github-agent.md`](github-agent.md)
explains the flow and where trust stops.

## Contribute to it

The roadmap branch changes the way the code changes: by a pull request,
against that branch. The store holds no policy, because standing belongs to
the host (SPEC §5), and a directory of objects is not a host. Content
addressing takes its place. Every object is named by the hash of its bytes,
nothing is ever rewritten, and a pull request's diff is exactly the objects
it adds. So reviewers read the objects like code, and merging them is a
maintainer's act.

To see what a particular writer's events would say without folding them,
pass `--untrusted NAME`. The list then shows the confirmed reading and the
claimed one side by side.
