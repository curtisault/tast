use std::path::{Path, PathBuf};

/// A discovered step binding function in user code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredBinding {
    /// The function name, e.g. "given_a_user_with_email".
    pub function_name: String,
    /// Path to the file where the function was found.
    pub file_path: PathBuf,
    /// Whether the function accepts a `HashMap` parameter.
    pub accepts_data: bool,
    /// Whether the function returns a `HashMap`.
    pub returns_data: bool,
    /// The step type prefix: "given", "when", or "then".
    pub step_type: String,
}

/// Convert step text to the expected function name.
///
/// Rules:
/// - Prefix with step type (`given_`, `when_`, `then_`)
/// - `and`/`but` use the provided parent type instead
/// - Lowercase everything
/// - Strip quoted strings (both single and double)
/// - Replace non-alphanumeric chars with `_`
/// - Collapse consecutive underscores
/// - Strip leading/trailing underscores from the text portion
/// - Truncate at 80 characters
pub fn step_to_function_name(step_type: &str, text: &str, parent_type: Option<&str>) -> String {
    let prefix = match step_type {
        "and" | "but" => parent_type.unwrap_or("given"),
        other => other,
    };

    // Strip quoted strings
    let mut cleaned = String::with_capacity(text.len());
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    for ch in text.chars() {
        match ch {
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            _ if !in_double_quote && !in_single_quote => cleaned.push(ch),
            _ => {}
        }
    }

    // Lowercase and replace non-alphanumeric with underscore
    let lowered = cleaned.to_lowercase();
    let mut result = String::with_capacity(prefix.len() + 1 + lowered.len());
    result.push_str(prefix);
    result.push('_');

    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch);
        } else {
            result.push('_');
        }
    }

    // Collapse consecutive underscores
    let collapsed = collapse_underscores(&result);

    // Strip trailing underscore
    let trimmed = collapsed.trim_end_matches('_');

    // Truncate at 80 chars
    if trimmed.len() > 80 {
        trimmed[..80].to_string()
    } else {
        trimmed.to_string()
    }
}

fn collapse_underscores(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for ch in s.chars() {
        if ch == '_' {
            if !prev_underscore {
                result.push('_');
            }
            prev_underscore = true;
        } else {
            result.push(ch);
            prev_underscore = false;
        }
    }
    result
}

/// Scan a single `.rs` file for step binding functions.
///
/// Looks for lines matching `fn given_*`, `fn when_*`, `fn then_*`.
/// Detects `HashMap` in parameter list and return type.
pub fn scan_file(path: &Path) -> Vec<DiscoveredBinding> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    scan_content(&content, path)
}

/// Scan content string for step binding functions (testable without filesystem).
pub(crate) fn scan_content(content: &str, path: &Path) -> Vec<DiscoveredBinding> {
    let mut bindings = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Look for `fn <name>` or `pub fn <name>`
        let fn_start = trimmed
            .strip_prefix("pub fn ")
            .or_else(|| trimmed.strip_prefix("fn "));

        let Some(rest) = fn_start else {
            continue;
        };

        // Extract function name (up to `(`)
        let Some(paren_pos) = rest.find('(') else {
            continue;
        };
        let fn_name = rest[..paren_pos].trim();

        // Check if it's a step function
        let step_type = if fn_name.starts_with("given_") {
            "given"
        } else if fn_name.starts_with("when_") {
            "when"
        } else if fn_name.starts_with("then_") {
            "then"
        } else {
            continue;
        };

        // Check for HashMap in parameter and return type
        let after_name = &rest[paren_pos..];
        let accepts_data = after_name
            .split(')')
            .next()
            .is_some_and(|params| params.contains("HashMap"));
        let returns_data = after_name
            .split(')')
            .nth(1)
            .is_some_and(|ret| ret.contains("HashMap"));

        bindings.push(DiscoveredBinding {
            function_name: fn_name.to_string(),
            file_path: path.to_path_buf(),
            accepts_data,
            returns_data,
            step_type: step_type.to_string(),
        });
    }

    bindings
}

/// Discover all step bindings from the given search paths.
///
/// Each path can be a file or directory. Directories are scanned
/// recursively for `.rs` files.
pub fn discover_bindings(search_paths: &[PathBuf]) -> Vec<DiscoveredBinding> {
    let mut all = Vec::new();
    for path in search_paths {
        if path.is_file() {
            all.extend(scan_file(path));
        } else if path.is_dir()
            && let Ok(entries) = std::fs::read_dir(path)
        {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|e| e == "rs") {
                    all.extend(scan_file(&p));
                }
            }
        }
    }
    all
}

