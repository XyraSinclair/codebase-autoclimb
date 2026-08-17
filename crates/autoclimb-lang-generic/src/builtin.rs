//! Built-in language configurations for the generic plugin system.
//!
//! Each function returns a GenericLangConfig for a specific language.
//! These are ~10-30 lines each — just configuration, no logic.

use std::collections::BTreeSet;

use autoclimb_discovery::zones::ZoneRule;
use autoclimb_types::enums::Zone;

use crate::plugin::{standard_test_zone_rule, GenericLangConfig, PluginDepth};

pub fn rust_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "rust".into(),
        extensions: vec!["rs".into()],
        detect_markers: vec!["Cargo.toml".into(), "Cargo.lock".into()],
        exclude_patterns: vec!["target/".into()],
        tools: vec![],
        treesitter_lang: Some("rust".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["main".into(), "lib".into()],
        barrel_names: BTreeSet::from(["mod.rs".to_string(), "lib.rs".to_string()]),
        large_threshold: 400,
        complexity_threshold: 20,
    }
}

pub fn java_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "java".into(),
        extensions: vec!["java".into()],
        detect_markers: vec![
            "pom.xml".into(),
            "build.gradle".into(),
            "build.gradle.kts".into(),
        ],
        exclude_patterns: vec!["build/".into(), "target/".into(), ".gradle/".into()],
        tools: vec![],
        treesitter_lang: Some("java".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![
            standard_test_zone_rule(),
            ZoneRule {
                zone: Zone::Test,
                patterns: vec!["Test.java".into(), "Tests.java".into()],
            },
        ],
        entry_patterns: vec!["Main".into(), "Application".into(), "App".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 400,
        complexity_threshold: 25,
    }
}

pub fn kotlin_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "kotlin".into(),
        extensions: vec!["kt".into(), "kts".into()],
        detect_markers: vec![
            "build.gradle.kts".into(),
            "build.gradle".into(),
            "settings.gradle.kts".into(),
        ],
        exclude_patterns: vec!["build/".into(), ".gradle/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Shallow,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["Main".into(), "Application".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 350,
        complexity_threshold: 20,
    }
}

