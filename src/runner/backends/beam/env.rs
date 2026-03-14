use std::path::{Path, PathBuf};

/// The type of BEAM project detected at a given path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeamProjectType {
    /// Elixir project with `mix.exs`.
    Elixir,
    /// Gleam project with `gleam.toml`.
    Gleam,
    /// Erlang project with `rebar.config`.
    Erlang,
    /// Erlang project with `erlang.mk` (secondary).
    ErlangMk,
    /// Unknown or not a BEAM project.
    Unknown,
}

/// Detect the BEAM project type at the given path.
///
/// Priority: Gleam > Elixir > Erlang (rebar) > Erlang (erlang.mk).
/// Gleam is checked first because `gleam build` generates a `rebar.config`.
pub fn detect_beam_project(path: &Path) -> BeamProjectType {
    if path.join("gleam.toml").exists() {
        BeamProjectType::Gleam
    } else if path.join("mix.exs").exists() {
        BeamProjectType::Elixir
    } else if path.join("rebar.config").exists() {
        BeamProjectType::Erlang
    } else if path.join("erlang.mk").exists() {
        BeamProjectType::ErlangMk
    } else {
        BeamProjectType::Unknown
    }
}

/// Find the test directory for a BEAM project.
pub fn test_directory(path: &Path, project_type: &BeamProjectType) -> PathBuf {
    match project_type {
        BeamProjectType::Elixir
        | BeamProjectType::Gleam
        | BeamProjectType::Erlang
        | BeamProjectType::ErlangMk => path.join("test"),
        BeamProjectType::Unknown => path.join("test"),
    }
}

/// Find the build/output directory for a BEAM project.
pub fn build_directory(path: &Path, project_type: &BeamProjectType) -> PathBuf {
    match project_type {
        BeamProjectType::Elixir => path.join("_build").join("test"),
        BeamProjectType::Gleam => path.join("build"),
        BeamProjectType::Erlang | BeamProjectType::ErlangMk => path.join("_build").join("test"),
        BeamProjectType::Unknown => path.join("_build"),
    }
}

/// Generate a unique module name for a TAST-generated test file.
///
/// Combines "TastGen" with the plan name (sanitized).
/// Example: `"UserAuthentication"` → `"TastGenUserAuthentication"`
pub fn generated_module_name(plan_name: &str) -> String {
    let sanitized: String = plan_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    format!("TastGen{sanitized}")
}