/// Return default binding search paths for a project directory.
///
/// Defaults: `tests/tast_steps.rs`, `tests/tast_steps/`, `src/tast_steps.rs`.
/// If `config_paths` is non-empty, use those instead.
pub fn binding_search_paths(project_dir: &Path, config_paths: &[PathBuf]) -> Vec<PathBuf> {
    if !config_paths.is_empty() {
        return config_paths
            .iter()
            .map(|p| {
                if p.is_absolute() {
                    p.clone()
                } else {
                    project_dir.join(p)
                }
            })
            .collect();
    }

    vec![
        project_dir.join("tests/tast_steps.rs"),
        project_dir.join("tests/tast_steps"),
        project_dir.join("src/tast_steps.rs"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- step_to_function_name tests --

    #[test]
    fn function_name_given_step() {
        let name = step_to_function_name("given", "a registered user", None);
        assert_eq!(name, "given_a_registered_user");
    }

    #[test]
    fn function_name_when_step() {
        let name = step_to_function_name("when", "the user submits the form", None);
        assert_eq!(name, "when_the_user_submits_the_form");
    }

    #[test]
    fn function_name_then_step() {
        let name = step_to_function_name("then", "the account is created", None);
        assert_eq!(name, "then_the_account_is_created");
    }

    #[test]
    fn function_name_and_inherits_parent() {
        let name = step_to_function_name("and", "the session is active", Some("given"));
        assert_eq!(name, "given_the_session_is_active");
    }

    #[test]
    fn function_name_but_inherits_parent() {
        let name = step_to_function_name("but", "no errors occur", Some("then"));
        assert_eq!(name, "then_no_errors_occur");
    }

    #[test]
    fn function_name_and_defaults_to_given() {
        let name = step_to_function_name("and", "something", None);
        assert_eq!(name, "given_something");
    }

    #[test]
    fn function_name_strips_quoted_strings() {
        let name = step_to_function_name("given", "a user with email \"test@example.com\"", None);
        assert_eq!(name, "given_a_user_with_email");
    }

    #[test]
    fn function_name_collapses_underscores() {
        let name = step_to_function_name("when", "the  user   acts", None);
        assert_eq!(name, "when_the_user_acts");
    }

    #[test]
    fn function_name_truncates_at_80() {
        let long_text = "a".to_string() + &" word".repeat(20);
        let name = step_to_function_name("given", &long_text, None);
        assert!(name.len() <= 80);
    }

    // -- scan_content tests --

    #[test]
    fn scan_finds_given_function() {
        let content = "pub fn given_a_registered_user() {\n    // setup\n}\n";
        let bindings = scan_content(content, Path::new("test.rs"));
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].function_name, "given_a_registered_user");
        assert_eq!(bindings[0].step_type, "given");
        assert!(!bindings[0].accepts_data);
        assert!(!bindings[0].returns_data);
    }

    #[test]
    fn scan_finds_when_and_then_functions() {
        let content = "\
fn when_the_user_logs_in() {}
fn then_a_token_is_returned() {}
";
        let bindings = scan_content(content, Path::new("test.rs"));
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].step_type, "when");
        assert_eq!(bindings[1].step_type, "then");
    }

    #[test]
    fn scan_detects_hashmap_parameter() {
        let content = "pub fn given_a_user(data: &HashMap<String, String>) {}\n";
        let bindings = scan_content(content, Path::new("test.rs"));
        assert_eq!(bindings.len(), 1);
        assert!(bindings[0].accepts_data);
        assert!(!bindings[0].returns_data);
    }

    #[test]
    fn scan_detects_hashmap_return() {
        let content =
            "pub fn when_submit(data: &HashMap<String, String>) -> HashMap<String, String> {}\n";
        let bindings = scan_content(content, Path::new("test.rs"));
        assert_eq!(bindings.len(), 1);
        assert!(bindings[0].accepts_data);
        assert!(bindings[0].returns_data);
    }

    #[test]
    fn scan_ignores_non_step_functions() {
        let content = "\
fn helper_function() {}
pub fn setup() {}
fn given_a_user() {}
fn not_a_step() {}
";
        let bindings = scan_content(content, Path::new("test.rs"));
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].function_name, "given_a_user");
    }

    // -- binding_search_paths tests --

    #[test]
    fn search_paths_defaults() {
        let paths = binding_search_paths(Path::new("/project"), &[]);
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], PathBuf::from("/project/tests/tast_steps.rs"));
        assert_eq!(paths[1], PathBuf::from("/project/tests/tast_steps"));
        assert_eq!(paths[2], PathBuf::from("/project/src/tast_steps.rs"));
    }

    #[test]
    fn search_paths_config_override() {
        let config = vec![PathBuf::from("custom/steps.rs")];
        let paths = binding_search_paths(Path::new("/project"), &config);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], PathBuf::from("/project/custom/steps.rs"));
    }

    #[test]
    fn search_paths_config_absolute() {
        let config = vec![PathBuf::from("/absolute/steps.rs")];
        let paths = binding_search_paths(Path::new("/project"), &config);
        assert_eq!(paths[0], PathBuf::from("/absolute/steps.rs"));
    }
}
