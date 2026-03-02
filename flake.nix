{
  description = "MaBlocks2 - A whiteboard-style desktop application for organizing images";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" "clippy" ];
          };
        in
        {
          default = pkgs.mkShell {
            name = "ma_blocks2-dev";
            
            buildInputs = with pkgs; [
              # Rust toolchain
              rustToolchain
              cargo
              rustc
              
              # Build essentials
              gcc
              gnumake
              meson
              ninja
              cmake
              nasm
              pkg-config
              python3 # Required by meson
              
              # LLVM/Clang (required for libavif-sys)
              llvmPackages.clang
              llvmPackages.libclang
              
              # Wayland support
              wayland
              wayland-protocols
              wayland-scanner
              libxkbcommon
              
              # X11 support
              libx11
              libxcursor
              libxrandr
              libxi
              libxinerama
              
              # Graphics/OpenGL
              mesa
              libGL
              libGLU
              libepoxy
              
              # GTK and DBus (for file dialogs via rfd)
              gtk3
              dbus
              libdbusmenu
              
              # SSL
              openssl
              
              # AVIF image support libraries
              dav1d
              libaom
              
              # Other image libraries (runtime deps)
              libpng
              libjpeg
              libwebp
              
              # Additional tools
              gdb
              valgrind
            ];

            shellHook = ''
              echo "🎨 MaBlocks2 Development Environment"
              echo ""
              echo "Rust version: $(rustc --version)"
              echo "Cargo version: $(cargo --version)"
              echo ""
              echo "Available commands:"
              echo "  cargo run          - Run in development mode"
              echo "  cargo build        - Build the project"
              echo "  cargo build --release  - Build release binary"
              echo ""
              echo "Wayland tip: Use WINIT_UNIX_BACKEND=wayland or WINIT_UNIX_BACKEND=x11 to force backend"
            '';

            # Environment variables for build
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            PKG_CONFIG_PATH = with pkgs; lib.makeSearchPath "lib/pkgconfig" [
              wayland.dev
              libxkbcommon.dev
              gtk3.dev
              openssl.dev
              libx11.dev
            ];
            
            # For wayland-client-sys and other -sys crates
            WAYLAND_PROTOCOLS_PATH = "${pkgs.wayland-protocols}";
            WAYLAND_SCANNER_PATH = "${pkgs.wayland-scanner}/bin/wayland-scanner";
          };
        });

      # Package definition with library wrapping
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          
          # Runtime libraries that need to be in LD_LIBRARY_PATH
          runtimeLibs = with pkgs; [
            wayland
            wayland-protocols
            libxkbcommon
            libx11
            libxcursor
            libxrandr
            libxi
            libxinerama
            mesa
            libGL
            libGLU
            gtk3
            dbus
            openssl
            dav1d
            libaom
            libpng
            libjpeg
            libwebp
          ];
        in
        {
          default = pkgs.stdenv.mkDerivation {
            pname = "ma_blocks2";
            version = "0.1.0";
            src = ./.;
            
            nativeBuildInputs = with pkgs; [
              cargo
              rustc
              rustPlatform.cargoSetupHook
              pkg-config
              cmake
              nasm
              meson
              ninja
              llvmPackages.clang
              makeWrapper
              python3
            ];
            
            buildInputs = runtimeLibs;
            
            cargoDeps = pkgs.rustPlatform.importCargoLock {
              lockFile = ./Cargo.lock;
            };
            
            env = {
              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
              WAYLAND_PROTOCOLS_PATH = "${pkgs.wayland-protocols}";
              WAYLAND_SCANNER_PATH = "${pkgs.wayland-scanner}/bin/wayland-scanner";
            };
            
            # Disable automatic configure hooks
            dontUseCmakeConfigure = true;
            dontUseMesonConfigure = true;
            
            buildPhase = ''
              export HOME=$TMPDIR
              cargo build --release
            '';
            
            installPhase = ''
              mkdir -p $out/bin
              cp target/release/ma_blocks2 $out/bin/
              
              wrapProgram $out/bin/ma_blocks2 \
                --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath runtimeLibs}" \
                --set WAYLAND_PROTOCOLS_PATH "${pkgs.wayland-protocols}" \
                --set WAYLAND_SCANNER_PATH "${pkgs.wayland-scanner}/bin/wayland-scanner"
            '';
            
            meta = with pkgs.lib; {
              description = "A whiteboard-style desktop application for organizing images";
              homepage = "https://github.com/yourusername/ma_blocks";
              license = licenses.mit;
              platforms = platforms.linux;
              mainProgram = "ma_blocks2";
            };
          };
        });
    };
}
