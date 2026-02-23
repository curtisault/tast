use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ir::{IrGraph, lower};
use crate::parser::ast;
use crate::parser::parse::parse;

/// A resolved import: the name used in the importing file and the IR graphs it contains.
#[derive(Debug, Clone)]
pub struct ResolvedImport {
    pub name: String,
    pub graphs: Vec<IrGraph>,
}

/// Resolves imports from .tast files with caching and circular import detection.
pub struct ImportResolver {
    base_dir: PathBuf,
    loaded: HashMap<PathBuf, Vec<IrGraph>>,
    in_progress: HashSet<PathBuf>,
}

impl ImportResolver {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_owned(),
            loaded: HashMap::new(),
            in_progress: HashSet::new(),
        }
    }

    /// Resolve a list of AST imports into their IR graphs.
    ///
    /// # Errors
    ///
    /// Returns an error if a file cannot be read, parsed, or if a circular import is detected.
    pub fn resolve_imports(
        &mut self,
        imports: &[ast::Import],
    ) -> Result<Vec<ResolvedImport>, String> {
        let mut resolved = Vec::new();

        for import in imports {
            let path = self.base_dir.join(&import.path);
            let canonical = path
                .canonicalize()
                .map_err(|e| format!("failed to resolve import '{}': {e}", import.path))?;

            if self.in_progress.contains(&canonical) {
                return Err(format!("circular import detected: {}", import.path));
            }

            let graphs = if let Some(cached) = self.loaded.get(&canonical) {
                cached.clone()
            } else {
                self.in_progress.insert(canonical.clone());

                let input = std::fs::read_to_string(&canonical)
                    .map_err(|e| format!("failed to read import '{}': {e}", import.path))?;

                let ast_graphs = parse(&input)
                    .map_err(|e| format!("error in imported file '{}': {e}", import.path))?;

                let mut ir_graphs = Vec::new();
                for g in &ast_graphs {
                    let ir = lower(g)
                        .map_err(|e| format!("error in imported file '{}': {e}", import.path))?;
                    ir_graphs.push(ir);
                }

                self.in_progress.remove(&canonical);
                self.loaded.insert(canonical, ir_graphs.clone());
                ir_graphs
            };

            resolved.push(ResolvedImport {
                name: import.name.clone(),
                graphs,
            });
        }

        Ok(resolved)
    }
}

