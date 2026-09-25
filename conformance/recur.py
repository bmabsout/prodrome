# conformance/recur.py — SPEC §7.3 vectors for `Tended`, `Recur` and
# `Periodic`, in the grammar of §2, read through `tests/common/vectors.rs`'s
# VECTORS vocabulary. WRITTEN BY HAND from the laws, not generated: every
# value below is arithmetic a reader can redo. Frozen evidence: nothing under
# `cargo test` writes this file.
#
# `events` fold at `at` under the reference policy with `untrusted` on its
# roster: the toothbrush is tended on 10 Jun and 12 Aug, and the 20 Sep pass
# is a triage claim, which binds nothing. Its body is a decay from 0.98 on its
# anchor to 0.3 sixty days on; `Recur` slides it to the last binding tending.
# The rent is a curve from 1.0 to 0.0 over thirty days, repeated from 1 Sep
# in both directions.
Recurs(untrusted=('triage',), events=(
"Tended(todo='toothbrushvinegar', at=datetime(2026, 6, 10, 0, 0, 0), actor='bassel', note='')",
"Tended(todo='toothbrushvinegar', at=datetime(2026, 8, 12, 0, 0, 0), actor='bassel', note='vinegared')",
"Tended(todo='toothbrushvinegar', at=datetime(2026, 9, 20, 0, 0, 0), actor='triage', note='')",
), at=datetime(2026, 12, 31, 0, 0, 0), cases=(
RecurCase(term="Recur(todo='toothbrushvinegar', anchor=datetime(2026, 8, 12, 0, 0, 0), term=Decay(start=0.98, end=0.3, end_date=datetime(2026, 10, 11, 0, 0, 0), lead_up=timedelta(days=60), start_date=None), pending=Flat(value=0.3))", samples=(
Sample(now=datetime(2026, 6, 1, 0, 0, 0), value=0.3),
Sample(now=datetime(2026, 6, 10, 0, 0, 0), value=0.98),
Sample(now=datetime(2026, 7, 10, 0, 0, 0), value=0.64),
Sample(now=datetime(2026, 8, 11, 0, 0, 0), value=0.3),
Sample(now=datetime(2026, 8, 12, 0, 0, 0), value=0.98),
Sample(now=datetime(2026, 9, 21, 0, 0, 0), value=0.5266666666666667),
Sample(now=datetime(2026, 9, 25, 0, 0, 0), value=0.48133333333333334),
Sample(now=datetime(2026, 10, 20, 0, 0, 0), value=0.3),
)),
RecurCase(term="Recur(todo='never', anchor=datetime(2026, 8, 12, 0, 0, 0), term=Flat(value=1.0), pending=Flat(value=0.2))", samples=(
Sample(now=datetime(2026, 6, 1, 0, 0, 0), value=0.2),
Sample(now=datetime(2026, 12, 1, 0, 0, 0), value=0.2),
)),
RecurCase(term="Periodic(period=timedelta(days=30), anchor=datetime(2026, 9, 1, 0, 0, 0), term=Curve(points=(CurvePoint(at=datetime(2026, 9, 1, 0, 0, 0), value=1.0, label=''), CurvePoint(at=datetime(2026, 10, 1, 0, 0, 0), value=0.0, label=''))))", samples=(
Sample(now=datetime(2026, 8, 17, 0, 0, 0), value=0.5),
Sample(now=datetime(2026, 9, 1, 0, 0, 0), value=1.0),
Sample(now=datetime(2026, 9, 16, 0, 0, 0), value=0.5),
Sample(now=datetime(2026, 9, 30, 12, 0, 0), value=0.016666666666666666),
Sample(now=datetime(2026, 10, 1, 0, 0, 0), value=1.0),
Sample(now=datetime(2026, 10, 16, 0, 0, 0), value=0.5),
)),
))
