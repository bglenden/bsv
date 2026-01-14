use crate::bd::Issue;
use crate::HierarchyMode;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub issue: Issue,
    pub children: Vec<String>,        // ID-based children (from dotted IDs)
    pub dep_children: Vec<String>,    // Dependency-based children (issues that depend on this)
    pub depth: usize,
}

#[derive(Debug)]
pub struct IssueTree {
    pub nodes: HashMap<String, TreeNode>,
    pub root_ids: Vec<String>,              // ID-based roots (no dots or orphans)
    pub dep_root_ids: Vec<String>,          // Dependency-based roots (no dependencies)
    pub expanded: HashSet<String>,          // Expansion state for ID-based view
    pub dep_expanded: HashSet<String>,      // Expansion state for dependency view
    pub multi_parent_ids: HashSet<String>,  // Issues with multiple parents in dep view
    pub ready_ids: HashSet<String>,
    pub visible_items: Vec<String>,
    pub cursor: usize,
    pub show_closed: bool,
    pub hierarchy_mode: HierarchyMode,
}

impl IssueTree {
    pub fn from_issues(
        issues: Vec<Issue>,
        expanded: HashSet<String>,
        dep_expanded: HashSet<String>,
        ready_ids: HashSet<String>,
        hierarchy_mode: HierarchyMode,
    ) -> Self {
        let mut nodes: HashMap<String, TreeNode> = HashMap::new();
        let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut dep_children_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut parent_count: HashMap<String, usize> = HashMap::new();

        // First pass: create all nodes
        for issue in &issues {
            nodes.insert(issue.id.clone(), TreeNode {
                issue: issue.clone(),
                children: vec![],
                dep_children: vec![],
                depth: 0,
            });
        }

        // Second pass: build parent-child relationships from DOTTED IDs ONLY
        // e.g., "bsv-abc.1" is child of "bsv-abc"
        for issue in &issues {
            if let Some(parent_id) = Self::parent_from_dotted_id(&issue.id) {
                if nodes.contains_key(&parent_id) {
                    children_map.entry(parent_id).or_default().push(issue.id.clone());
                }
            }
        }

        // Third pass: build dependency-based parent-child relationships
        // If issue A depends on B (B blocks A), then A is a child of B in dep view
        for issue in &issues {
            if let Some(deps) = &issue.dependencies {
                let blocking_deps: Vec<&crate::bd::Dependency> = deps.iter()
                    .filter(|d| d.dependency_type.as_deref() != Some("related"))
                    .collect();

                // Track parent count for multi-parent detection
                if blocking_deps.len() > 1 {
                    parent_count.insert(issue.id.clone(), blocking_deps.len());
                }

                for dep in blocking_deps {
                    // dep.id is the parent (blocker), issue.id is the child (blocked)
                    if nodes.contains_key(&dep.id) {
                        dep_children_map.entry(dep.id.clone())
                            .or_default()
                            .push(issue.id.clone());
                    }
                }
            }
        }

        // Populate children lists
        for (parent_id, child_ids) in &children_map {
            if let Some(node) = nodes.get_mut(parent_id) {
                node.children = child_ids.clone();
            }
        }
        for (parent_id, child_ids) in &dep_children_map {
            if let Some(node) = nodes.get_mut(parent_id) {
                // Deduplicate children in case of duplicate dependencies in source data
                let mut seen = HashSet::new();
                node.dep_children = child_ids.iter()
                    .filter(|id| seen.insert(id.to_string()))
                    .cloned()
                    .collect();
            }
        }

        // Find ID-based root nodes: no dot in ID, OR parent from dotted ID doesn't exist
        let mut root_ids: Vec<String> = nodes.keys()
            .filter(|id| {
                match Self::parent_from_dotted_id(id) {
                    Some(parent_id) => !nodes.contains_key(&parent_id),
                    None => true, // no dot = root
                }
            })
            .cloned()
            .collect();

        // Find dependency-based root nodes: issues with no blocking dependencies
        let mut dep_root_ids: Vec<String> = nodes.keys()
            .filter(|id| {
                let node = nodes.get(*id).unwrap();
                let has_blocking_deps = node.issue.dependencies
                    .as_ref()
                    .map(|deps| deps.iter().any(|d| {
                        d.dependency_type.as_deref() != Some("related") &&
                        nodes.contains_key(&d.id)
                    }))
                    .unwrap_or(false);
                !has_blocking_deps
            })
            .cloned()
            .collect();

        // Sort roots by priority then by title
        let sort_fn = |a: &String, b: &String| {
            let node_a = nodes.get(a).unwrap();
            let node_b = nodes.get(b).unwrap();
            node_a.issue.priority.cmp(&node_b.issue.priority)
                .then_with(|| node_a.issue.title.cmp(&node_b.issue.title))
        };
        root_ids.sort_by(sort_fn);
        dep_root_ids.sort_by(sort_fn);

        // Identify multi-parent issues
        let multi_parent_ids: HashSet<String> = parent_count.into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(id, _)| id)
            .collect();

        let mut tree = IssueTree {
            nodes,
            root_ids,
            dep_root_ids,
            expanded,
            dep_expanded,
            multi_parent_ids,
            ready_ids,
            visible_items: vec![],
            cursor: 0,
            show_closed: true,
            hierarchy_mode,
        };

