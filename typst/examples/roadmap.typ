// The list, from the example store's data. Compile from the repository root:
//   typst compile --package-path <dir holding local/prodrome-typst/0.1.0> \
//     --features html --format html typst/examples/roadmap.typ
#import "@local/prodrome-typst:0.1.0": roadmap
#roadmap(json("data.json"), markup: true)
