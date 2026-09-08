{
  description = "Prodrome — a temporal, content-addressed event database with fulfillment-priority semantics";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
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

        # WHAT A BUILD IS ALLOWED TO SEE. Narrowed with `fileset` rather than
        # handed `./.`, so a README edit does not change a derivation's hash.
        # `conformance/` is in it because the tests read those vectors: a check
        # that cannot reach its evidence is a check that passes for the wrong
        # reason.
        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./core
            ./wasm
            ./conformance
          ];
        };

        # A CHECK IS A VERDICT, NOT AN ARTEFACT — hence `touch $out`. Both
        # checks below are what `nix develop -c cargo …` runs by hand, moved
        # somewhere `nix flake check` reaches them, so CI needs no rust of its
        # own and no network for the dependency tree: everything is vendored
        # from the committed `Cargo.lock` (`cargoLock.lockFile`).
        check = { name, phases }: pkgs.rustPlatform.buildRustPackage ({
          pname = name;
          inherit version src;
          cargoLock.lockFile = ./Cargo.lock;
          installPhase = "touch $out";
        } // phases);

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
        prodrome-wasm =
          let
            bindgen = pkgs.wasm-bindgen-cli_0_2_127;
          in
          pkgs.stdenv.mkDerivation {
            pname = "prodrome-wasm";
            inherit version src;

            cargoDeps = pkgs.rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };

            nativeBuildInputs = [
              pkgs.rustPlatform.cargoSetupHook
              pkgs.cargo
              pkgs.rustc
              pkgs.lld
              bindgen
              pkgs.binaryen # wasm-opt: -Os over what bindgen emits
            ];

            buildPhase = ''
              runHook preBuild
              cargo build --offline --frozen \
                --profile wasm-release --target wasm32-unknown-unknown -p prodrome-wasm
              runHook postBuild
            '';

            # One glue per host. `web/` is the ES module an application imports
            # at runtime; `nodejs/` is a CommonJS glue that reads the file off
            # disk, which is what lets a test run the SAME module the browser
            # does under node.
            installPhase = ''
                runHook preInstall
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
                runHook postInstall
            '';

            meta = {
              description = "The Prodrome's core as WebAssembly, with the JS glue a page loads";
              license = with lib.licenses; [ mit asl20 ];
            };
          };
      in
      {
        packages = {
          inherit prodrome-wasm;
          default = prodrome-wasm;
        };

        # `nix flake check` runs the crate's own suites — SPEC §9 against
        # `conformance/*.json` and the laws over generated logs and DAGs — and
        # then clippy at `-D warnings` over `--all-targets`, which is the tests
        # too.
        checks = {
          prodrome-core = check {
            name = "prodrome-core";
            phases = { buildAndTestSubdir = "core"; doCheck = true; };
          };
          prodrome-clippy = check {
            name = "prodrome-clippy";
            phases = {
              nativeBuildInputs = [ pkgs.clippy ];
              buildPhase = "cargo clippy --offline --all-targets -- -D warnings";
              doCheck = false;
            };
          };
          inherit prodrome-wasm;
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