/// Convert a CamelCase node name to a snake_case BEAM name.
///
/// BEAM atoms and function names use snake_case.
/// `"RegisterUser"` → `"register_user"`
/// `"AccessDashboard"` → `"access_dashboard"`
/// `"already_snake"` → `"already_snake"`
pub fn to_beam_name(node_name: &str) -> String {
    let mut result = String::with_capacity(node_name.len() + 4);
    let mut prev_was_upper = false;

    for (i, ch) in node_name.chars().enumerate() {
        if ch.is_uppercase() {
            // Insert underscore before uppercase if:
            // - not the first character
            // - previous char was not uppercase (avoids "HTTP" → "h_t_t_p")
            //   OR next char is lowercase (handles "HTTPServer" → "http_server")
            if i > 0 {
                let next_is_lower = node_name
                    .chars()
                    .nth(i + 1)
                    .is_some_and(|c| c.is_lowercase());
                if !prev_was_upper || next_is_lower {
                    result.push('_');
                }
            }
            result.push(ch.to_lowercase().next().unwrap_or(ch));
            prev_was_upper = true;
        } else {
            result.push(ch);
            prev_was_upper = false;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detect_beam_project_elixir() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("mix.exs"), "").unwrap();
        assert_eq!(detect_beam_project(dir.path()), BeamProjectType::Elixir);
    }

    #[test]
    fn detect_beam_project_gleam() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("gleam.toml"), "").unwrap();
        assert_eq!(detect_beam_project(dir.path()), BeamProjectType::Gleam);
    }

    #[test]
    fn detect_beam_project_erlang_rebar() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("rebar.config"), "").unwrap();
        assert_eq!(detect_beam_project(dir.path()), BeamProjectType::Erlang);
    }

    #[test]
    fn detect_beam_project_erlang_mk() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("erlang.mk"), "").unwrap();
        assert_eq!(detect_beam_project(dir.path()), BeamProjectType::ErlangMk);
    }

    #[test]
    fn detect_beam_project_unknown() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_beam_project(dir.path()), BeamProjectType::Unknown);
    }

    #[test]
    fn detect_beam_project_gleam_over_erlang() {
        // Gleam projects generate rebar.config — Gleam should win.
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("gleam.toml"), "").unwrap();
        std::fs::write(dir.path().join("rebar.config"), "").unwrap();
        assert_eq!(detect_beam_project(dir.path()), BeamProjectType::Gleam);
    }

    #[test]
    fn detect_beam_project_elixir_over_erlang() {
        // Elixir projects can have rebar deps.
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("mix.exs"), "").unwrap();
        std::fs::write(dir.path().join("rebar.config"), "").unwrap();
        assert_eq!(detect_beam_project(dir.path()), BeamProjectType::Elixir);
    }

    #[test]
    fn test_directory_elixir() {
        let base = Path::new("/home/user/project");
        assert_eq!(
            test_directory(base, &BeamProjectType::Elixir),
            PathBuf::from("/home/user/project/test")
        );
    }

    #[test]
    fn test_directory_gleam() {
        let base = Path::new("/home/user/project");
        assert_eq!(
            test_directory(base, &BeamProjectType::Gleam),
            PathBuf::from("/home/user/project/test")
        );
    }

    #[test]
    fn test_directory_erlang() {
        let base = Path::new("/home/user/project");
        assert_eq!(
            test_directory(base, &BeamProjectType::Erlang),
            PathBuf::from("/home/user/project/test")
        );
    }

    #[test]
    fn build_directory_elixir() {
        let base = Path::new("/home/user/project");
        assert_eq!(
            build_directory(base, &BeamProjectType::Elixir),
            PathBuf::from("/home/user/project/_build/test")
        );
    }

    #[test]
    fn build_directory_gleam() {
        let base = Path::new("/home/user/project");
        assert_eq!(
            build_directory(base, &BeamProjectType::Gleam),
            PathBuf::from("/home/user/project/build")
        );
    }

    #[test]
    fn build_directory_erlang() {
        let base = Path::new("/home/user/project");
        assert_eq!(
            build_directory(base, &BeamProjectType::Erlang),
            PathBuf::from("/home/user/project/_build/test")
        );
    }

    #[test]
    fn generated_module_name_simple() {
        assert_eq!(
            generated_module_name("UserAuthentication"),
            "TastGenUserAuthentication"
        );
    }

    #[test]
    fn generated_module_name_with_spaces() {
        assert_eq!(
            generated_module_name("User Auth Flow"),
            "TastGenUserAuthFlow"
        );
    }

    #[test]
    fn generated_module_name_with_special_chars() {
        assert_eq!(generated_module_name("Auth-Flow.v2"), "TastGenAuthFlowv2");
    }

    #[test]
    fn to_beam_name_camel_case() {
        assert_eq!(to_beam_name("RegisterUser"), "register_user");
    }

    #[test]
    fn to_beam_name_multi_word() {
        assert_eq!(to_beam_name("AccessDashboard"), "access_dashboard");
    }

    #[test]
    fn to_beam_name_already_snake() {
        assert_eq!(to_beam_name("already_snake"), "already_snake");
    }

    #[test]
    fn to_beam_name_single_word() {
        assert_eq!(to_beam_name("Login"), "login");
    }

    #[test]
    fn to_beam_name_acronym() {
        assert_eq!(to_beam_name("HTTPServer"), "http_server");
    }

    #[test]
    fn to_beam_name_consecutive_uppercase() {
        assert_eq!(to_beam_name("APIEndpoint"), "api_endpoint");
    }
}