/// Resolve cross-graph edges in a graph that references imported graphs.
///
/// Cross-graph references use dotted notation: `Auth.Login -> PlaceOrder`
/// This function finds edges with dotted `from` or `to` names, looks up the
/// referenced node in the imported graphs, copies it into the current graph,
/// and rewires the edge to point to the local copy.
///
/// # Errors
///
/// Returns an error if a referenced graph or node cannot be found.
pub fn resolve_cross_graph_edges(
    graph: &mut IrGraph,
    imports: &[ResolvedImport],
) -> Result<(), String> {
    let import_map: HashMap<&str, &[IrGraph]> = imports
        .iter()
        .map(|ri| (ri.name.as_str(), ri.graphs.as_slice()))
        .collect();

    // Collect edges that need cross-graph resolution
    let edges_to_resolve: Vec<(usize, String, String, bool)> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(i, edge)| {
            if edge.from.contains('.') {
                Some((i, edge.from.clone(), edge.to.clone(), true))
            } else if edge.to.contains('.') {
                Some((i, edge.from.clone(), edge.to.clone(), false))
            } else {
                None
            }
        })
        .collect();

    for (edge_idx, from, to, is_from_cross) in edges_to_resolve {
        let dotted = if is_from_cross { &from } else { &to };
        let (graph_name, node_name) = dotted
            .split_once('.')
            .ok_or_else(|| format!("invalid cross-graph reference: '{dotted}'"))?;

        let imported_graphs = import_map
            .get(graph_name)
            .ok_or_else(|| format!("unknown imported graph '{graph_name}'"))?;

        // Find the node in the imported graphs
        let imported_node = imported_graphs
            .iter()
            .flat_map(|g| g.nodes.iter())
            .find(|n| n.name == node_name)
            .ok_or_else(|| {
                format!("unknown node '{node_name}' in imported graph '{graph_name}'")
            })?;

        // Check if we've already copied this node
        let local_name = format!("{graph_name}.{node_name}");
        let local_idx = if let Some(idx) = graph.nodes.iter().position(|n| n.name == local_name) {
            idx
        } else {
            let mut copied = imported_node.clone();
            copied.name = local_name.clone();
            let idx = graph.nodes.len();
            graph.nodes.push(copied);
            idx
        };

        // Update the edge
        let edge = &mut graph.edges[edge_idx];
        if is_from_cross {
            edge.from = local_name;
            edge.from_index = local_idx;
        } else {
            edge.to = local_name;
            edge.to_index = local_idx;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::span::Span;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
    }

    #[test]
    fn resolves_import_relative_path() {
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports = vec![ast::Import {
            name: "Auth".into(),
            path: "./importable_auth.tast".into(),
            span: Span::default(),
        }];
        let resolved = resolver.resolve_imports(&imports).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "Auth");
        assert_eq!(resolved[0].graphs[0].name, "Auth");
    }

    #[test]
    fn resolves_import_caches_files() {
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports = vec![ast::Import {
            name: "Auth".into(),
            path: "./importable_auth.tast".into(),
            span: Span::default(),
        }];
        // Resolve twice — second should use cache
        let _ = resolver.resolve_imports(&imports).unwrap();
        let resolved = resolver.resolve_imports(&imports).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].graphs.len(), 1);
    }

    #[test]
    fn resolve_missing_file_errors() {
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports = vec![ast::Import {
            name: "Missing".into(),
            path: "./nonexistent.tast".into(),
            span: Span::default(),
        }];
        let result = resolver.resolve_imports(&imports);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to resolve import"));
    }

    #[test]
    fn resolve_invalid_file_errors() {
        // Create a temp file with invalid syntax — use the existing invalid_syntax fixture
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports = vec![ast::Import {
            name: "Bad".into(),
            path: "./invalid_syntax.tast".into(),
            span: Span::default(),
        }];
        let result = resolver.resolve_imports(&imports);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("error in imported file"));
    }

    #[test]
    fn resolve_circular_import_errors() {
        // Simulate circular import by marking a file as in-progress
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let canonical = fixtures_dir()
            .join("importable_auth.tast")
            .canonicalize()
            .unwrap();
        resolver.in_progress.insert(canonical);
        let imports = vec![ast::Import {
            name: "Auth".into(),
            path: "./importable_auth.tast".into(),
            span: Span::default(),
        }];
        let result = resolver.resolve_imports(&imports);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("circular import"));
    }

    #[test]
    fn resolve_returns_named_graphs() {
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports = vec![ast::Import {
            name: "AuthModule".into(),
            path: "./importable_auth.tast".into(),
            span: Span::default(),
        }];
        let resolved = resolver.resolve_imports(&imports).unwrap();
        assert_eq!(resolved[0].name, "AuthModule");
        // But the graph inside is still named "Auth"
        assert_eq!(resolved[0].graphs[0].name, "Auth");
    }

    // ── Cross-graph edge resolution ────────────────────────

    #[test]
    fn resolves_cross_graph_edge() {
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports_ast = vec![ast::Import {
            name: "Auth".into(),
            path: "./importable_auth.tast".into(),
            span: Span::default(),
        }];
        let resolved = resolver.resolve_imports(&imports_ast).unwrap();

        let mut graph = IrGraph {
            name: "OrderFlow".into(),
            nodes: vec![crate::ir::IrNode {
                name: "PlaceOrder".into(),
                description: None,
                steps: vec![],
                tags: vec![],
                requires: vec![],
                span: Span::default(),
            }],
            edges: vec![crate::ir::IrEdge {
                from: "Auth.Login".into(),
                to: "PlaceOrder".into(),
                from_index: 0, // will be resolved
                to_index: 0,
                passes: vec!["auth_token".into()],
                description: None,
                span: Span::default(),
            }],
            span: Span::default(),
        };

        resolve_cross_graph_edges(&mut graph, &resolved).unwrap();

        // Should have added a node for Auth.Login
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[1].name, "Auth.Login");
        // Edge should now reference the copied node
        assert_eq!(graph.edges[0].from, "Auth.Login");
        assert_eq!(graph.edges[0].from_index, 1);
    }

    #[test]
    fn cross_graph_unknown_graph_errors() {
        let resolved = vec![];
        let mut graph = IrGraph {
            name: "G".into(),
            nodes: vec![],
            edges: vec![crate::ir::IrEdge {
                from: "Unknown.Node".into(),
                to: "X".into(),
                from_index: 0,
                to_index: 0,
                passes: vec![],
                description: None,
                span: Span::default(),
            }],
            span: Span::default(),
        };
        let result = resolve_cross_graph_edges(&mut graph, &resolved);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown imported graph"));
    }

    #[test]
    fn cross_graph_unknown_node_errors() {
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports_ast = vec![ast::Import {
            name: "Auth".into(),
            path: "./importable_auth.tast".into(),
            span: Span::default(),
        }];
        let resolved = resolver.resolve_imports(&imports_ast).unwrap();

        let mut graph = IrGraph {
            name: "G".into(),
            nodes: vec![],
            edges: vec![crate::ir::IrEdge {
                from: "Auth.NonExistent".into(),
                to: "X".into(),
                from_index: 0,
                to_index: 0,
                passes: vec![],
                description: None,
                span: Span::default(),
            }],
            span: Span::default(),
        };
        let result = resolve_cross_graph_edges(&mut graph, &resolved);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown node 'NonExistent'"));
    }

    #[test]
    fn cross_graph_passes_data() {
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports_ast = vec![ast::Import {
            name: "Auth".into(),
            path: "./importable_auth.tast".into(),
            span: Span::default(),
        }];
        let resolved = resolver.resolve_imports(&imports_ast).unwrap();

        let mut graph = IrGraph {
            name: "G".into(),
            nodes: vec![crate::ir::IrNode {
                name: "PlaceOrder".into(),
                description: None,
                steps: vec![],
                tags: vec![],
                requires: vec![],
                span: Span::default(),
            }],
            edges: vec![crate::ir::IrEdge {
                from: "Auth.Login".into(),
                to: "PlaceOrder".into(),
                from_index: 0,
                to_index: 0,
                passes: vec!["token".into()],
                description: None,
                span: Span::default(),
            }],
            span: Span::default(),
        };

        resolve_cross_graph_edges(&mut graph, &resolved).unwrap();
        assert_eq!(graph.edges[0].passes, vec!["token"]);
    }

    #[test]
    fn cross_graph_preserves_local_edges() {
        let mut resolver = ImportResolver::new(&fixtures_dir());
        let imports_ast = vec![ast::Import {
            name: "Auth".into(),
            path: "./importable_auth.tast".into(),
            span: Span::default(),
        }];
        let resolved = resolver.resolve_imports(&imports_ast).unwrap();

        let mut graph = IrGraph {
            name: "G".into(),
            nodes: vec![
                crate::ir::IrNode {
                    name: "A".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    span: Span::default(),
                },
                crate::ir::IrNode {
                    name: "B".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    span: Span::default(),
                },
            ],
            edges: vec![
                crate::ir::IrEdge {
                    from: "A".into(),
                    to: "B".into(),
                    from_index: 0,
                    to_index: 1,
                    passes: vec![],
                    description: None,
                    span: Span::default(),
                },
                crate::ir::IrEdge {
                    from: "Auth.Login".into(),
                    to: "A".into(),
                    from_index: 0, // will be resolved
                    to_index: 0,
                    passes: vec![],
                    description: None,
                    span: Span::default(),
                },
            ],
            span: Span::default(),
        };

        resolve_cross_graph_edges(&mut graph, &resolved).unwrap();
        // Local edge A -> B should be untouched
        assert_eq!(graph.edges[0].from, "A");
        assert_eq!(graph.edges[0].to, "B");
        // Cross-graph edge should be resolved
        assert_eq!(graph.edges[1].from, "Auth.Login");
    }
}
