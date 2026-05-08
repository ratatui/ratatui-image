{ pkgs, src, self, system }:

let
  makeScreenshotTest = { terminal, terminalCommand, terminalPackages, setup ? null, xwayland ? false }: pkgs.testers.nixosTest {
    name = "ratatui-test-wayland-${terminal}";

    nodes.machine = { pkgs, ... }: {
      virtualisation.memorySize = 4096;
      virtualisation.resolution = { x = 2560; y = 1440; };

      programs.sway = {
        enable = true;
        wrapperFeatures.gtk = true;
      };

      programs.xwayland.enable = xwayland;

      services.xserver.enable = true;
      services.displayManager.sddm.enable = true;
      services.displayManager.sddm.wayland.enable = true;

      services.displayManager.autoLogin = {
        enable = true;
        user = "test";
      };

      services.displayManager.defaultSession = "sway";

      # Create test user
      users.users.test = {
        isNormalUser = true;
        extraGroups = [ "wheel" "video" ];
        packages = [ ];
      };

      # Fonts for proper Unicode rendering (chafa, braille, block elements, etc.)
      fonts.packages = with pkgs; [
        unifont          # ~77,000 glyphs, covers entire BMP
        noto-fonts       # Google's "no tofu" - comprehensive Unicode
        noto-fonts-lgc-plus
        noto-fonts-cjk-sans
        noto-fonts-color-emoji
        dejavu_fonts     # Good fallback with extended coverage
        freefont_ttf     # GNU FreeFont - wide Unicode range
        fira-code
        fira-mono
      ];

      # Ensure required packages are available
      environment.systemPackages = with pkgs;
        terminalPackages ++ [ chafa ];
    };

    testScript = ''
      machine.wait_for_unit("graphical.target")

      machine.wait_until_succeeds("pgrep -f sway")

      machine.succeed("mkdir -p /tmp/test-assets/assets")
      machine.copy_from_host("${src}/assets/Jenkins.png", "/tmp/test-assets/assets/Jenkins.png")

      machine.wait_until_succeeds("systemd-run --uid=test --setenv=XDG_RUNTIME_DIR=/run/user/1000 --setenv=WAYLAND_DISPLAY=wayland-1 -- swaymsg -t get_version")

      machine.succeed("${if setup != null then setup else "true"}")

      # Use systemd-run to ensure proper environment
      machine.succeed("""
        systemd-run --uid=test --setenv=XDG_RUNTIME_DIR=/run/user/1000 \
          --setenv=WAYLAND_DISPLAY=wayland-1 \
          --setenv=LIBGL_ALWAYS_SOFTWARE=1 \
          --setenv=QT_QPA_PLATFORM="wayland" \
          --setenv=RUST_BACKTRACE=full \
          ${if xwayland then "--setenv=DISPLAY=:0" else ""} \
          --working-directory=/tmp/test-assets \
          -- ${terminalCommand}
      """)

      print("Waiting for /tmp/demo-ready...")

      with subtest("Waiting for /tmp/demo-ready..."):
        try:
          machine.wait_until_succeeds("test -f /tmp/demo-ready", timeout=10)
        except Exception as e:
          print(f"/tmp/demo-ready not found within timeout: {e}")
        finally:
          machine.succeed("sleep 5")
          machine.screenshot("screenshot-${terminal}")
          print("Screenshot saved to test output directory as screenshot-${terminal}.png")
    '';
  };

  screenshotTests = {
    screenshot-test-foot = makeScreenshotTest {
      terminal = "foot";
      terminalCommand = "foot ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.foot ];
    };

    screenshot-test-kitty = makeScreenshotTest {
      terminal = "kitty";
      terminalCommand = "kitty ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.kitty ];
    };

    screenshot-test-wezterm = makeScreenshotTest {
      terminal = "wezterm";
      terminalCommand = "wezterm start --always-new-process --cwd /tmp/test-assets -- ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.wezterm ];
    };

    screenshot-test-ghostty = makeScreenshotTest {
      terminal = "ghostty";
      terminalCommand = "ghostty -e ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.ghostty ];
    };

    screenshot-test-mlterm = makeScreenshotTest {
      terminal = "mlterm";
      terminalCommand = "mlterm -e ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.mlterm ];
    };

    screenshot-test-rio = makeScreenshotTest {
      terminal = "rio";
      terminalCommand = "rio -w /tmp/test-assets -e ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.rio ];
      setup = "mkdir -p /home/test/.config/rio && touch /home/test/.config/rio/config.toml"; # Skip welcome screen
    };

    screenshot-test-xterm-vt340 = makeScreenshotTest {
      terminal = "xterm-vt340";
      terminalCommand = "xterm -ti vt340 -fa \"Noto Sans Mono\" -fs 16 -e ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.xterm ];
      xwayland = true;
    };

    screenshot-test-xterm = makeScreenshotTest {
      terminal = "xterm";
      terminalCommand = "xterm -fa \"Noto Sans Mono\" -fs 16 -e ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.xterm ];
      xwayland = true;
    };

    screenshot-test-blackbox = makeScreenshotTest {
      terminal = "blackbox";
      terminalCommand = "blackbox -c \"${self.packages.${system}.demo}/bin/demo --tmp-demo-ready\"";
      terminalPackages = [ pkgs.blackbox-terminal ];
    };

    screenshot-test-xfce4-terminal = makeScreenshotTest {
      terminal = "xfce4-terminal";
      terminalCommand = "xfce4-terminal -e \"${self.packages.${system}.demo}/bin/demo --tmp-demo-ready\"";
      terminalPackages = [ pkgs.xfce.xfce4-terminal ];
    };

    screenshot-test-contour = makeScreenshotTest {
      terminal = "contour";
      # This sleep is different: it's something about stdin not being ready immeadiately.
      terminalCommand = "contour --working-directory /tmp/test-assets /run/current-system/sw/bin/bash -c \"sleep 1; ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready\"";
      terminalPackages = [ pkgs.contour ];
    };

    screenshot-test-alacritty = makeScreenshotTest {
      terminal = "alacritty";
      terminalCommand = "alacritty -e ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.alacritty ];
    };

    screenshot-test-konsole = makeScreenshotTest {
      terminal = "konsole";
      terminalCommand = "konsole -e ${self.packages.${system}.demo}/bin/demo --tmp-demo-ready";
      terminalPackages = [ pkgs.kdePackages.konsole pkgs.kdePackages.qtwayland ];
    };
  };
in
screenshotTests
