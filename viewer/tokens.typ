// The viewer's stylesheet tokens, read from the design system: each CSS
// custom property is a sample or a face `typst-design` names, never a hex
// picked here. `build.sh` queries `<tokens>` and puts the rule it holds at
// the head of `style.css`, so the page and the typeset content inside it
// wear one palette, and a change to a ramp reaches both.

#import "@local/typst-design:0.1.0" as design

#let colors = (
  ink: design.palette.ink,
  muted: design.palette.ink-muted,
  paper: design.palette.paper,
  primary: design.palette.primary,
  secondary: design.palette.secondary,
  rule: design.palette.rule,
  mark-fill: design.palette.mark-fill,
  mark-line: design.palette.mark-line,
  unpriced: design.fpl.unpriced,
  notice: design.ramps.orange.sample(design.jobs.notice),
  observed: design.roles.observed,
  acted: design.roles.acted,
  valued: design.roles.valued,
  ..design.chart.enumerate().map(((i, c)) => ("chart-" + str(i + 1), c)).to-dict(),
)

// A face's name in single quotes, so the rule holds no double quote and its
// JSON string reads back as itself.
#let stack(faces, generic) = {
  let faces = if type(faces) == str { (faces,) } else { faces }
  faces.map(face => "'" + face + "'").join(", ") + ", " + generic
}

#let fonts = (
  serif: stack(design.faces.serif, "Georgia, serif"),
  display: stack(design.faces.display, "Georgia, serif"),
  sans: stack(design.faces.sans, "system-ui, sans-serif"),
  mono: stack(design.faces.mono, "ui-monospace, monospace"),
)

#metadata(
  ":root { "
    + colors.pairs().map(((name, c)) => "--" + name + ": " + c.to-hex() + ";").join(" ")
    + " "
    + fonts.pairs().map(((name, faces)) => "--" + name + ": " + faces + ";").join(" ")
    + " }",
) <tokens>
