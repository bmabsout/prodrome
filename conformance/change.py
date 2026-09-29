# conformance/change.py — Draft A's vectors for stores of changes, in the
# grammar of §2, read through `tests/common/vectors.rs`'s VECTORS vocabulary.
# WRITTEN BY HAND from laws 19–21, 23, 24 and 28, not generated: every print
# and every dep below is what those laws say a writer holding the case's
# `base` (and `conformance/dag.py`'s store `dag`, where one is named)
# writes, and every name is the sha256 of its print. A step appends `event`
# or snapshots, as the handle writing into `genesis` ('' for the store's one
# genesis), and answers `answer` ('' for a refusal), writing one object
# when `writes`; `objects` is everything the steps wrote. Each `Price` is
# a todo's state and value at `at` under the reference policy with
# `{'triage'}` on its roster, in the prodrome `genesis` ('' for the legacy
# one); a value of None is not priced here. Frozen evidence: nothing under
# `cargo test` writes this file.
Changes(cases=(
ChangeCase(name='a small store of changes', dag=None, base=(
Object(name='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', literal="Genesis(label='conformance', nonce='0123456789abcdef0123456789abcdef')"),
), steps=(
Step(op='append', genesis='', event="Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='write the vectors', note='')", answer='705c3456667ba77b1bad508f5778fe6b1b8437b22480975922f91663b9e1d863', writes=True),
Step(op='append', genesis='', event="SpecRevised(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', spec=Flat(value=0.5), note='')", answer='2796db7bd2cfddb2cb215222acad62c8451d825bb7b75562aef60f3968075634', writes=True),
Step(op='append', genesis='', event="Completed(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note='')", answer='434fe076b1ac8509ad3ad2eccc3556a937966a4d66252777d801c516a364a073', writes=True),
Step(op='append', genesis='', event="Reopened(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', note='')", answer='b518da080ae2fbddb0a89b083831ddba8f040439270f2e3fd8869c8181d20562', writes=True),
Step(op='append', genesis='', event="SpecRevised(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', spec=Flat(value=0.8), note='')", answer='4ca57e19002a96f9fb43c08808942026f8b19b2ab782bb51acb64f7b17116c0a', writes=True),
Step(op='append', genesis='', event="Created(todo='beta', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', text='drop it', note='')", answer='baa8fe2abe78eced46d95d10e63cdf3cc267635bd339471008bc2dcede2ad802', writes=True),
Step(op='append', genesis='', event="Cancelled(todo='beta', at=datetime(2026, 9, 4, 12, 0, 0), actor='bassel', note='')", answer='998b14319e6c26e62cf1f91178c1ed3f85663c91fe3eac8c0e0c57014637aa5d', writes=True),
), objects=(
Object(name='2796db7bd2cfddb2cb215222acad62c8451d825bb7b75562aef60f3968075634', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=SpecRevised(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', spec=Flat(value=0.5), note=''))"),
Object(name='434fe076b1ac8509ad3ad2eccc3556a937966a4d66252777d801c516a364a073', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Completed(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note=''))"),
Object(name='4ca57e19002a96f9fb43c08808942026f8b19b2ab782bb51acb64f7b17116c0a', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=('2796db7bd2cfddb2cb215222acad62c8451d825bb7b75562aef60f3968075634',), event=SpecRevised(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', spec=Flat(value=0.8), note=''))"),
Object(name='705c3456667ba77b1bad508f5778fe6b1b8437b22480975922f91663b9e1d863', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='write the vectors', note=''))"),
Object(name='998b14319e6c26e62cf1f91178c1ed3f85663c91fe3eac8c0e0c57014637aa5d', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Cancelled(todo='beta', at=datetime(2026, 9, 4, 12, 0, 0), actor='bassel', note=''))"),
Object(name='b518da080ae2fbddb0a89b083831ddba8f040439270f2e3fd8869c8181d20562', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=('434fe076b1ac8509ad3ad2eccc3556a937966a4d66252777d801c516a364a073',), event=Reopened(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', note=''))"),
Object(name='baa8fe2abe78eced46d95d10e63cdf3cc267635bd339471008bc2dcede2ad802', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Created(todo='beta', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', text='drop it', note=''))"),
), verify=(), at=datetime(2026, 9, 5, 12, 0, 0), prices=(
Price(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', todo='alpha', state='open', value=0.8),
Price(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', todo='beta', state='cancelled', value=None),
)),
ChangeCase(name='a replay writes nothing', dag=None, base=(
Object(name='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', literal="Genesis(label='conformance', nonce='0123456789abcdef0123456789abcdef')"),
), steps=(
Step(op='append', genesis='', event="Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='a proposal', note='')", answer='d60f23f15a50239fc23c10fbb269b3c53c788e5c3ce96333287c76d4e114292c', writes=True),
Step(op='append', genesis='', event="Completed(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='triage', note='proposed')", answer='94e2b141334e3516b0aa7cbe8505d1a9ea6dc1ee0d712ca9be8c1d43585d18ac', writes=True),
Step(op='append', genesis='', event="Reopened(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', note='overridden')", answer='10a536bd28c63b50256711f67d6505dcce8faa64073dd0ab3aacf81ad0d6cca2', writes=True),
Step(op='append', genesis='', event="Completed(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='triage', note='proposed')", answer='94e2b141334e3516b0aa7cbe8505d1a9ea6dc1ee0d712ca9be8c1d43585d18ac', writes=False),
Step(op='append', genesis='', event="Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='a proposal', note='')", answer='d60f23f15a50239fc23c10fbb269b3c53c788e5c3ce96333287c76d4e114292c', writes=False),
Step(op='append', genesis='', event="Reopened(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', note='overridden')", answer='10a536bd28c63b50256711f67d6505dcce8faa64073dd0ab3aacf81ad0d6cca2', writes=False),
), objects=(
Object(name='10a536bd28c63b50256711f67d6505dcce8faa64073dd0ab3aacf81ad0d6cca2', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=('94e2b141334e3516b0aa7cbe8505d1a9ea6dc1ee0d712ca9be8c1d43585d18ac',), event=Reopened(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', note='overridden'))"),
Object(name='94e2b141334e3516b0aa7cbe8505d1a9ea6dc1ee0d712ca9be8c1d43585d18ac', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Completed(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='triage', note='proposed'))"),
Object(name='d60f23f15a50239fc23c10fbb269b3c53c788e5c3ce96333287c76d4e114292c', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='a proposal', note=''))"),
), verify=(), at=datetime(2026, 9, 4, 12, 0, 0), prices=(
Price(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', todo='alpha', state='open', value='absent'),
)),
ChangeCase(name='a twin pair', dag=None, base=(
Object(name='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', literal="Genesis(label='conformance', nonce='0123456789abcdef0123456789abcdef')"),
Object(name='5e01aa2840edd8a35c33f3ddbc6ed9672adc5f2ea41722b6b5f636c4a5a6fcab', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Completed(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', note=''))"),
Object(name='4297393ca3e35a6eb91a2b52ad9fdac24b76bc9fb313781d4c55d2f51e73c13a', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Cancelled(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', note=''))"),
Object(name='65490c9992f61fcb4def0cf215e80fde80d160ec8bf01742f54782a01d36b5b3', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=('5e01aa2840edd8a35c33f3ddbc6ed9672adc5f2ea41722b6b5f636c4a5a6fcab',), event=Reopened(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note=''))"),
Object(name='b3ad7d6ed4c0821e9a1f0ca3848959772244def75c449ba03b531f29640dd59c', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=('4297393ca3e35a6eb91a2b52ad9fdac24b76bc9fb313781d4c55d2f51e73c13a',), event=Reopened(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note=''))"),
), steps=(
Step(op='append', genesis='', event="Reopened(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note='')", answer='65490c9992f61fcb4def0cf215e80fde80d160ec8bf01742f54782a01d36b5b3', writes=False),
), objects=(
), verify=(), at=datetime(2026, 9, 3, 12, 0, 0), prices=(
Price(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', todo='alpha', state='open', value='absent'),
)),
ChangeCase(name='a conflict prices as its most urgent world', dag=None, base=(
Object(name='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', literal="Genesis(label='conformance', nonce='0123456789abcdef0123456789abcdef')"),
Object(name='afe5c6dca8b6afb9d67719a74343a656ea83c1dacf5c026a8d10a72cf62583bb', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='priced twice', note=''))"),
Object(name='290b52e01506298925c6c4e5dbd443316569f055c16fb9e3ae36529ed0d76411', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=SpecRevised(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', spec=Flat(value=0.3), note='mine'))"),
Object(name='e14fa7a06268f337370a6509ee882706b13bedf47a0c6d0cb685c660c4cadbbc', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=SpecRevised(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', spec=Flat(value=0.7), note='theirs'))"),
), steps=(
Step(op='append', genesis='', event="SpecRevised(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', spec=Flat(value=0.3), note='mine')", answer='290b52e01506298925c6c4e5dbd443316569f055c16fb9e3ae36529ed0d76411', writes=False),
), objects=(
), verify=(), at=datetime(2026, 9, 2, 12, 0, 0), prices=(
Price(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', todo='alpha', state='open', value=0.3),
)),
ChangeCase(name='two geneses united', dag=None, base=(
Object(name='2ef158d23e67ee07de35d3c10063a40ac10d41afb963b645310a3fdefa20b82c', literal="Genesis(label='mine', nonce='0123456789abcdef0123456789abcdef')"),
Object(name='dff6a0a0c4844e7ba987c3a8d709fff5f67f2344df47e9f367edb85c04f8a560', literal="Genesis(label='theirs', nonce='0123456789abcdef0123456789abcdef')"),
), steps=(
Step(op='append', genesis='', event="Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='one todo, two prodromes', note='')", answer='', writes=False),
Step(op='append', genesis='2ef158d23e67ee07de35d3c10063a40ac10d41afb963b645310a3fdefa20b82c', event="Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='one todo, two prodromes', note='')", answer='c2f880d94e664772782236eff2ee7fa561fab119575f405db522cfdff1bb7962', writes=True),
Step(op='append', genesis='dff6a0a0c4844e7ba987c3a8d709fff5f67f2344df47e9f367edb85c04f8a560', event="Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='one todo, two prodromes', note='')", answer='cc91574c64faae89089fbc2c4114389b75e6fb1b7056cdcba7e05249c872323a', writes=True),
Step(op='append', genesis='2ef158d23e67ee07de35d3c10063a40ac10d41afb963b645310a3fdefa20b82c', event="Completed(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note='')", answer='2d06574e982c91cb1624d914b2a1e9e3ef2100db27e63ae3890fbbd1f7195f15', writes=True),
Step(op='append', genesis='dff6a0a0c4844e7ba987c3a8d709fff5f67f2344df47e9f367edb85c04f8a560', event="Reopened(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', note='')", answer='9af5c86e529b0aa0e88e10c6b3ed11639f2423d0dc725ac4890e2ecae79f3a3b', writes=True),
), objects=(
Object(name='2d06574e982c91cb1624d914b2a1e9e3ef2100db27e63ae3890fbbd1f7195f15', literal="Change(genesis='2ef158d23e67ee07de35d3c10063a40ac10d41afb963b645310a3fdefa20b82c', deps=(), event=Completed(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note=''))"),
Object(name='9af5c86e529b0aa0e88e10c6b3ed11639f2423d0dc725ac4890e2ecae79f3a3b', literal="Change(genesis='dff6a0a0c4844e7ba987c3a8d709fff5f67f2344df47e9f367edb85c04f8a560', deps=(), event=Reopened(todo='alpha', at=datetime(2026, 9, 3, 12, 0, 0), actor='bassel', note=''))"),
Object(name='c2f880d94e664772782236eff2ee7fa561fab119575f405db522cfdff1bb7962', literal="Change(genesis='2ef158d23e67ee07de35d3c10063a40ac10d41afb963b645310a3fdefa20b82c', deps=(), event=Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='one todo, two prodromes', note=''))"),
Object(name='cc91574c64faae89089fbc2c4114389b75e6fb1b7056cdcba7e05249c872323a', literal="Change(genesis='dff6a0a0c4844e7ba987c3a8d709fff5f67f2344df47e9f367edb85c04f8a560', deps=(), event=Created(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', text='one todo, two prodromes', note=''))"),
), verify=(), at=datetime(2026, 9, 4, 12, 0, 0), prices=(
Price(genesis='2ef158d23e67ee07de35d3c10063a40ac10d41afb963b645310a3fdefa20b82c', todo='alpha', state='completed', value=None),
Price(genesis='dff6a0a0c4844e7ba987c3a8d709fff5f67f2344df47e9f367edb85c04f8a560', todo='alpha', state='open', value='absent'),
)),
ChangeCase(name='a snapshot chain', dag=None, base=(
Object(name='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', literal="Genesis(label='conformance', nonce='0123456789abcdef0123456789abcdef')"),
), steps=(
Step(op='append', genesis='', event="Completed(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', note='')", answer='5e01aa2840edd8a35c33f3ddbc6ed9672adc5f2ea41722b6b5f636c4a5a6fcab', writes=True),
Step(op='snapshot', genesis='', event='', answer='7621ae40957e3986c53d07d5662984ebf88d2a40e3783a1633be0de865b045dc', writes=True),
Step(op='snapshot', genesis='', event='', answer='7621ae40957e3986c53d07d5662984ebf88d2a40e3783a1633be0de865b045dc', writes=False),
Step(op='append', genesis='', event="Reopened(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note='')", answer='65490c9992f61fcb4def0cf215e80fde80d160ec8bf01742f54782a01d36b5b3', writes=True),
Step(op='snapshot', genesis='', event='', answer='40b7ea8bc9032f05e315b4c214157a58be5d5465cec9159018be51ef27f4f9eb', writes=True),
), objects=(
Object(name='40b7ea8bc9032f05e315b4c214157a58be5d5465cec9159018be51ef27f4f9eb', literal="Snapshot(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', tips=('65490c9992f61fcb4def0cf215e80fde80d160ec8bf01742f54782a01d36b5b3',), previous='7621ae40957e3986c53d07d5662984ebf88d2a40e3783a1633be0de865b045dc')"),
Object(name='5e01aa2840edd8a35c33f3ddbc6ed9672adc5f2ea41722b6b5f636c4a5a6fcab', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=(), event=Completed(todo='alpha', at=datetime(2026, 9, 1, 12, 0, 0), actor='bassel', note=''))"),
Object(name='65490c9992f61fcb4def0cf215e80fde80d160ec8bf01742f54782a01d36b5b3', literal="Change(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', deps=('5e01aa2840edd8a35c33f3ddbc6ed9672adc5f2ea41722b6b5f636c4a5a6fcab',), event=Reopened(todo='alpha', at=datetime(2026, 9, 2, 12, 0, 0), actor='bassel', note=''))"),
Object(name='7621ae40957e3986c53d07d5662984ebf88d2a40e3783a1633be0de865b045dc', literal="Snapshot(genesis='d079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6', tips=('5e01aa2840edd8a35c33f3ddbc6ed9672adc5f2ea41722b6b5f636c4a5a6fcab', 'd079727b853e56c77dce07c3a355aa925267473706b3c767df12ad2adfe222d6'), previous='')"),
), verify=(), at=datetime(2026, 9, 3, 12, 0, 0), prices=(
)),
ChangeCase(name='a mixed store over a dag.py one', dag=27, base=(
), steps=(
Step(op='append', genesis='', event="Completed(todo='alpha', at=datetime(2026, 10, 2, 13, 51, 9), actor='bassel', note='')", answer='f3b4e27930f46d056be6e4caecb4cdbd582f5a8d9088cb42f86752a6cd3fe96e', writes=False),
Step(op='append', genesis='', event="Reopened(todo='beta', at=datetime(2026, 11, 1, 12, 0, 0), actor='bassel', note='settled')", answer='f57fa3ccc0201a824ecd5b210b3a255fabeb7da2f593d59e4f5048ea99e4857b', writes=True),
Step(op='append', genesis='', event="Completed(todo='alpha', at=datetime(2026, 11, 2, 12, 0, 0), actor='bassel', note='')", answer='c663c4e7cce794efd9025ba3bcd8c312a94330fd2de12d9b395c27343ae678fc', writes=True),
Step(op='append', genesis='', event="Reopened(todo='gamma', at=datetime(2026, 11, 3, 12, 0, 0), actor='bassel', note='')", answer='242028b0c3369c7fa4f353855b46faef9a9da9dc25a436ad1fb03ec03625a437', writes=True),
), objects=(
Object(name='242028b0c3369c7fa4f353855b46faef9a9da9dc25a436ad1fb03ec03625a437', literal="Change(genesis='77472fb8ecc9bb2812e758397dd3cdbdc8d2664859ce055b15d3607fa4afbf44', deps=('77472fb8ecc9bb2812e758397dd3cdbdc8d2664859ce055b15d3607fa4afbf44',), event=Reopened(todo='gamma', at=datetime(2026, 11, 3, 12, 0, 0), actor='bassel', note=''))"),
Object(name='c663c4e7cce794efd9025ba3bcd8c312a94330fd2de12d9b395c27343ae678fc', literal="Change(genesis='77472fb8ecc9bb2812e758397dd3cdbdc8d2664859ce055b15d3607fa4afbf44', deps=('f3b4e27930f46d056be6e4caecb4cdbd582f5a8d9088cb42f86752a6cd3fe96e',), event=Completed(todo='alpha', at=datetime(2026, 11, 2, 12, 0, 0), actor='bassel', note=''))"),
Object(name='f57fa3ccc0201a824ecd5b210b3a255fabeb7da2f593d59e4f5048ea99e4857b', literal="Change(genesis='77472fb8ecc9bb2812e758397dd3cdbdc8d2664859ce055b15d3607fa4afbf44', deps=('4094ba6abc3005442ae9c68664184484211840d9284f6634a8b67b28bc09dc29', '4f4e823084d3c17ff1b0ce0f726a530806f891966aeb06494ba50c2be0389d0e'), event=Reopened(todo='beta', at=datetime(2026, 11, 1, 12, 0, 0), actor='bassel', note='settled'))"),
), verify=(), at=datetime(2026, 11, 5, 12, 0, 0), prices=(
Price(genesis='', todo='alpha', state='completed', value=None),
Price(genesis='', todo='beta', state='open', value='absent'),
Price(genesis='', todo='gamma', state='open', value='absent'),
)),
))