pub fn ruby_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "ruby".into(),
        extensions: vec!["rb".into()],
        detect_markers: vec!["Gemfile".into(), "Rakefile".into(), ".ruby-version".into()],
        exclude_patterns: vec!["vendor/".into()],
        tools: vec![],
        treesitter_lang: Some("ruby".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![
            standard_test_zone_rule(),
            ZoneRule {
                zone: Zone::Config,
                patterns: vec!["Gemfile".into(), "Rakefile".into()],
            },
        ],
        entry_patterns: vec!["application".into(), "app".into(), "config".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn php_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "php".into(),
        extensions: vec!["php".into()],
        detect_markers: vec!["composer.json".into(), "composer.lock".into()],
        exclude_patterns: vec!["vendor/".into()],
        tools: vec![],
        treesitter_lang: Some("php".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["index".into(), "app".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn swift_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "swift".into(),
        extensions: vec!["swift".into()],
        detect_markers: vec![
            "Package.swift".into(),
            "*.xcodeproj".into(),
            "*.xcworkspace".into(),
        ],
        exclude_patterns: vec![".build/".into(), "Pods/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Shallow,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["main".into(), "AppDelegate".into(), "App".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 400,
        complexity_threshold: 20,
    }
}

pub fn scala_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "scala".into(),
        extensions: vec!["scala".into()],
        detect_markers: vec!["build.sbt".into(), "project/build.properties".into()],
        exclude_patterns: vec!["target/".into()],
        tools: vec![],
        treesitter_lang: Some("scala".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["Main".into(), "App".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 400,
        complexity_threshold: 20,
    }
}

pub fn bash_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "bash".into(),
        extensions: vec!["sh".into(), "bash".into()],
        detect_markers: vec![],
        exclude_patterns: vec![],
        tools: vec![],
        treesitter_lang: Some("bash".into()),
        depth: PluginDepth::Shallow,
        zone_rules: vec![],
        entry_patterns: vec![],
        barrel_names: BTreeSet::new(),
        large_threshold: 200,
        complexity_threshold: 15,
    }
}

pub fn lua_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "lua".into(),
        extensions: vec!["lua".into()],
        detect_markers: vec![".luarc.json".into(), ".luacheckrc".into()],
        exclude_patterns: vec![],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Shallow,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["init".into(), "main".into()],
        barrel_names: BTreeSet::from(["init.lua".to_string()]),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn perl_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "perl".into(),
        extensions: vec!["pl".into(), "pm".into()],
        detect_markers: vec!["Makefile.PL".into(), "cpanfile".into()],
        exclude_patterns: vec![],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Minimal,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec![],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn haskell_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "haskell".into(),
        extensions: vec!["hs".into()],
        detect_markers: vec![
            "stack.yaml".into(),
            "cabal.project".into(),
            "*.cabal".into(),
        ],
        exclude_patterns: vec![".stack-work/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Shallow,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["Main".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn elixir_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "elixir".into(),
        extensions: vec!["ex".into(), "exs".into()],
        detect_markers: vec!["mix.exs".into()],
        exclude_patterns: vec!["_build/".into(), "deps/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Shallow,
        zone_rules: vec![
            standard_test_zone_rule(),
            ZoneRule {
                zone: Zone::Config,
                patterns: vec!["config/".into()],
            },
        ],
        entry_patterns: vec!["application".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn erlang_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "erlang".into(),
        extensions: vec!["erl".into(), "hrl".into()],
        detect_markers: vec!["rebar.config".into(), "rebar.lock".into()],
        exclude_patterns: vec!["_build/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Minimal,
        zone_rules: vec![],
        entry_patterns: vec![],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn fsharp_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "fsharp".into(),
        extensions: vec!["fs".into(), "fsx".into()],
        detect_markers: vec!["*.fsproj".into()],
        exclude_patterns: vec!["bin/".into(), "obj/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Minimal,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["Program".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn ocaml_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "ocaml".into(),
        extensions: vec!["ml".into(), "mli".into()],
        detect_markers: vec!["dune-project".into(), "*.opam".into()],
        exclude_patterns: vec!["_build/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Minimal,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["main".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn nim_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "nim".into(),
        extensions: vec!["nim".into()],
        detect_markers: vec!["*.nimble".into()],
        exclude_patterns: vec![],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Minimal,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec![],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn zig_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "zig".into(),
        extensions: vec!["zig".into()],
        detect_markers: vec!["build.zig".into()],
        exclude_patterns: vec!["zig-cache/".into(), "zig-out/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Shallow,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["main".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 400,
        complexity_threshold: 20,
    }
}

pub fn powershell_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "powershell".into(),
        extensions: vec!["ps1".into(), "psm1".into()],
        detect_markers: vec![],
        exclude_patterns: vec![],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Minimal,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec![],
        barrel_names: BTreeSet::new(),
        large_threshold: 200,
        complexity_threshold: 15,
    }
}

pub fn r_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "r".into(),
        extensions: vec!["R".into(), "r".into()],
        detect_markers: vec!["DESCRIPTION".into(), ".Rproj".into()],
        exclude_patterns: vec![],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Minimal,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec![],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn gdscript_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "gdscript".into(),
        extensions: vec!["gd".into()],
        detect_markers: vec!["project.godot".into()],
        exclude_patterns: vec![".godot/".into(), "addons/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Standard,
        zone_rules: vec![
            ZoneRule {
                zone: Zone::Test,
                patterns: vec![
                    "/tests/".into(),
                    "/test/".into(),
                    "test_".into(),
                    "_test.gd".into(),
                ],
            },
            ZoneRule {
                zone: Zone::Config,
                patterns: vec!["project.godot".into()],
            },
            ZoneRule {
                zone: Zone::Generated,
                patterns: vec![".godot/".into()],
            },
        ],
        entry_patterns: vec!["main".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn cpp_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "cpp".into(),
        extensions: vec![
            "cpp".into(),
            "cc".into(),
            "cxx".into(),
            "hpp".into(),
            "h".into(),
        ],
        detect_markers: vec![
            "CMakeLists.txt".into(),
            "Makefile".into(),
            "meson.build".into(),
        ],
        exclude_patterns: vec!["build/".into(), "cmake-build-*/".into()],
        tools: vec![],
        treesitter_lang: Some("cpp".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["main".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 500,
        complexity_threshold: 25,
    }
}

pub fn c_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "c".into(),
        extensions: vec!["c".into(), "h".into()],
        detect_markers: vec!["CMakeLists.txt".into(), "Makefile".into()],
        exclude_patterns: vec!["build/".into()],
        tools: vec![],
        treesitter_lang: Some("c".into()),
        depth: PluginDepth::Shallow,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["main".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 500,
        complexity_threshold: 25,
    }
}

pub fn clojure_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "clojure".into(),
        extensions: vec!["clj".into(), "cljs".into(), "cljc".into()],
        detect_markers: vec!["project.clj".into(), "deps.edn".into()],
        exclude_patterns: vec!["target/".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Minimal,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["core".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn typescript_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "typescript".into(),
        extensions: vec!["ts".into(), "tsx".into()],
        detect_markers: vec!["tsconfig.json".into(), "package.json".into()],
        exclude_patterns: vec![
            "node_modules/".into(),
            "dist/".into(),
            "build/".into(),
            ".next/".into(),
        ],
        tools: vec![],
        treesitter_lang: Some("typescript".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![
            standard_test_zone_rule(),
            ZoneRule {
                zone: Zone::Test,
                patterns: vec![
                    "__tests__/".into(),
                    "*.stories.".into(),
                    "*.storybook.".into(),
                ],
            },
        ],
        entry_patterns: vec!["index".into(), "main".into(), "app".into(), "server".into()],
        barrel_names: BTreeSet::from(["index.ts".to_string(), "index.tsx".to_string()]),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn javascript_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "javascript".into(),
        extensions: vec!["js".into(), "jsx".into(), "mjs".into()],
        detect_markers: vec!["package.json".into()],
        exclude_patterns: vec!["node_modules/".into(), "dist/".into(), "build/".into()],
        tools: vec![],
        treesitter_lang: Some("javascript".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![
            standard_test_zone_rule(),
            ZoneRule {
                zone: Zone::Test,
                patterns: vec!["__tests__/".into()],
            },
        ],
        entry_patterns: vec!["index".into(), "main".into(), "app".into(), "server".into()],
        barrel_names: BTreeSet::from(["index.js".to_string()]),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

pub fn go_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "go".into(),
        extensions: vec!["go".into()],
        detect_markers: vec!["go.mod".into(), "go.sum".into()],
        exclude_patterns: vec!["vendor/".into()],
        tools: vec![],
        treesitter_lang: Some("go".into()),
        depth: PluginDepth::Full,
        zone_rules: vec![
            ZoneRule {
                zone: Zone::Test,
                patterns: vec!["_test.go".into()],
            },
            ZoneRule {
                zone: Zone::Config,
                patterns: vec!["go.mod".into(), "go.sum".into()],
            },
        ],
        entry_patterns: vec!["main".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 400,
        complexity_threshold: 20,
    }
}

pub fn csharp_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "csharp".into(),
        extensions: vec!["cs".into()],
        detect_markers: vec!["*.csproj".into(), "*.sln".into()],
        exclude_patterns: vec!["bin/".into(), "obj/".into()],
        tools: vec![],
        treesitter_lang: Some("csharp".into()),
        depth: PluginDepth::Standard,
        zone_rules: vec![standard_test_zone_rule()],
        entry_patterns: vec!["Program".into(), "Startup".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 400,
        complexity_threshold: 25,
    }
}

pub fn dart_config() -> GenericLangConfig {
    GenericLangConfig {
        name: "dart".into(),
        extensions: vec!["dart".into()],
        detect_markers: vec!["pubspec.yaml".into(), "pubspec.lock".into()],
        exclude_patterns: vec![".dart_tool/".into(), "build/".into(), ".packages".into()],
        tools: vec![],
        treesitter_lang: None,
        depth: PluginDepth::Standard,
        zone_rules: vec![
            ZoneRule {
                zone: Zone::Test,
                patterns: vec![
                    "_test.dart".into(),
                    "/test/".into(),
                    "/integration_test/".into(),
                ],
            },
            ZoneRule {
                zone: Zone::Config,
                patterns: vec!["pubspec.yaml".into(), "analysis_options.yaml".into()],
            },
            ZoneRule {
                zone: Zone::Generated,
                patterns: vec![".g.dart".into(), ".freezed.dart".into()],
            },
        ],
        entry_patterns: vec!["main".into()],
        barrel_names: BTreeSet::new(),
        large_threshold: 300,
        complexity_threshold: 20,
    }
}

/// All built-in generic language configs.
pub fn all_builtin_configs() -> Vec<GenericLangConfig> {
    vec![
        typescript_config(),
        javascript_config(),
        go_config(),
        csharp_config(),
        dart_config(),
        rust_config(),
        java_config(),
        kotlin_config(),
        ruby_config(),
        php_config(),
        swift_config(),
        scala_config(),
        bash_config(),
        lua_config(),
        perl_config(),
        haskell_config(),
        elixir_config(),
        erlang_config(),
        fsharp_config(),
        ocaml_config(),
        nim_config(),
        zig_config(),
        powershell_config(),
        r_config(),
        gdscript_config(),
        cpp_config(),
        c_config(),
        clojure_config(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_configs_have_names() {
        for config in all_builtin_configs() {
            assert!(!config.name.is_empty());
            assert!(!config.extensions.is_empty());
        }
    }

    #[test]
    fn config_count() {
        assert!(all_builtin_configs().len() >= 23);
    }
}