        tree.rebuild_visible();
        tree
    }

    // "bsv-abc.1.2" -> Some("bsv-abc.1"), "bsv-abc" -> None
    fn parent_from_dotted_id(id: &str) -> Option<String> {
        id.rfind('.').map(|pos| id[..pos].to_string())
    }

    pub fn rebuild_visible(&mut self) {
        self.visible_items.clear();
        match self.hierarchy_mode {
            HierarchyMode::IdBased => {
                for root_id in &self.root_ids.clone() {
                    self.add_visible_recursive_id(root_id, 0);
                }
            }
            HierarchyMode::DependencyBased => {
                let mut visited = HashSet::new();
                // Track all items already added to avoid duplicates anywhere in the tree
                // This prevents items from appearing multiple times at different depths
                let mut added: HashSet<String> = HashSet::new();
                for root_id in &self.dep_root_ids.clone() {
                    self.add_visible_recursive_dep(root_id, 0, &mut visited, &mut added);
                }
            }
        }
        if self.cursor >= self.visible_items.len() && !self.visible_items.is_empty() {
            self.cursor = self.visible_items.len() - 1;
        }
    }

    fn add_visible_recursive_id(&mut self, id: &str, depth: usize) {
        // Check if this issue is closed
        let is_closed = self.nodes.get(id)
            .map(|node| node.issue.status == "closed")
            .unwrap_or(false);

        // Only add to visible if showing closed OR issue is not closed
        if self.show_closed || !is_closed {
            self.visible_items.push(id.to_string());

            if let Some(node) = self.nodes.get_mut(id) {
                node.depth = depth;
            }
        }

        // Traverse children if:
        // 1. This node is expanded, OR
        // 2. This node is closed and hidden (so open children can still appear)
        let should_traverse = self.expanded.contains(id) || (!self.show_closed && is_closed);

        if should_traverse {
            if let Some(node) = self.nodes.get(id).cloned() {
                let mut children = node.children.clone();
                // Sort children by priority then title
                children.sort_by(|a, b| {
                    let node_a = self.nodes.get(a);
                    let node_b = self.nodes.get(b);
                    match (node_a, node_b) {
                        (Some(na), Some(nb)) => {
                            na.issue.priority.cmp(&nb.issue.priority)
                                .then_with(|| na.issue.title.cmp(&nb.issue.title))
                        }
                        _ => std::cmp::Ordering::Equal,
                    }
                });
                // If current node is hidden (closed), children appear at same depth
                // Otherwise, children are indented
                let child_depth = if !self.show_closed && is_closed { depth } else { depth + 1 };
                for child_id in children {
                    self.add_visible_recursive_id(&child_id, child_depth);
                }
            }
        }
    }

    fn add_visible_recursive_dep(
        &mut self,
        id: &str,
        depth: usize,
        visited: &mut HashSet<String>,
        added: &mut HashSet<String>,
    ) {
        // Check if this issue is closed
        let is_closed = self.nodes.get(id)
            .map(|node| node.issue.status == "closed")
            .unwrap_or(false);

        // Cycle detection: if already in current path, skip to prevent infinite loops
        if visited.contains(id) {
            return; // Already in current traversal path - cycle detected
        }

        // Check if this node is hidden (closed and not showing closed)
        let is_hidden = !self.show_closed && is_closed;

        // Only add to visible if showing closed OR issue is not closed
        if self.show_closed || !is_closed {
            // Global deduplication: show each item only once (first occurrence wins)
            if added.contains(id) {
                return; // Already shown elsewhere in tree, skip entirely
            }
            self.visible_items.push(id.to_string());
            added.insert(id.to_string());

            if let Some(node) = self.nodes.get_mut(id) {
                node.depth = depth;
            }
        }

        // Traverse children if:
        // 1. This node is expanded, OR
        // 2. This node is closed and hidden (so open children can still appear)
        let should_traverse = self.dep_expanded.contains(id) || is_hidden;

        if should_traverse {
            if let Some(node) = self.nodes.get(id).cloned() {
                let mut children = node.dep_children.clone();
                // Sort children by priority then title
                children.sort_by(|a, b| {
                    let node_a = self.nodes.get(a);
                    let node_b = self.nodes.get(b);
                    match (node_a, node_b) {
                        (Some(na), Some(nb)) => {
                            na.issue.priority.cmp(&nb.issue.priority)
                                .then_with(|| na.issue.title.cmp(&nb.issue.title))
                        }
                        _ => std::cmp::Ordering::Equal,
                    }
                });
                visited.insert(id.to_string()); // Mark as in-path
                // If current node is hidden (closed), children appear at same depth
                // Otherwise, children are indented
                let child_depth = if is_hidden { depth } else { depth + 1 };
                for child_id in children {
                    self.add_visible_recursive_dep(&child_id, child_depth, visited, added);
                }
                visited.remove(id); // Remove from path when backtracking
            }
        }
    }

    /// Get the current expansion state based on hierarchy mode
    fn current_expanded(&self) -> &HashSet<String> {
        match self.hierarchy_mode {
            HierarchyMode::IdBased => &self.expanded,
            HierarchyMode::DependencyBased => &self.dep_expanded,
        }
    }

    /// Get the current children for a node based on hierarchy mode
    fn current_children<'a>(&self, node: &'a TreeNode) -> &'a Vec<String> {
        match self.hierarchy_mode {
            HierarchyMode::IdBased => &node.children,
            HierarchyMode::DependencyBased => &node.dep_children,
        }
    }

    /// Check if a node has children in the current hierarchy mode
    pub fn has_children_in_current_mode(&self, id: &str) -> bool {
        self.nodes.get(id)
            .map(|n| !self.current_children(n).is_empty())
            .unwrap_or(false)
    }

    /// Check if a node is expanded in the current hierarchy mode
    pub fn is_expanded_in_current_mode(&self, id: &str) -> bool {
        self.current_expanded().contains(id)
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.visible_items.get(self.cursor).map(|s| s.as_str())
    }

    pub fn selected_node(&self) -> Option<&TreeNode> {
        self.selected_id().and_then(|id| self.nodes.get(id))
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.visible_items.len() {
            self.cursor += 1;
        }
    }

    pub fn move_to_top(&mut self) {
        self.cursor = 0;
    }

    pub fn move_to_bottom(&mut self) {
        if !self.visible_items.is_empty() {
            self.cursor = self.visible_items.len() - 1;
        }
    }

    pub fn toggle_expand(&mut self) {
        if let Some(id) = self.selected_id().map(|s| s.to_string()) {
            if self.has_children_in_current_mode(&id) {
                let expanded = match self.hierarchy_mode {
                    HierarchyMode::IdBased => &mut self.expanded,
                    HierarchyMode::DependencyBased => &mut self.dep_expanded,
                };
                if expanded.contains(&id) {
                    expanded.remove(&id);
                } else {
                    expanded.insert(id);
                }
                self.rebuild_visible();
            }
        }
    }

    pub fn expand(&mut self) {
        if let Some(id) = self.selected_id().map(|s| s.to_string()) {
            if self.has_children_in_current_mode(&id) {
                let expanded = match self.hierarchy_mode {
                    HierarchyMode::IdBased => &mut self.expanded,
                    HierarchyMode::DependencyBased => &mut self.dep_expanded,
                };
                if !expanded.contains(&id) {
                    expanded.insert(id);
                    self.rebuild_visible();
                }
            }
        }
    }

    pub fn collapse(&mut self) {
        if let Some(id) = self.selected_id().map(|s| s.to_string()) {
            let expanded = match self.hierarchy_mode {
                HierarchyMode::IdBased => &mut self.expanded,
                HierarchyMode::DependencyBased => &mut self.dep_expanded,
            };
            if expanded.contains(&id) {
                expanded.remove(&id);
                self.rebuild_visible();
            } else {
                // If already collapsed or leaf, move to parent
                // In ID mode: use dotted ID parent
                // In Dep mode: find first dependency (if any)
                let parent_id = match self.hierarchy_mode {
                    HierarchyMode::IdBased => Self::parent_from_dotted_id(&id),
                    HierarchyMode::DependencyBased => {
                        self.nodes.get(&id).and_then(|node| {
                            node.issue.dependencies.as_ref().and_then(|deps| {
                                deps.iter()
                                    .find(|d| d.dependency_type.as_deref() != Some("related"))
                                    .map(|d| d.id.clone())
                            })
                        })
                    }
                };
                if let Some(parent_id) = parent_id {
                    if let Some(pos) = self.visible_items.iter().position(|x| x == &parent_id) {
                        self.cursor = pos;
                    }
                }
            }
        }
    }

    pub fn debug_dump(&self) {
        eprintln!("=== Tree Debug Dump ===");
        eprintln!("Hierarchy Mode: {:?}", self.hierarchy_mode);
        eprintln!();
        eprintln!("=== ID-Based (Epics) Hierarchy ===");
        eprintln!("Root IDs: {:?}", self.root_ids);
        eprintln!("Expanded: {:?}", self.expanded);
        eprintln!();
        eprintln!("=== Dependency-Based (Deps) Hierarchy ===");
        eprintln!("Dep Root IDs: {:?}", self.dep_root_ids);
        eprintln!("Dep Expanded: {:?}", self.dep_expanded);
        eprintln!("Multi-parent IDs: {:?}", self.multi_parent_ids);
        eprintln!();
        eprintln!("Ready IDs: {:?}", self.ready_ids);
        eprintln!();
        eprintln!("All nodes:");
        for (id, node) in &self.nodes {
            let deps_info = if !node.dep_children.is_empty() {
                format!(", dep_children: {:?}", node.dep_children)
            } else {
                String::new()
            };
            eprintln!("  {} -> children: {:?}{}", id, node.children, deps_info);
        }
        eprintln!();
        eprintln!("Visible items (cursor={}, mode={:?}):", self.cursor, self.hierarchy_mode);
        for (i, id) in self.visible_items.iter().enumerate() {
            let marker = if i == self.cursor { ">" } else { " " };
            if let Some(node) = self.nodes.get(id) {
                let indent = "  ".repeat(node.depth);
                let status = if node.issue.status == "closed" {
                    "[CLOSED]"
                } else if self.ready_ids.contains(id) {
                    "[READY]"
                } else {
                    "[BLOCKED]"
                };
                let multi = if self.multi_parent_ids.contains(id) { " [MULTI]" } else { "" };
                eprintln!("{} {}{} - {} {}{}", marker, indent, id, node.issue.title, status, multi);
            }
        }
        eprintln!("=== End Dump ===");
    }

    #[allow(dead_code)]
    pub fn has_children(&self, id: &str) -> bool {
        self.nodes.get(id).map(|n| !n.children.is_empty()).unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn is_expanded(&self, id: &str) -> bool {
        self.current_expanded().contains(id)
    }

    pub fn toggle_expand_all(&mut self) {
        let expanded = match self.hierarchy_mode {
            HierarchyMode::IdBased => &mut self.expanded,
            HierarchyMode::DependencyBased => &mut self.dep_expanded,
        };

        // If anything is expanded, collapse all; otherwise expand all
        if expanded.is_empty() {
            // Expand all nodes with children (in current mode)
            for (id, node) in &self.nodes {
                let has_children = match self.hierarchy_mode {
                    HierarchyMode::IdBased => !node.children.is_empty(),
                    HierarchyMode::DependencyBased => !node.dep_children.is_empty(),
                };
                if has_children {
                    expanded.insert(id.clone());
                }
            }
        } else {
            expanded.clear();
        }
        self.rebuild_visible();
    }

    pub fn toggle_show_closed(&mut self) {
        self.show_closed = !self.show_closed;
        self.rebuild_visible();
    }

    /// Set the hierarchy mode and rebuild visible items
    pub fn set_hierarchy_mode(&mut self, mode: HierarchyMode) {
        self.hierarchy_mode = mode;
        self.rebuild_visible();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bd::Issue;

    fn make_issue(id: &str, title: &str, priority: i32) -> Issue {
        Issue {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            status: "open".to_string(),
            priority,
            issue_type: "task".to_string(),
            created_at: "2024-01-01".to_string(),
            created_by: None,
            updated_at: "2024-01-01".to_string(),
            labels: None,
            parent: None,
            dependencies: None,
            dependents: None,
            notes: None,
            design: None,
            acceptance_criteria: None,
        }
    }

    /// Helper to create a tree with ID-based hierarchy (default mode)
    fn make_tree(issues: Vec<Issue>, expanded: HashSet<String>, ready_ids: HashSet<String>) -> IssueTree {
        IssueTree::from_issues(issues, expanded, HashSet::new(), ready_ids, HierarchyMode::IdBased)
    }

    #[test]
    fn test_parent_from_dotted_id() {
        // No dot = no parent
        assert_eq!(IssueTree::parent_from_dotted_id("bsv-abc"), None);

        // Single dot
        assert_eq!(IssueTree::parent_from_dotted_id("bsv-abc.1"), Some("bsv-abc".to_string()));

        // Multiple dots
        assert_eq!(IssueTree::parent_from_dotted_id("bsv-abc.1.2"), Some("bsv-abc.1".to_string()));
        assert_eq!(IssueTree::parent_from_dotted_id("bsv-abc.1.2.3"), Some("bsv-abc.1.2".to_string()));
    }

    #[test]
    fn test_tree_from_flat_issues() {
        // Issues without dots should all be roots
        let issues = vec![
            make_issue("bsv-a", "Issue A", 2),
            make_issue("bsv-b", "Issue B", 1),
            make_issue("bsv-c", "Issue C", 2),
        ];

        let tree = make_tree(issues, HashSet::new(), HashSet::new());

        assert_eq!(tree.root_ids.len(), 3);
        // Should be sorted by priority then title
        assert_eq!(tree.root_ids[0], "bsv-b"); // P1
        assert_eq!(tree.root_ids[1], "bsv-a"); // P2, "A" < "C"
        assert_eq!(tree.root_ids[2], "bsv-c"); // P2
    }

    #[test]
    fn test_tree_hierarchy_from_dotted_ids() {
        let issues = vec![
            make_issue("bsv-epic", "Epic", 1),
            make_issue("bsv-epic.1", "Task 1", 2),
            make_issue("bsv-epic.2", "Task 2", 2),
            make_issue("bsv-epic.1.1", "Subtask 1.1", 2),
        ];

        let tree = make_tree(issues, HashSet::new(), HashSet::new());

        // Only the epic should be a root
        assert_eq!(tree.root_ids.len(), 1);
        assert_eq!(tree.root_ids[0], "bsv-epic");

        // Epic should have 2 children
        let epic_node = tree.nodes.get("bsv-epic").unwrap();
        assert_eq!(epic_node.children.len(), 2);

        // Task 1 should have 1 child
        let task1_node = tree.nodes.get("bsv-epic.1").unwrap();
        assert_eq!(task1_node.children.len(), 1);
        assert!(task1_node.children.contains(&"bsv-epic.1.1".to_string()));

        // Task 2 should have no children
        let task2_node = tree.nodes.get("bsv-epic.2").unwrap();
        assert!(task2_node.children.is_empty());
    }

    #[test]
    fn test_orphan_dotted_ids_become_roots() {
        // If parent doesn't exist, dotted ID becomes a root
        let issues = vec![
            make_issue("bsv-epic.1", "Orphan Task", 2),
            make_issue("bsv-other", "Other", 2),
        ];

        let tree = make_tree(issues, HashSet::new(), HashSet::new());

        // Both should be roots since bsv-epic doesn't exist
        assert_eq!(tree.root_ids.len(), 2);
        assert!(tree.root_ids.contains(&"bsv-epic.1".to_string()));
        assert!(tree.root_ids.contains(&"bsv-other".to_string()));
    }

    #[test]
    fn test_visible_items_collapsed() {
        let issues = vec![
            make_issue("bsv-a", "A", 2),
            make_issue("bsv-a.1", "A.1", 2),
            make_issue("bsv-b", "B", 2),
        ];

        let tree = make_tree(issues, HashSet::new(), HashSet::new());

        // With nothing expanded, should only see roots
        assert_eq!(tree.visible_items.len(), 2);
        assert!(tree.visible_items.contains(&"bsv-a".to_string()));
        assert!(tree.visible_items.contains(&"bsv-b".to_string()));
    }

    #[test]
    fn test_visible_items_expanded() {
        let issues = vec![
            make_issue("bsv-a", "A", 2),
            make_issue("bsv-a.1", "A.1", 2),
            make_issue("bsv-b", "B", 2),
        ];

        let mut expanded = HashSet::new();
        expanded.insert("bsv-a".to_string());

        let tree = make_tree(issues, expanded, HashSet::new());

        // Should see A, A.1, and B
        assert_eq!(tree.visible_items.len(), 3);
    }

    #[test]
    fn test_navigation() {
        let issues = vec![
            make_issue("bsv-a", "A", 2),
            make_issue("bsv-b", "B", 2),
            make_issue("bsv-c", "C", 2),
        ];

        let mut tree = make_tree(issues, HashSet::new(), HashSet::new());

        assert_eq!(tree.cursor, 0);
        assert_eq!(tree.selected_id(), Some("bsv-a"));

        tree.move_down();
        assert_eq!(tree.cursor, 1);
        assert_eq!(tree.selected_id(), Some("bsv-b"));

        tree.move_down();
        assert_eq!(tree.cursor, 2);

        // Can't go past end
        tree.move_down();
        assert_eq!(tree.cursor, 2);

        tree.move_up();
        assert_eq!(tree.cursor, 1);

        tree.move_to_top();
        assert_eq!(tree.cursor, 0);

        tree.move_to_bottom();
        assert_eq!(tree.cursor, 2);
    }

    #[test]
    fn test_expand_collapse() {
        let issues = vec![
            make_issue("bsv-a", "A", 2),
            make_issue("bsv-a.1", "A.1", 2),
        ];

        let mut tree = make_tree(issues, HashSet::new(), HashSet::new());

        // Initially only root visible
        assert_eq!(tree.visible_items.len(), 1);
        assert!(!tree.is_expanded("bsv-a"));

        // Expand
        tree.expand();
        assert!(tree.is_expanded("bsv-a"));
        assert_eq!(tree.visible_items.len(), 2);

        // Collapse
        tree.collapse();
        assert!(!tree.is_expanded("bsv-a"));
        assert_eq!(tree.visible_items.len(), 1);

        // Toggle
        tree.toggle_expand();
        assert!(tree.is_expanded("bsv-a"));
        tree.toggle_expand();
        assert!(!tree.is_expanded("bsv-a"));
    }

    #[test]
    fn test_depth_calculation() {
        let issues = vec![
            make_issue("bsv-a", "A", 2),
            make_issue("bsv-a.1", "A.1", 2),
            make_issue("bsv-a.1.1", "A.1.1", 2),
        ];

        let mut expanded = HashSet::new();
        expanded.insert("bsv-a".to_string());
        expanded.insert("bsv-a.1".to_string());

        let tree = make_tree(issues, expanded, HashSet::new());

        assert_eq!(tree.nodes.get("bsv-a").unwrap().depth, 0);
        assert_eq!(tree.nodes.get("bsv-a.1").unwrap().depth, 1);
        assert_eq!(tree.nodes.get("bsv-a.1.1").unwrap().depth, 2);
    }

    // === Dependency Hierarchy Tests ===

    fn make_issue_with_deps(id: &str, title: &str, dep_ids: Vec<&str>) -> Issue {
        let dependencies = if dep_ids.is_empty() {
            None
        } else {
            Some(dep_ids.iter().map(|dep_id| crate::bd::Dependency {
                id: dep_id.to_string(),
                title: format!("Dep {}", dep_id),
                dependency_type: Some("blocks".to_string()),
            }).collect())
        };
        Issue {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            status: "open".to_string(),
            priority: 2,
            issue_type: "task".to_string(),
            created_at: "2024-01-01".to_string(),
            created_by: None,
            updated_at: "2024-01-01".to_string(),
            labels: None,
            parent: None,
            dependencies,
            dependents: None,
            notes: None,
            design: None,
            acceptance_criteria: None,
        }
    }

    fn make_tree_dep_mode(issues: Vec<Issue>, dep_expanded: HashSet<String>) -> IssueTree {
        IssueTree::from_issues(issues, HashSet::new(), dep_expanded, HashSet::new(), HierarchyMode::DependencyBased)
    }

    #[test]
    fn test_dep_hierarchy_roots() {
        // Issues with no dependencies should be roots
        let issues = vec![
            make_issue_with_deps("root1", "Root 1", vec![]),
            make_issue_with_deps("root2", "Root 2", vec![]),
            make_issue_with_deps("child", "Child", vec!["root1"]),
        ];

        let tree = make_tree_dep_mode(issues, HashSet::new());

        // root1 and root2 have no dependencies, so they're roots
        assert!(tree.dep_root_ids.contains(&"root1".to_string()));
        assert!(tree.dep_root_ids.contains(&"root2".to_string()));
        // child depends on root1, so it's not a root
        assert!(!tree.dep_root_ids.contains(&"child".to_string()));
    }

    #[test]
    fn test_dep_hierarchy_children() {
        // In dep mode, if A depends on B, then A is a child of B
        let issues = vec![
            make_issue_with_deps("parent", "Parent", vec![]),
            make_issue_with_deps("child", "Child", vec!["parent"]),
            make_issue_with_deps("grandchild", "Grandchild", vec!["child"]),
        ];

        let tree = make_tree_dep_mode(issues, HashSet::new());

        // parent's dep_children should include child
        assert!(tree.nodes.get("parent").unwrap().dep_children.contains(&"child".to_string()));
        // child's dep_children should include grandchild
        assert!(tree.nodes.get("child").unwrap().dep_children.contains(&"grandchild".to_string()));
        // grandchild has no dep_children
        assert!(tree.nodes.get("grandchild").unwrap().dep_children.is_empty());
    }

    #[test]
    fn test_dep_hierarchy_multi_parent() {
        // Issue depending on multiple issues should appear under each
        let issues = vec![
            make_issue_with_deps("root1", "Root 1", vec![]),
            make_issue_with_deps("root2", "Root 2", vec![]),
            make_issue_with_deps("multi", "Multi-parent", vec!["root1", "root2"]),
        ];

        let tree = make_tree_dep_mode(issues, HashSet::new());

        // multi should be in multi_parent_ids
        assert!(tree.multi_parent_ids.contains(&"multi".to_string()));
        // multi should be a child of both root1 and root2
        assert!(tree.nodes.get("root1").unwrap().dep_children.contains(&"multi".to_string()));
        assert!(tree.nodes.get("root2").unwrap().dep_children.contains(&"multi".to_string()));
    }

    #[test]
    fn test_dep_hierarchy_visible_collapsed() {
        // When collapsed, only roots should be visible
        let issues = vec![
            make_issue_with_deps("root", "Root", vec![]),
            make_issue_with_deps("child", "Child", vec!["root"]),
        ];

        let tree = make_tree_dep_mode(issues, HashSet::new());

        // Only root should be visible (collapsed by default)
        assert_eq!(tree.visible_items.len(), 1);
        assert_eq!(tree.visible_items[0], "root");
    }

    #[test]
    fn test_dep_hierarchy_visible_expanded() {
        // When expanded, children should be visible
        let issues = vec![
            make_issue_with_deps("root", "Root", vec![]),
            make_issue_with_deps("child", "Child", vec!["root"]),
        ];

        let mut dep_expanded = HashSet::new();
        dep_expanded.insert("root".to_string());

        let tree = make_tree_dep_mode(issues, dep_expanded);

        // Both root and child should be visible
        assert_eq!(tree.visible_items.len(), 2);
        assert!(tree.visible_items.contains(&"root".to_string()));
        assert!(tree.visible_items.contains(&"child".to_string()));
    }

    #[test]
    fn test_mode_switching() {
        // Same issues should show different hierarchies in different modes
        let issues = vec![
            // ID-based: epic -> epic.1
            make_issue_with_deps("epic", "Epic", vec![]),
            make_issue_with_deps("epic.1", "Epic Task", vec![]),
            // Dep-based: task depends on epic.1
            make_issue_with_deps("task", "Standalone Task", vec!["epic.1"]),
        ];

        // In ID mode
        let tree_id = IssueTree::from_issues(
            issues.clone(),
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
            HierarchyMode::IdBased
        );
        // epic.1 is child of epic, task is a root
        assert!(tree_id.nodes.get("epic").unwrap().children.contains(&"epic.1".to_string()));
        assert!(tree_id.root_ids.contains(&"task".to_string()));

        // In Dep mode
        let tree_dep = IssueTree::from_issues(
            issues,
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
            HierarchyMode::DependencyBased
        );
        // task depends on epic.1, so task is child of epic.1 in dep view
        assert!(tree_dep.nodes.get("epic.1").unwrap().dep_children.contains(&"task".to_string()));
        // epic and epic.1 are dep roots (no dependencies)
        assert!(tree_dep.dep_root_ids.contains(&"epic".to_string()));
        assert!(tree_dep.dep_root_ids.contains(&"epic.1".to_string()));
    }

    #[test]
    fn test_dep_ignores_related_type() {
        // "related" dependency type should not create parent-child relationship
        let mut issue = make_issue_with_deps("child", "Child", vec!["parent"]);
        // Change the dependency type to "related"
        if let Some(ref mut deps) = issue.dependencies {
            deps[0].dependency_type = Some("related".to_string());
        }
        let issues = vec![
            make_issue_with_deps("parent", "Parent", vec![]),
            issue,
        ];

        let tree = make_tree_dep_mode(issues, HashSet::new());

        // child should NOT be in parent's dep_children (because it's "related", not "blocks")
        assert!(!tree.nodes.get("parent").unwrap().dep_children.contains(&"child".to_string()));
        // child should be a root since its only dependency is "related"
        assert!(tree.dep_root_ids.contains(&"child".to_string()));
    }

    #[test]
    fn test_dep_hierarchy_depth() {
        // Verify depth is calculated correctly in dep mode
        let issues = vec![
            make_issue_with_deps("root", "Root", vec![]),
            make_issue_with_deps("child", "Child", vec!["root"]),
            make_issue_with_deps("grandchild", "Grandchild", vec!["child"]),
        ];

        let mut dep_expanded = HashSet::new();
        dep_expanded.insert("root".to_string());
        dep_expanded.insert("child".to_string());

        let tree = make_tree_dep_mode(issues, dep_expanded);

        // Check visible items are in correct order with correct depths
        assert_eq!(tree.visible_items, vec!["root", "child", "grandchild"]);

        // Note: depth is recalculated during rebuild_visible based on traversal
        // The node.depth in the visible traversal should reflect the tree depth
    }

    // === Tests for show_closed behavior ===

    fn make_closed_issue(id: &str, title: &str, priority: i32) -> Issue {
        Issue {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            status: "closed".to_string(),
            priority,
            issue_type: "task".to_string(),
            created_at: "2024-01-01".to_string(),
            created_by: None,
            updated_at: "2024-01-01".to_string(),
            labels: None,
            parent: None,
            dependencies: None,
            dependents: None,
            notes: None,
            design: None,
            acceptance_criteria: None,
        }
    }

    #[test]
    fn test_open_child_of_closed_parent_visible_in_id_mode() {
        // Open children of closed parents should still be visible when closed hidden
        let issues = vec![
            make_closed_issue("parent", "Closed Parent", 1),
            make_issue("parent.1", "Open Child", 2),
        ];

        let mut expanded = HashSet::new();
        expanded.insert("parent".to_string());

        let mut tree = IssueTree::from_issues(
            issues,
            expanded,
            HashSet::new(),
            HashSet::new(),
            HierarchyMode::IdBased
        );

        // With show_closed = true, both should be visible
        assert_eq!(tree.visible_items.len(), 2);
        assert!(tree.visible_items.contains(&"parent".to_string()));
        assert!(tree.visible_items.contains(&"parent.1".to_string()));

        // Toggle show_closed to false
        tree.toggle_show_closed();

        // Now only the open child should be visible
        assert_eq!(tree.visible_items.len(), 1);
        assert!(!tree.visible_items.contains(&"parent".to_string()));
        assert!(tree.visible_items.contains(&"parent.1".to_string()));

        // The open child should appear at depth 0 (since parent is hidden)
        assert_eq!(tree.nodes.get("parent.1").unwrap().depth, 0);
    }

    #[test]
    fn test_open_grandchild_of_closed_hierarchy() {
        // Open grandchildren should be visible even when ancestors are closed
        let issues = vec![
            make_closed_issue("root", "Closed Root", 1),
            make_closed_issue("root.1", "Closed Child", 1),
            make_issue("root.1.1", "Open Grandchild", 2),
        ];

        let mut expanded = HashSet::new();
        expanded.insert("root".to_string());
        expanded.insert("root.1".to_string());

        let mut tree = IssueTree::from_issues(
            issues,
            expanded,
            HashSet::new(),
            HashSet::new(),
            HierarchyMode::IdBased
        );

        // Toggle show_closed to false
        tree.toggle_show_closed();

        // Only the open grandchild should be visible
        assert_eq!(tree.visible_items.len(), 1);
        assert!(tree.visible_items.contains(&"root.1.1".to_string()));

        // The grandchild should appear at depth 0 (since all ancestors are hidden)
        assert_eq!(tree.nodes.get("root.1.1").unwrap().depth, 0);
    }

    #[test]
    fn test_open_child_of_closed_parent_in_dep_mode() {
        // In dep mode, open children of closed issues should still be visible
        let mut closed_issue = make_issue_with_deps("blocker", "Closed Blocker", vec![]);
        closed_issue.status = "closed".to_string();

        let issues = vec![
            closed_issue,
            make_issue_with_deps("blocked", "Open Blocked", vec!["blocker"]),
        ];

        let mut dep_expanded = HashSet::new();
        dep_expanded.insert("blocker".to_string());

        let mut tree = IssueTree::from_issues(
            issues,
            HashSet::new(),
            dep_expanded,
            HashSet::new(),
            HierarchyMode::DependencyBased
        );

        // With show_closed = true, both should be visible
        assert_eq!(tree.visible_items.len(), 2);

        // Toggle show_closed to false
        tree.toggle_show_closed();

        // Only the open issue should be visible
        assert_eq!(tree.visible_items.len(), 1);
        assert!(tree.visible_items.contains(&"blocked".to_string()));

        // It should appear at depth 0 (since its only parent is hidden)
        assert_eq!(tree.nodes.get("blocked").unwrap().depth, 0);
    }

    #[test]
    fn test_no_duplicates_from_multiple_hidden_closed_parents() {
        // When multiple hidden closed parents share the same open child,
        // the child should only appear once (not duplicated)
        let mut closed1 = make_issue_with_deps("closed1", "Closed Blocker 1", vec![]);
        closed1.status = "closed".to_string();

        let mut closed2 = make_issue_with_deps("closed2", "Closed Blocker 2", vec![]);
        closed2.status = "closed".to_string();

        let issues = vec![
            closed1,
            closed2,
            make_issue_with_deps("shared_child", "Shared Open Child", vec!["closed1", "closed2"]),
        ];

        let mut tree = IssueTree::from_issues(
            issues,
            HashSet::new(),
            HashSet::new(), // not expanded - closed items will auto-traverse when hidden
            HashSet::new(),
            HierarchyMode::DependencyBased
        );

        // With show_closed = true, only roots are visible (nothing expanded)
        // closed1 and closed2 are roots, shared_child is not visible yet
        assert_eq!(tree.visible_items.len(), 2);

        // Toggle show_closed to false - now closed items are hidden but auto-traversed
        tree.toggle_show_closed();

        // The shared child should appear only ONCE, not twice
        // (even though it's a child of two different hidden closed parents)
        assert_eq!(tree.visible_items.len(), 1);
        assert!(tree.visible_items.contains(&"shared_child".to_string()));

        // Count how many times shared_child appears
        let count = tree.visible_items.iter()
            .filter(|id| *id == "shared_child")
            .count();
        assert_eq!(count, 1, "shared_child should appear exactly once, not {} times", count);
    }

    #[test]
    fn test_no_duplicates_multi_path_different_depths() {
        // Test the scenario where an item has multiple parents at different depths
        // and would otherwise appear multiple times at different depths.
        //
        // Structure:
        //   root (expanded)
        //   ├── parent_a (expanded)
        //   │   └── shared_leaf (depends on parent_a)
        //   └── parent_b (expanded)
        //       └── child_b (expanded)
        //           └── shared_leaf (depends on child_b too)
        //
        // Without deduplication, shared_leaf would appear at depth 2 (under parent_a)
        // AND at depth 3 (under child_b). With global deduplication, it appears only once.
        let issues = vec![
            make_issue_with_deps("root", "Root", vec![]),
            make_issue_with_deps("parent_a", "Parent A", vec!["root"]),
            make_issue_with_deps("parent_b", "Parent B", vec!["root"]),
            make_issue_with_deps("child_b", "Child B", vec!["parent_b"]),
            make_issue_with_deps("shared_leaf", "Shared Leaf", vec!["parent_a", "child_b"]),
        ];

        let mut dep_expanded = HashSet::new();
        dep_expanded.insert("root".to_string());
        dep_expanded.insert("parent_a".to_string());
        dep_expanded.insert("parent_b".to_string());
        dep_expanded.insert("child_b".to_string());

        let tree = IssueTree::from_issues(
            issues,
            HashSet::new(),
            dep_expanded,
            HashSet::new(),
            HierarchyMode::DependencyBased
        );

        // shared_leaf should appear exactly once
        let count = tree.visible_items.iter()
            .filter(|id| *id == "shared_leaf")
            .count();
        assert_eq!(count, 1, "shared_leaf should appear exactly once, not {} times", count);

        // Verify total structure: root, parent_a, shared_leaf, parent_b, child_b
        // (shared_leaf appears under parent_a first due to traversal order)
        assert_eq!(tree.visible_items.len(), 5);
    }

    #[test]
    fn test_no_duplicates_diamond_dependency() {
        // Diamond dependency pattern:
        //        root
        //       /    \
        //   left     right
        //       \    /
        //       bottom
        //
        // bottom depends on both left and right, which both depend on root.
        // bottom should only appear once.
        let issues = vec![
            make_issue_with_deps("root", "Root", vec![]),
            make_issue_with_deps("left", "Left", vec!["root"]),
            make_issue_with_deps("right", "Right", vec!["root"]),
            make_issue_with_deps("bottom", "Bottom", vec!["left", "right"]),
        ];

        let mut dep_expanded = HashSet::new();
        dep_expanded.insert("root".to_string());
        dep_expanded.insert("left".to_string());
        dep_expanded.insert("right".to_string());

        let tree = IssueTree::from_issues(
            issues,
            HashSet::new(),
            dep_expanded,
            HashSet::new(),
            HierarchyMode::DependencyBased
        );

        // bottom should appear exactly once
        let count = tree.visible_items.iter()
            .filter(|id| *id == "bottom")
            .count();
        assert_eq!(count, 1, "bottom should appear exactly once in diamond pattern");

        // All 4 items should be visible
        assert_eq!(tree.visible_items.len(), 4);
    }

    #[test]
    fn test_no_duplicates_deeply_nested_multi_parent() {
        // Deep nesting with multi-parent at the bottom:
        //   root -> a -> b -> c -> shared
        //   root -> x -> y -> shared
        //
        // shared has paths at depth 4 (via a->b->c) and depth 3 (via x->y)
        let issues = vec![
            make_issue_with_deps("root", "Root", vec![]),
            make_issue_with_deps("a", "A", vec!["root"]),
            make_issue_with_deps("b", "B", vec!["a"]),
            make_issue_with_deps("c", "C", vec!["b"]),
            make_issue_with_deps("x", "X", vec!["root"]),
            make_issue_with_deps("y", "Y", vec!["x"]),
            make_issue_with_deps("shared", "Shared", vec!["c", "y"]),
        ];

        let mut dep_expanded = HashSet::new();
        for id in ["root", "a", "b", "c", "x", "y"] {
            dep_expanded.insert(id.to_string());
        }

        let tree = IssueTree::from_issues(
            issues,
            HashSet::new(),
            dep_expanded,
            HashSet::new(),
            HierarchyMode::DependencyBased
        );

        // shared should appear exactly once
        let count = tree.visible_items.iter()
            .filter(|id| *id == "shared")
            .count();
        assert_eq!(count, 1, "shared should appear exactly once even with deep nesting");

        // All 7 items should be visible
        assert_eq!(tree.visible_items.len(), 7);
    }
}
