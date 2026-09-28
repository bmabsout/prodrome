// One item's page, from the example store's data; its text is Typst markup.
#import "@local/prodrome-typst:0.1.0": item
#let data = json("data.json")
#item(data, data.focus, markup: true)
