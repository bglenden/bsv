use crate::bd::Issue;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub issue: Issue,
    pub children: Vec<String>,
    pub depth: usize,
}

#[derive(Debug)]
pub struct IssueTree {
    pub nodes: HashMap<String, TreeNode>,
    pub root_ids: Vec<String>,
    pub expanded: HashSet<String>,
    pub ready_ids: HashSet<String>,
    pub visible_items: Vec<String>,
    pub cursor: usize,
    pub show_closed: bool,
}

impl IssueTree {
    pub fn from_issues(issues: Vec<Issue>, expanded: HashSet<String>, ready_ids: HashSet<String>) -> Self {
        let mut nodes: HashMap<String, TreeNode> = HashMap::new();
        let mut children_map: HashMap<String, Vec<String>> = HashMap::new();

        // First pass: create all nodes
        for issue in &issues {
            nodes.insert(issue.id.clone(), TreeNode {
                issue: issue.clone(),
                children: vec![],
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

        // Third pass: populate children lists
        for (parent_id, child_ids) in &children_map {
            if let Some(node) = nodes.get_mut(parent_id) {
                node.children = child_ids.clone();
            }
        }

        // Find root nodes: no dot in ID, OR parent from dotted ID doesn't exist
        let mut root_ids: Vec<String> = nodes.keys()
            .filter(|id| {
                match Self::parent_from_dotted_id(id) {
                    Some(parent_id) => !nodes.contains_key(&parent_id),
                    None => true, // no dot = root
                }
            })
            .cloned()
            .collect();

        // Sort roots by priority then by title
        root_ids.sort_by(|a, b| {
            let node_a = nodes.get(a).unwrap();
            let node_b = nodes.get(b).unwrap();
            node_a.issue.priority.cmp(&node_b.issue.priority)
                .then_with(|| node_a.issue.title.cmp(&node_b.issue.title))
        });

        let mut tree = IssueTree {
            nodes,
            root_ids,
            expanded,
            ready_ids,
            visible_items: vec![],
            cursor: 0,
            show_closed: true,
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
        for root_id in &self.root_ids.clone() {
            self.add_visible_recursive(root_id, 0);
        }
        if self.cursor >= self.visible_items.len() && !self.visible_items.is_empty() {
            self.cursor = self.visible_items.len() - 1;
        }
    }

    fn add_visible_recursive(&mut self, id: &str, depth: usize) {
        // Skip closed issues if show_closed is false
        if !self.show_closed {
            if let Some(node) = self.nodes.get(id) {
                if node.issue.status == "closed" {
                    return;
                }
            }
        }

        self.visible_items.push(id.to_string());

        if let Some(node) = self.nodes.get_mut(id) {
            node.depth = depth;
        }

        if self.expanded.contains(id) {
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
                for child_id in children {
                    self.add_visible_recursive(&child_id, depth + 1);
                }
            }
        }
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
            if let Some(node) = self.nodes.get(&id) {
                if !node.children.is_empty() {
                    if self.expanded.contains(&id) {
                        self.expanded.remove(&id);
                    } else {
                        self.expanded.insert(id);
                    }
                    self.rebuild_visible();
                }
            }
        }
    }

    pub fn expand(&mut self) {
        if let Some(id) = self.selected_id().map(|s| s.to_string()) {
            if let Some(node) = self.nodes.get(&id) {
                if !node.children.is_empty() && !self.expanded.contains(&id) {
                    self.expanded.insert(id);
                    self.rebuild_visible();
                }
            }
        }
    }

    pub fn collapse(&mut self) {
        if let Some(id) = self.selected_id().map(|s| s.to_string()) {
            if self.expanded.contains(&id) {
                self.expanded.remove(&id);
                self.rebuild_visible();
            } else {
                // If already collapsed or leaf, move to parent (based on dotted ID)
                if let Some(parent_id) = Self::parent_from_dotted_id(&id) {
                    if let Some(pos) = self.visible_items.iter().position(|x| x == &parent_id) {
                        self.cursor = pos;
                    }
                }
            }
        }
    }

    pub fn debug_dump(&self) {
        eprintln!("=== Tree Debug Dump ===");
        eprintln!("Root IDs: {:?}", self.root_ids);
        eprintln!("Expanded: {:?}", self.expanded);
        eprintln!("\nAll nodes:");
        for (id, node) in &self.nodes {
            eprintln!("  {} -> children: {:?}, depth: {}", id, node.children, node.depth);
        }
        eprintln!("\nVisible items (cursor={}):", self.cursor);
        for (i, id) in self.visible_items.iter().enumerate() {
            let marker = if i == self.cursor { ">" } else { " " };
            if let Some(node) = self.nodes.get(id) {
                let indent = "  ".repeat(node.depth);
                eprintln!("{} {}{} - {}", marker, indent, id, node.issue.title);
            }
        }
        eprintln!("=== End Dump ===");
    }

    #[allow(dead_code)]
    pub fn has_children(&self, id: &str) -> bool {
        self.nodes.get(id).map(|n| !n.children.is_empty()).unwrap_or(false)
    }

    pub fn is_expanded(&self, id: &str) -> bool {
        self.expanded.contains(id)
    }

    pub fn toggle_expand_all(&mut self) {
        // If anything is expanded, collapse all; otherwise expand all
        if self.expanded.is_empty() {
            // Expand all nodes with children
            for (id, node) in &self.nodes {
                if !node.children.is_empty() {
                    self.expanded.insert(id.clone());
                }
            }
        } else {
            self.expanded.clear();
        }
        self.rebuild_visible();
    }

    pub fn toggle_show_closed(&mut self) {
        self.show_closed = !self.show_closed;
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

        let tree = IssueTree::from_issues(issues, HashSet::new(), HashSet::new());

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

        let tree = IssueTree::from_issues(issues, HashSet::new(), HashSet::new());

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

        let tree = IssueTree::from_issues(issues, HashSet::new(), HashSet::new());

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

        let tree = IssueTree::from_issues(issues, HashSet::new(), HashSet::new());

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

        let tree = IssueTree::from_issues(issues, expanded, HashSet::new());

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

        let mut tree = IssueTree::from_issues(issues, HashSet::new(), HashSet::new());

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

        let mut tree = IssueTree::from_issues(issues, HashSet::new(), HashSet::new());

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

        let tree = IssueTree::from_issues(issues, expanded, HashSet::new());

        assert_eq!(tree.nodes.get("bsv-a").unwrap().depth, 0);
        assert_eq!(tree.nodes.get("bsv-a.1").unwrap().depth, 1);
        assert_eq!(tree.nodes.get("bsv-a.1.1").unwrap().depth, 2);
    }
}
