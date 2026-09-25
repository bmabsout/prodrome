# conformance/link.py — SPEC §7.2 vectors for `Ref` and `link`, in the grammar
# of §2, read through `tests/common/vectors.rs`'s VECTORS vocabulary. WRITTEN
# BY HAND from the laws, not generated: every value below is arithmetic a
# reader can redo. Frozen evidence: nothing under `cargo test` writes this
# file. `specs` is every todo's function; `x` and `y` loop, and only the case
# that reaches them is refused.
Links(specs=(
Spec(todo='a', term='Flat(value=0.8)'),
Spec(todo='b', term="Curve(points=(CurvePoint(at=datetime(2026, 10, 1, 0, 0, 0), value=0.0, label=''), CurvePoint(at=datetime(2026, 10, 11, 0, 0, 0), value=1.0, label='')))"),
Spec(todo='c', term="Conj(terms=(Ref(todo='a'), Ref(todo='b')), p=-1.0)"),
Spec(todo='d', term="Piecewise(head=Ref(todo='c'), pieces=(Piece(at=datetime(2026, 10, 6, 0, 0, 0), term=Flat(value=1.0)),))"),
Spec(todo='e', term="After(event='a', anchor=datetime(2026, 10, 1, 0, 0, 0), term=Ref(todo='b'), pending=Flat(value=0.3), needs=None)"),
Spec(todo='x', term="Offset(delta=0.5, term=Ref(todo='y'))"),
Spec(todo='y', term="Conj(terms=(Flat(value=0.5), Ref(todo='x')), p=-4.0)"),
), env=(
Bound(todo='a', kind='Completed', at=datetime(2026, 10, 3, 0, 0, 0)),
), cases=(
LinkCase(term="Ref(todo='a')", linked='Flat(value=0.8)', samples=(Sample(now=datetime(2026, 10, 1, 0, 0, 0), value=0.8),), refused=None),
LinkCase(term="Ref(todo='b')", linked="Curve(points=(CurvePoint(at=datetime(2026, 10, 1, 0, 0, 0), value=0.0, label=''), CurvePoint(at=datetime(2026, 10, 11, 0, 0, 0), value=1.0, label='')))", samples=(Sample(now=datetime(2026, 9, 20, 0, 0, 0), value=0.0), Sample(now=datetime(2026, 10, 6, 0, 0, 0), value=0.5), Sample(now=datetime(2026, 10, 20, 0, 0, 0), value=1.0)), refused=None),
LinkCase(term="Ref(todo='c')", linked="Conj(terms=(Flat(value=0.8), Curve(points=(CurvePoint(at=datetime(2026, 10, 1, 0, 0, 0), value=0.0, label=''), CurvePoint(at=datetime(2026, 10, 11, 0, 0, 0), value=1.0, label='')))), p=-1.0)", samples=(Sample(now=datetime(2026, 10, 1, 0, 0, 0), value=0.0019975031210986267), Sample(now=datetime(2026, 10, 6, 0, 0, 0), value=0.6153846153846154), Sample(now=datetime(2026, 10, 11, 0, 0, 0), value=0.8888888888888888)), refused=None),
LinkCase(term="Ref(todo='d')", linked="Piecewise(head=Conj(terms=(Flat(value=0.8), Curve(points=(CurvePoint(at=datetime(2026, 10, 1, 0, 0, 0), value=0.0, label=''), CurvePoint(at=datetime(2026, 10, 11, 0, 0, 0), value=1.0, label='')))), p=-1.0), pieces=(Piece(at=datetime(2026, 10, 6, 0, 0, 0), term=Flat(value=1.0)),))", samples=(Sample(now=datetime(2026, 10, 1, 0, 0, 0), value=0.0019975031210986267), Sample(now=datetime(2026, 10, 5, 0, 0, 0), value=0.5333333333333333), Sample(now=datetime(2026, 10, 6, 0, 0, 0), value=1.0)), refused=None),
LinkCase(term="Ref(todo='e')", linked="After(event='a', anchor=datetime(2026, 10, 1, 0, 0, 0), term=Curve(points=(CurvePoint(at=datetime(2026, 10, 1, 0, 0, 0), value=0.0, label=''), CurvePoint(at=datetime(2026, 10, 11, 0, 0, 0), value=1.0, label=''))), pending=Flat(value=0.3), needs=None)", samples=(Sample(now=datetime(2026, 10, 2, 0, 0, 0), value=0.3), Sample(now=datetime(2026, 10, 3, 0, 0, 0), value=0.0), Sample(now=datetime(2026, 10, 8, 0, 0, 0), value=0.5), Sample(now=datetime(2026, 10, 13, 0, 0, 0), value=1.0)), refused=None),
LinkCase(term="Piecewise(head=Flat(value=0.1), pieces=(Piece(at=datetime(2026, 10, 2, 0, 0, 0), term=Ref(todo='d')),))", linked="Piecewise(head=Flat(value=0.1), pieces=(Piece(at=datetime(2026, 10, 2, 0, 0, 0), term=Conj(terms=(Flat(value=0.8), Curve(points=(CurvePoint(at=datetime(2026, 10, 1, 0, 0, 0), value=0.0, label=''), CurvePoint(at=datetime(2026, 10, 11, 0, 0, 0), value=1.0, label='')))), p=-1.0)), Piece(at=datetime(2026, 10, 6, 0, 0, 0), term=Flat(value=1.0))))", samples=(Sample(now=datetime(2026, 10, 1, 0, 0, 0), value=0.1), Sample(now=datetime(2026, 10, 5, 0, 0, 0), value=0.5333333333333333), Sample(now=datetime(2026, 10, 7, 0, 0, 0), value=1.0)), refused=None),
LinkCase(term='Offset(delta=0.5, term=Flat(value=0.2))', linked='Offset(delta=0.5, term=Flat(value=0.2))', samples=(Sample(now=datetime(2026, 10, 1, 0, 0, 0), value=0.6),), refused=None),
LinkCase(term="Ref(todo='z')", linked=None, samples=(), refused=Refused(kind='unknown', todos=('z',))),
LinkCase(term="Ref(todo='x')", linked=None, samples=(), refused=Refused(kind='cycle', todos=('x', 'y', 'x'))),
))
