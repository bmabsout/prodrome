{
  description = "Prodrome — a temporal, content-addressed event database with fulfillment-priority semantics";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        inherit (nixpkgs) lib;
        pkgs = import nixpkgs { inherit system; };

        # THE VERSION IS READ, NEVER TYPED TWICE. Two places to write a version
        # is one place to forget it, so this comes out of the workspace
        # manifest — the one file cargo also reads it from.
        version =
          let
            line = lib.findFirst (lib.hasPrefix "version = ")
              (throw "flake.nix: no version line in Cargo.toml")
              (lib.splitString "\n" (builtins.readFile ./Cargo.toml));
          in
          lib.removeSuffix "\"" (lib.removePrefix "version = \"" line);

        craneLib = crane.mkLib pkgs;

        # Panic messages carry the path of the source they come from. Crane
        # vendors crates into the store, so without this every artefact would
        # name the vendored sources — bytes in the wasm, and all of them in
        # its closure — where nixpkgs' vendoring left `/build/…`.
        remapVendorDir = ''
          for registry in "$cargoVendorDir"/*/; do
            export CARGO_BUILD_RUSTFLAGS="--remap-path-prefix=''${registry%/}=/build/cargo-vendor-dir"
          done
        '';

        # WHAT A BUILD IS ALLOWED TO SEE. Narrowed with `fileset` rather than
        # handed `./.`, so a README edit does not change a derivation's hash,
        # and narrowed per derivation, so each rebuilds only for its own
        # crates. Every member's manifest is always in: cargo loads the whole
        # workspace to build any part of it. `conformance/` is in the
        # workspace check because the tests read those vectors: a check that
        # cannot reach its evidence is a check that passes for the wrong
        # reason. The roadmap is not in any of them: it lives on the
        # `roadmap-data` branch, DATA the binary reads and not source the
        # binary is built from, so a todo appended to it rebuilds nothing,
        # and CI runs `prodrome verify` against that branch's checkout.
        rustSrc = paths: lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions ([
            ./Cargo.toml
            ./Cargo.lock
            ./cli/Cargo.toml
            ./core/Cargo.toml
            ./github/Cargo.toml
            ./wasm/Cargo.toml
          ] ++ paths);
        };

        common = {
          inherit version;
          cargoVendorDir = craneLib.vendorCargoDeps { cargoLock = ./Cargo.lock; };
          preBuild = remapVendorDir;
          strictDeps = true;
          # No debuginfo in the dev and test profiles: no check runs a
          # debugger or prints a backtrace, and a panic names its line
          # without it. Here, so that the deps are built to match.
          CARGO_PROFILE_DEV_DEBUG = "0";
        };

        # THE DEPENDENCY TREE, BUILT ONCE. `buildDepsOnly` compiles it against
        # a dummy of the workspace made from the manifests alone, so this
        # rebuilds when Cargo.toml or Cargo.lock does and never for a `.rs`
        # edit. Everything native below starts from its target directory.
        cargoArtifacts = craneLib.buildDepsOnly (common // {
          pname = "prodrome";
          src = rustSrc [ ./cli ./core ./github ./wasm ];
          doCheck = false;
          # One cargo invocation per consumer below: features unify across
          # the packages an invocation selects, so each needs its own build.
          buildPhaseCargoCommand = ''
            cargo check --locked --workspace --all-targets
            cargo test --locked --workspace --no-run
            cargo build --release --locked -p prodrome-cli
            cargo build --release --locked -p prodrome-github
          '';
        });

        # THE CORE, FOR THE BROWSER (`nix build .#prodrome-wasm`).
        #
        # `wasm/` compiled to WebAssembly, so a tab runs the same evaluator a
        # server does instead of a second reading of the same spec. Built
        # purely: the dependencies are vendored from `Cargo.lock`, nothing here
        # reaches the network, and the output is the two glues (`web/` for a
        # bundler, `nodejs/` for a script) beside the one `.wasm` each.
        #
        # THE wasm32 TARGET COMES FROM NIXPKGS. `rustc --print target-list` has
        # wasm32-unknown-unknown and `$(rustc --print sysroot)/lib/rustlib/`
        # carries its std, so there is no rust-overlay and no fenix input to
        # keep in step — one fewer flake input for a toolchain the pinned
        # nixpkgs already ships. `lld` is on the PATH because that rustc links
        # wasm with the system one rather than a bundled rust-lld.
        #
        # THE TWO wasm-bindgen HALVES ARE PINNED TOGETHER. The crate is
        # `=0.2.127` in wasm/Cargo.toml and the generator is
        # `wasm-bindgen-cli_0_2_127` here — they negotiate over a schema
        # version compiled into both, so a drift is a loud failure at bindgen
        # time, and pinning only one of them would make that failure a
        # `nix flake update` away.
        wasmArgs = common // {
          src = rustSrc [ ./core/src ./wasm/src ];
          nativeBuildInputs = [ pkgs.lld ];
          doCheck = false;
          buildPhaseCargoCommand = ''
            cargo build --offline --frozen \
              --profile wasm-release --target wasm32-unknown-unknown -p prodrome-wasm
          '';
        };

        prodrome-wasm =
          let
            bindgen = pkgs.wasm-bindgen-cli_0_2_127;
          in
          craneLib.mkCargoDerivation (wasmArgs // {
            pname = "prodrome-wasm";
            cargoArtifacts = craneLib.buildDepsOnly (wasmArgs // { pname = "prodrome-wasm"; });

            nativeBuildInputs = [
              pkgs.lld
              bindgen
              pkgs.binaryen # wasm-opt: -Os over what bindgen emits
            ];

            doInstallCargoArtifacts = false;

            # One glue per host. `web/` is the ES module an application imports
            # at runtime; `nodejs/` is a CommonJS glue that reads the file off
            # disk, which is what lets a test run the SAME module the browser
            # does under node.
            installPhaseCommand = ''
                wasm=target/wasm32-unknown-unknown/wasm-release/prodrome_wasm.wasm
                # The proposals rustc's wasm32-unknown-unknown baseline already
                # emits. wasm-opt's own default is an older baseline, so it
                # REFUSES a module using bulk memory rather than quietly
                # dropping it — named here so the set is a decision and not a
                # flag copied off an error message.
                features="--enable-bulk-memory --enable-bulk-memory-opt --enable-sign-ext"
                features="$features --enable-nontrapping-float-to-int --enable-mutable-globals"
                features="$features --enable-multivalue --enable-reference-types"
                for target in web nodejs; do
                  wasm-bindgen --target "$target" --out-dir "$out/$target" --out-name prodrome "$wasm"
                  # shellcheck disable=SC2086
                  wasm-opt -Os $features -o "$out/$target/prodrome_bg.wasm" "$out/$target/prodrome_bg.wasm"
                done
                echo "prodrome.wasm: $(stat -c %s "$out/web/prodrome_bg.wasm") bytes"
            '';

            meta = {
              description = "The Prodrome's core as WebAssembly, with the JS glue a page loads";
              license = with lib.licenses; [ mit asl20 ];
            };
          });

        # TYPST FOR THE BROWSER (`nix build .#prodrome-typst-wasm`) — an
        # OPTIONAL EXTRA, outside the core's workspace and its lock file.
        #
        # `typst-wasm/` is its own cargo workspace with its own `Cargo.lock`,
        # so the typesetter's ~290 crates never enter the tree `prodrome-core`
        # and `prodrome-cli` are built from, and the sources above never see
        # it. Built the way `prodrome-wasm` is — vendored, offline, its
        # dependencies built once, the same pinned wasm-bindgen, `wasm-opt
        # -Os` — with the `web/` glue only: its one consumer is the viewer.
        # `SIZES` records the module raw and brotli-compressed, since the size
        # is what a first visit pays.
        typstCommon = {
          version = "0.1.0";
          src = lib.fileset.toSource {
            root = ./typst-wasm;
            fileset = lib.fileset.unions [
              ./typst-wasm/Cargo.toml
              ./typst-wasm/Cargo.lock
              ./typst-wasm/src
            ];
          };
          cargoVendorDir = craneLib.vendorCargoDeps { cargoLock = ./typst-wasm/Cargo.lock; };
          preBuild = remapVendorDir;
          strictDeps = true;
        };

        typstWasmArgs = typstCommon // {
          nativeBuildInputs = [ pkgs.lld ];
          doCheck = false;
          buildPhaseCargoCommand = ''
            cargo build --offline --frozen \
              --profile wasm-release --target wasm32-unknown-unknown
          '';
        };

        prodrome-typst-wasm = craneLib.mkCargoDerivation (typstWasmArgs // {
          pname = "prodrome-typst-wasm";
          cargoArtifacts = craneLib.buildDepsOnly (typstWasmArgs // { pname = "prodrome-typst-wasm"; });

          nativeBuildInputs = [
            pkgs.lld
            pkgs.wasm-bindgen-cli_0_2_127
            pkgs.binaryen
            pkgs.brotli
          ];

          doInstallCargoArtifacts = false;

          installPhaseCommand = ''
            wasm=target/wasm32-unknown-unknown/wasm-release/prodrome_typst_wasm.wasm
            features="--enable-bulk-memory --enable-bulk-memory-opt --enable-sign-ext"
            features="$features --enable-nontrapping-float-to-int --enable-mutable-globals"
            features="$features --enable-multivalue --enable-reference-types"
            wasm-bindgen --target web --out-dir "$out/web" --out-name typst "$wasm"
            # shellcheck disable=SC2086
            wasm-opt -Os $features -o "$out/web/typst_bg.wasm" "$out/web/typst_bg.wasm"
            raw=$(stat -c %s "$out/web/typst_bg.wasm")
            brotli=$(brotli -c -q 11 "$out/web/typst_bg.wasm" | wc -c)
            printf 'typst_bg.wasm\t%s bytes raw\t%s bytes brotli\n' "$raw" "$brotli" | tee "$out/SIZES"
          '';

          meta = {
            description = "Typst 0.15.1 as WebAssembly: HTML export, highlighting and completion";
            license = with lib.licenses; [ mit asl20 ];
          };
        });

        # THE VIEWER (`nix build .#prodrome-viewer`): the static app — no
        # data in it — that folds a store with prodrome-wasm and typesets it
        # with prodrome-typst-wasm and the `typst/` package. An EXAMPLE host.
        # TypeScript typechecked by nixpkgs' `tsc` and bundled by its
        # `esbuild`, so nothing comes from npm; `viewer/build.sh` is the whole
        # recipe, and `viewer/assemble.sh` puts a store's objects beside it.
        prodrome-viewer = pkgs.stdenv.mkDerivation {
          pname = "prodrome-viewer";
          version = "0.1.0";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [ ./viewer ./typst ];
          };
          nativeBuildInputs = [ pkgs.esbuild pkgs.typescript ];
          buildPhase = ''
            runHook preBuild
            sh viewer/build.sh "$out" ${prodrome-wasm}/web ${prodrome-typst-wasm}/web \
              ${pkgs.libertinus}/share/fonts ${pkgs.source-serif}/share/fonts
            runHook postBuild
          '';
          dontInstall = true;
          meta = {
            description = "A read-only Prodrome viewer: folded and typeset in the browser";
            license = with lib.licenses; [ mit asl20 ];
          };
        };

        # THE CORE AND THE CLI IN RELEASE, COMPILED ONCE. `prodrome-github`
        # links the same two crates with the same features, so both packages
        # start from this target directory: the CLI's binary is already built
        # and the mirror compiles only its own crate.
        cliArtifacts = craneLib.cargoBuild (common // {
          pname = "prodrome-cli";
          src = rustSrc [ ./core/src ./cli/src ];
          inherit cargoArtifacts;
          cargoExtraArgs = "--locked -p prodrome-cli";
          doCheck = false;
        });

        # THE BINARY (`nix build .#prodrome-cli`): what CI points at the
        # roadmap. Its tests ran in `prodrome-workspace`, not again here.
        prodrome-cli = craneLib.buildPackage (common // {
          pname = "prodrome-cli";
          src = rustSrc [ ./core/src ./cli/src ];
          cargoArtifacts = cliArtifacts;
          cargoExtraArgs = "--locked -p prodrome-cli";
          doCheck = false;
          meta = {
            description = "A command line over a Prodrome store";
            license = with lib.licenses; [ mit asl20 ];
            mainProgram = "prodrome";
          };
        });

        # THE GITHUB MIRROR (`nix build .#prodrome-github`), an integration kept
        # apart from the core and its CLI. Its fixture tests and laws ran in
        # `prodrome-workspace`.
        prodrome-github = craneLib.buildPackage (common // {
          pname = "prodrome-github";
          src = rustSrc [ ./core/src ./cli/src ./github/src ];
          cargoArtifacts = cliArtifacts;
          cargoExtraArgs = "--locked -p prodrome-github";
          doCheck = false;
          meta = {
            description = "Mirror GitHub issues into a Prodrome store";
            license = with lib.licenses; [ mit asl20 ];
            mainProgram = "prodrome-github";
          };
        });
      in
      {
        packages = {
          inherit prodrome-cli prodrome-github prodrome-wasm prodrome-typst-wasm prodrome-viewer;
          default = prodrome-cli;
        };

        checks = {
          # A CHECK IS A VERDICT, NOT AN ARTEFACT — hence `touch $out`. What
          # `nix develop -c cargo …` runs by hand, in one target directory:
          # every crate's suites — SPEC §9 against `conformance/*.json` and
          # the laws over generated logs and DAGs — and clippy at `-D
          # warnings` over `--all-targets`, which is the tests too. The dev
          # profile: it compiles the suites in a fraction of release's time,
          # and they run no slower.
          prodrome-workspace = craneLib.mkCargoDerivation (common // {
            pname = "prodrome-workspace";
            src = rustSrc [ ./cli ./core ./github ./wasm ./conformance ];
            inherit cargoArtifacts;
            nativeBuildInputs = [ pkgs.rustfmt pkgs.clippy ];
            buildPhaseCargoCommand = ''
              cargo fmt --all --check
              cargo clippy --locked --workspace --all-targets -- -D warnings
              cargo test --locked --workspace
            '';
            doInstallCargoArtifacts = false;
            installPhaseCommand = "touch $out";
          });
          # typst-wasm's own tests (offsets, errors as values, packages,
          # highlighting, completion), natively: a separate workspace, so a
          # separate check.
          prodrome-typst-wasm-tests = craneLib.mkCargoDerivation (typstCommon // {
            pname = "prodrome-typst-wasm-tests";
            cargoArtifacts = craneLib.buildDepsOnly (typstCommon // {
              pname = "prodrome-typst-wasm-tests";
              doCheck = false;
              buildPhaseCargoCommand = "cargo test --release --locked --no-run";
            });
            buildPhaseCargoCommand = "cargo test --release --locked";
            doInstallCargoArtifacts = false;
            installPhaseCommand = "touch $out";
          });
          # The Typst package compiles its examples with the typst the
          # pinned nixpkgs ships — 0.15.1, the version typst-wasm pins — to
          # PDF and to HTML, so a layout that breaks either target fails here,
          # and compiles its laws (`tests/laws.typ`), which fail the compile
          # when an assert does not hold.
          prodrome-typst = pkgs.runCommand "prodrome-typst-check"
            {
              nativeBuildInputs = [ pkgs.typst ];
              src = lib.fileset.toSource { root = ./typst; fileset = ./typst; };
            } ''
            mkdir -p pkgs/local/prodrome-typst
            cp -r "$src" pkgs/local/prodrome-typst/0.1.0
            chmod -R u+w pkgs
            cd pkgs/local/prodrome-typst/0.1.0
            typst compile --package-path "$NIX_BUILD_TOP/pkgs" tests/laws.typ "$NIX_BUILD_TOP/laws.pdf"
            cd examples
            for doc in roadmap item; do
              typst compile --package-path "$NIX_BUILD_TOP/pkgs" "$doc.typ" "$doc.pdf"
              typst compile --package-path "$NIX_BUILD_TOP/pkgs" --features html --format html "$doc.typ" "$doc.html"
            done
            grep -q 'href="#/todo/ship-the-viewer"' roadmap.html
            grep -q '<strong>the viewer</strong>' roadmap.html
            grep -q '<svg' item.html
            touch $out
          '';
          inherit prodrome-cli prodrome-github prodrome-wasm prodrome-typst-wasm prodrome-viewer;
        };

        # `nix develop` — the toolchain the checks above use, plus the editor's
        # server. `lld` is what the wasm32 target links with: nixpkgs' rustc
        # carries that target's std but expects the linker on PATH rather than
        # bundling rust-lld, and without it a wasm build stops at
        # "linker `lld` not found".
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc rustfmt clippy lld rust-analyzer ];
          RUST_BACKTRACE = "1";
        };
      });
}
