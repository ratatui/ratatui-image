fn main() {
    // chafa-dyn: Use pkg-config to find chafa and emit link flags
    #[cfg(feature = "chafa-dyn")]
    {
        pkg_config::Config::new()
            // https://github.com/hpjansson/chafa/commit/b1ddce829798a81db54572261e6864ebab171631
            // 1.8.0 added chafa_canvas_get_char_at()
            .atleast_version("1.8.0")
            .probe("chafa")
            .expect(
                "Failed to find chafa via pkg-config. Install libchafa-dev or set PKG_CONFIG_PATH. Needs version >= 1.8.0.",
            );
    }

    // chafa-static: Static linking only (no fallback)
    #[cfg(all(feature = "chafa-static", not(feature = "chafa-dyn")))]
    {
        let lib = pkg_config::Config::new()
            .statik(true)
            .probe("chafa")
            .expect(
                "Failed to find chafa via pkg-config. Install libchafa-dev or set PKG_CONFIG_PATH.",
            );

        // Find the static library
        for path in &lib.link_paths {
            let static_lib = path.join("libchafa.a");
            if static_lib.exists() {
                // Use this path for static linking
                println!("cargo:rustc-link-search=native={}", path.display());
                println!("cargo:rustc-link-lib=static=chafa");

                // Also link dependencies that pkg-config found
                for link_path in &lib.link_paths {
                    if link_path != path {
                        println!("cargo:rustc-link-search=native={}", link_path.display());
                    }
                }

                // Link glib and its dependencies statically
                println!("cargo:rustc-link-lib=static=glib-2.0");
                println!("cargo:rustc-link-lib=static=sysprof-capture-4");
                println!("cargo:rustc-link-lib=pcre2-8");
                println!("cargo:rustc-link-lib=m");

                return;
            }
        }

        // No static lib found - this is an error for chafa-static
        panic!(
            "chafa-static feature requires libchafa.a but it was not found in {:?}. \
             Either build chafa with --enable-static or use chafa-dyn for dynamic linking.",
            lib.link_paths
        );
    }
}
