# Fonts

The viewer bundles these fonts for Typst and for the page. Source Serif 4 is
the design system's serif (`faces.serif` in `typst-design`); its display and
sans faces (Crimson Pro, Source Sans 3) are not bundled, and the page falls
through their stacks as the design system's own documents do. The font files
are not in this repository: the flake takes them from nixpkgs (`libertinus`
7.051, `source-serif` 4.005) when it builds the viewer, and they are served
beside this file.

| File                          | Family         | Licence                                            |
| ----------------------------- | -------------- | -------------------------------------------------- |
| `LibertinusSerif-Regular.otf` | Libertinus     | SIL Open Font License 1.1, `OFL-Libertinus.txt`    |
| `LibertinusSerif-Italic.otf`  | Libertinus     | SIL Open Font License 1.1, `OFL-Libertinus.txt`    |
| `LibertinusSerif-Bold.otf`    | Libertinus     | SIL Open Font License 1.1, `OFL-Libertinus.txt`    |
| `LibertinusMath-Regular.otf`  | Libertinus     | SIL Open Font License 1.1, `OFL-Libertinus.txt`    |
| `SourceSerif4-Regular.otf`    | Source Serif 4 | SIL Open Font License 1.1, `LICENSE-SourceSerif.md` |
| `SourceSerif4-It.otf`         | Source Serif 4 | SIL Open Font License 1.1, `LICENSE-SourceSerif.md` |
| `SourceSerif4-Bold.otf`       | Source Serif 4 | SIL Open Font License 1.1, `LICENSE-SourceSerif.md` |

The licence texts are copied verbatim from the fonts' own repositories
(alerque/libertinus at v7.051, adobe-fonts/source-serif at 4.005R).
