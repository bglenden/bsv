use crate::bd::Issue;
use crate::tree::IssueTree;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

/// Convert markdown text to styled Lines
fn markdown_to_lines(text: &str) -> Vec<Line<'static>> {
    let mut lines_out = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lang = String::new();

    for line in text.lines() {
        // Check for code block fence
        if let Some(lang) = line.strip_prefix("```") {
            if in_code_block {
                // End of code block
                in_code_block = false;
                code_block_lang.clear();
            } else {
                // Start of code block
                in_code_block = true;
                code_block_lang = lang.trim().to_string();
                // Show language tag if present
                if !code_block_lang.is_empty() {
                    lines_out.push(Line::from(Span::styled(
                        format!("── {} ──", code_block_lang),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
            continue;
        }

        if in_code_block {
            // Code block content - show in green with slight indent
            lines_out.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Color::Green),
            )));
        } else {
            lines_out.push(markdown_line_to_spans(line));
        }
    }

    lines_out
}

/// Convert a single line of markdown to styled Spans
fn markdown_line_to_spans(line: &str) -> Line<'static> {
    // Handle horizontal rules (---, ***, ___)
    let trimmed = line.trim();
    if (trimmed.chars().all(|c| c == '-') && trimmed.len() >= 3)
        || (trimmed.chars().all(|c| c == '*') && trimmed.len() >= 3)
        || (trimmed.chars().all(|c| c == '_') && trimmed.len() >= 3)
    {
        return Line::from(Span::styled(
            "────────────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Handle headers (check longest prefix first)
    if let Some(text) = line.strip_prefix("### ") {
        return Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(text) = line.strip_prefix("## ") {
        return Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(text) = line.strip_prefix("# ") {
        return Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }

    // Handle blockquotes
    if let Some(text) = line.strip_prefix("> ") {
        return Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                text.to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::ITALIC),
            ),
        ]);
    }
    if line == ">" {
        return Line::from(Span::styled("│", Style::default().fg(Color::DarkGray)));
    }

    // Handle table rows (lines starting with |)
    if line.starts_with('|') {
        // Check if it's a separator row (|---|---|)
        if line.contains("---") || line.contains(":-") || line.contains("-:") {
            return Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::DarkGray),
            ));
        }
        // Regular table row - highlight pipes
        let mut spans = Vec::new();
        for part in line.split('|') {
            if !spans.is_empty() {
                spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
            }
            spans.push(Span::raw(part.to_string()));
        }
        return Line::from(spans);
    }

    // Handle list items (just pass through with slight styling)
    if line.starts_with("- ") || line.starts_with("* ") {
        let rest = &line[2..];
        return Line::from(vec![
            Span::styled("• ", Style::default().fg(Color::Cyan)),
            Span::raw(parse_inline_markdown(rest)),
        ]);
    }

    // Handle indented list items
    if line.starts_with("  - ") || line.starts_with("  * ") {
        let rest = &line[4..];
        return Line::from(vec![
            Span::raw("  "),
            Span::styled("◦ ", Style::default().fg(Color::Cyan)),
            Span::raw(parse_inline_markdown(rest)),
        ]);
    }

    // Handle numbered lists
    if let Some(pos) = line.find(". ") {
        if pos <= 3 && line[..pos].chars().all(|c| c.is_ascii_digit()) {
            let rest = &line[pos + 2..];
            return Line::from(vec![
                Span::styled(format!("{}. ", &line[..pos]), Style::default().fg(Color::Cyan)),
                Span::raw(parse_inline_markdown(rest)),
            ]);
        }
    }

    // Parse inline markdown for regular lines
    parse_inline_markdown_to_line(line)
}

/// Parse inline markdown (bold, italic, code, links) and return a Line
fn parse_inline_markdown_to_line(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check for inline code
        if chars[i] == '`' {
            if !current.is_empty() {
                spans.push(Span::raw(current.clone()));
                current.clear();
            }
            i += 1;
            let mut code = String::new();
            while i < chars.len() && chars[i] != '`' {
                code.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // skip closing `
            }
            spans.push(Span::styled(code, Style::default().fg(Color::Cyan)));
            continue;
        }

        // Check for bold **text**
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if !current.is_empty() {
                spans.push(Span::raw(current.clone()));
                current.clear();
            }
            i += 2;
            let mut bold = String::new();
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                bold.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() {
                i += 2; // skip closing **
            }
            spans.push(Span::styled(bold, Style::default().add_modifier(Modifier::BOLD)));
            continue;
        }

        // Check for italic *text* (single asterisk, not followed by another)
        if chars[i] == '*' && (i + 1 >= chars.len() || chars[i + 1] != '*') {
            if !current.is_empty() {
                spans.push(Span::raw(current.clone()));
                current.clear();
            }
            i += 1;
            let mut italic = String::new();
            while i < chars.len() && chars[i] != '*' {
                italic.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // skip closing *
            }
            spans.push(Span::styled(italic, Style::default().add_modifier(Modifier::ITALIC)));
            continue;
        }

        // Check for links [text](url)
        if chars[i] == '[' {
            let start = i;
            i += 1;
            let mut link_text = String::new();
            while i < chars.len() && chars[i] != ']' {
                link_text.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() && chars[i] == ']' && chars[i + 1] == '(' {
                i += 2;
                let mut url = String::new();
                while i < chars.len() && chars[i] != ')' {
                    url.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1; // skip closing )
                }
                if !current.is_empty() {
                    spans.push(Span::raw(current.clone()));
                    current.clear();
                }
                spans.push(Span::styled(
                    link_text,
                    Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
                ));
                continue;
            } else {
                // Not a valid link, reset
                i = start;
            }
        }

        current.push(chars[i]);
        i += 1;
    }

    if !current.is_empty() {
        spans.push(Span::raw(current));
    }

    if spans.is_empty() {
        Line::from("")
    } else {
        Line::from(spans)
    }
}

/// Simple inline markdown parsing that returns plain string (for list items)
fn parse_inline_markdown(text: &str) -> String {
    // For simplicity, just return the text as-is for now
    // The full parsing happens in parse_inline_markdown_to_line
    text.to_string()
}

#[allow(clippy::too_many_arguments)]
pub fn render(frame: &mut Frame, tree: &IssueTree, selected_details: Option<&Issue>, show_help: bool, focus: crate::Focus, detail_scroll: u16, edit_state: Option<&crate::EditState>, panel_ratio: f32) {
    // Convert ratio to percentages, clamped to reasonable bounds
    let left_percent = ((panel_ratio.clamp(0.15, 0.85)) * 100.0) as u16;
    let right_percent = 100 - left_percent;

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(left_percent), Constraint::Percentage(right_percent)])
        .split(frame.area());

    let tree_focused = focus == crate::Focus::Tree;
    render_tree_panel(frame, tree, chunks[0], tree_focused);

    // Use full details if available (has dependencies), otherwise fall back to tree node
    let issue_for_details = selected_details.or_else(|| tree.selected_node().map(|n| &n.issue));
    render_detail_panel(frame, issue_for_details, &tree.ready_ids, chunks[1], !tree_focused, detail_scroll, edit_state);

    if show_help {
        render_help_overlay(frame);
    }
}

fn render_tree_panel(frame: &mut Frame, tree: &IssueTree, area: Rect, focused: bool) {
    use crate::HierarchyMode;

    let items: Vec<ListItem> = tree.visible_items
        .iter()
        .enumerate()
        .filter_map(|(idx, id)| {
            tree.nodes.get(id).map(|node| {
                let is_selected = idx == tree.cursor;
                // Use mode-aware children check
                let has_children = tree.has_children_in_current_mode(id);
                let is_expanded = tree.is_expanded_in_current_mode(id);
                let is_closed = node.issue.status == "closed";
                let is_ready = tree.ready_ids.contains(id);
                let is_multi_parent = tree.multi_parent_ids.contains(id);

                // Build the tree prefix with indentation
                // Use hybrid indent: normal up to depth 4, then show [N] indicator
                const MAX_VISUAL_INDENT: usize = 4;
                let indent = if node.depth <= MAX_VISUAL_INDENT {
                    "  ".repeat(node.depth)
                } else {
                    format!("{}[{}]", "  ".repeat(MAX_VISUAL_INDENT), node.depth)
                };

                let icon = if has_children {
                    if is_expanded { "▼ " } else { "▶ " }
                } else {
                    "  "
                };

                // Status-based styling: green=ready, red=blocked, gray=closed
                let text_style = if is_closed {
                    Style::default().fg(Color::DarkGray)
                } else if is_ready {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                };

                // Multi-parent issues in dependency view show ID in cyan
                let id_style = if is_multi_parent && tree.hierarchy_mode == HierarchyMode::DependencyBased {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let line = Line::from(vec![
                    Span::styled(format!("{}{}", indent, icon), text_style),
                    Span::styled(format!("{} ", node.issue.id), id_style),
                    Span::styled(node.issue.title.clone(), text_style),
                ]);

                let style = if is_selected {
                    Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(line).style(style)
            })
        })
        .collect();

    // Show mode indicator in title
    let mode_indicator = match tree.hierarchy_mode {
        HierarchyMode::IdBased => "Epics",
        HierarchyMode::DependencyBased => "Deps",
    };
    let title = format!(" Issues ({}) ", mode_indicator);

    let border_color = if focused { Color::Cyan } else { Color::DarkGray };
    let list = List::new(items)
        .block(Block::default()
            .title(title)
            .title_bottom(Line::from(" ? help  d=Epics/Deps ").centered())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)));

    frame.render_widget(list, area);
}

fn render_detail_panel(frame: &mut Frame, issue: Option<&Issue>, ready_ids: &std::collections::HashSet<String>, area: Rect, focused: bool, scroll: u16, edit_state: Option<&crate::EditState>) {
    // If we're in edit mode, render the edit UI
    if let Some(edit) = edit_state {
        render_edit_panel(frame, issue, edit, area);
        return;
    }

    let content = match issue {
        Some(issue) => format_issue_detail(issue, ready_ids),
        None => vec![Line::from("No issue selected")],
    };

    let border_color = if focused { Color::Cyan } else { Color::DarkGray };
    let title = if focused { " Details (j/k to scroll, e=edit, i=title) " } else { " Details " };

    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

fn render_edit_panel(frame: &mut Frame, issue: Option<&Issue>, edit: &crate::EditState, area: Rect) {
    let field_name = match edit.field {
        crate::EditField::Title => "Title",
        crate::EditField::Description => "Description",
    };

    let title = format!(" Editing {} (Esc=cancel, Ctrl+S=save) ", field_name);

    // Create the content lines
    let mut lines: Vec<Line> = Vec::new();

    // Show the issue ID
    if let Some(issue) = issue {
        lines.push(Line::from(vec![
            Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(issue.id.clone()),
        ]));
        lines.push(Line::from(""));
    }

    // Show field label
    lines.push(Line::from(Span::styled(
        format!("{}:", field_name),
        Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow),
    )));

    // Render the editable text with cursor
    // Split buffer into lines
    let buffer_lines: Vec<&str> = edit.buffer.split('\n').collect();

    for (line_idx, line_text) in buffer_lines.iter().enumerate() {
        if line_idx == edit.cursor_line {
            // This line has the cursor - render with cursor indicator
            let cursor_col = edit.cursor_col;
            let chars: Vec<char> = line_text.chars().collect();

            let before_cursor: String = chars[..cursor_col.min(chars.len())].iter().collect();
            let cursor_char = if cursor_col < chars.len() {
                chars[cursor_col].to_string()
            } else {
                " ".to_string()
            };
            let after_cursor: String = if cursor_col < chars.len() {
                chars[cursor_col + 1..].iter().collect()
            } else {
                String::new()
            };

            lines.push(Line::from(vec![
                Span::raw(before_cursor),
                Span::styled(cursor_char, Style::default().bg(Color::White).fg(Color::Black)),
                Span::raw(after_cursor),
            ]));
        } else {
            lines.push(Line::from(line_text.to_string()));
        }
    }

    // Add hint at bottom
    lines.push(Line::from(""));
    let hint = if edit.is_modified() {
        Line::from(Span::styled(
            "[Modified] Press Ctrl+S to save, Esc to cancel",
            Style::default().fg(Color::Yellow),
        ))
    } else {
        Line::from(Span::styled(
            "Press Ctrl+S to save, Esc to cancel",
            Style::default().fg(Color::DarkGray),
        ))
    };
    lines.push(hint);

    let paragraph = Paragraph::new(lines)
        .block(Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn format_issue_detail(issue: &Issue, ready_ids: &std::collections::HashSet<String>) -> Vec<Line<'static>> {
    let mut lines = vec![];

    // Title
    lines.push(Line::from(vec![
        Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(issue.title.clone()),
    ]));
    lines.push(Line::from(""));

    // ID and Type
    lines.push(Line::from(vec![
        Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(issue.id.clone()),
        Span::raw("  "),
        Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(issue.issue_type.clone()),
    ]));

    // Status and Priority
    let priority_color = match issue.priority {
        0 => Color::Red,
        1 => Color::Yellow,
        2 => Color::Green,
        3 => Color::Blue,
        _ => Color::DarkGray,
    };
    lines.push(Line::from(vec![
        Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(issue.status.clone()),
        Span::raw("  "),
        Span::styled("Priority: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("P{}", issue.priority), Style::default().fg(priority_color)),
    ]));

    // Ready/Blocked status (only for non-closed issues)
    if issue.status != "closed" {
        let is_ready = ready_ids.contains(&issue.id);
        if is_ready {
            lines.push(Line::from(Span::styled(
                "READY",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            )));
        } else {
            // Show blockers inline: "BLOCKED by id1, id2"
            let blocker_ids: Vec<String> = issue.dependencies
                .as_ref()
                .map(|deps| {
                    deps.iter()
                        .filter(|d| d.dependency_type.as_deref() != Some("related"))
                        .map(|d| d.id.clone())
                        .collect()
                })
                .unwrap_or_default();

            if blocker_ids.is_empty() {
                lines.push(Line::from(Span::styled(
                    "BLOCKED",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("BLOCKED", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" by {}", blocker_ids.join(", ")), Style::default().fg(Color::Red)),
                ]));
            }
        }
    }
    lines.push(Line::from(""));

    // Description (with markdown)
    if let Some(desc) = &issue.description {
        if !desc.is_empty() {
            lines.push(Line::from(Span::styled(
                "Description:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.extend(markdown_to_lines(desc));
            lines.push(Line::from(""));
        }
    }

    // Labels
    if let Some(labels) = &issue.labels {
        if !labels.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Labels: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(labels.join(", ")),
            ]));
            lines.push(Line::from(""));
        }
    }

    // Notes (with markdown)
    if let Some(notes) = &issue.notes {
        if !notes.is_empty() {
            lines.push(Line::from(Span::styled(
                "Notes:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.extend(markdown_to_lines(notes));
            lines.push(Line::from(""));
        }
    }

    // Dependencies
    if let Some(deps) = &issue.dependencies {
        if !deps.is_empty() {
            lines.push(Line::from(Span::styled(
                "Dependencies:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for dep in deps {
                let dep_type = dep.dependency_type.as_deref().unwrap_or("unknown");
                lines.push(Line::from(format!("  {} ({}) - {}", dep.id, dep_type, dep.title)));
            }
            lines.push(Line::from(""));
        }
    }

    // Dependents (children)
    if let Some(deps) = &issue.dependents {
        if !deps.is_empty() {
            lines.push(Line::from(Span::styled(
                "Children:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for dep in deps {
                lines.push(Line::from(format!("  {} - {}", dep.id, dep.title)));
            }
            lines.push(Line::from(""));
        }
    }

    // Timestamps
    lines.push(Line::from(vec![
        Span::styled("Created: ", Style::default().fg(Color::DarkGray)),
        Span::styled(issue.created_at.clone(), Style::default().fg(Color::DarkGray)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Updated: ", Style::default().fg(Color::DarkGray)),
        Span::styled(issue.updated_at.clone(), Style::default().fg(Color::DarkGray)),
    ]));

    lines
}

fn render_help_overlay(frame: &mut Frame) {
    let area = frame.area();

    // Center the help box
    let help_width = 50.min(area.width.saturating_sub(4));
    let help_height = 30.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(help_width)) / 2;
    let y = (area.height.saturating_sub(help_height)) / 2;
    let help_area = Rect::new(x, y, help_width, help_height);

    // Clear the area first
    frame.render_widget(Clear, help_area);

    let help_text = vec![
        Line::from(Span::styled("Tree Panel", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  j / ↓         Move down"),
        Line::from("  k / ↑         Move up"),
        Line::from("  g / Home      Go to top"),
        Line::from("  G / End       Go to bottom"),
        Line::from("  l / → / Enter Expand / focus details"),
        Line::from("  h / ←         Collapse / go to parent"),
        Line::from("  Space         Toggle expand/collapse"),
        Line::from("  Tab           Toggle expand/collapse all"),
        Line::from(""),
        Line::from(Span::styled("Details Panel", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  j / k         Scroll up/down"),
        Line::from("  g / G         Top/bottom"),
        Line::from("  h / ←         Return to tree"),
        Line::from("  e / i         Edit description / title"),
        Line::from("  Click         Focus panel"),
        Line::from(""),
        Line::from(Span::styled("Edit Mode", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  Esc           Cancel editing"),
        Line::from("  Ctrl+S        Save changes"),
        Line::from("  Tab/Shift+Tab Navigate fields"),
        Line::from(""),
        Line::from(Span::styled("Global", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  c             Toggle show/hide closed"),
        Line::from("  d             Toggle Epics/Deps view"),
        Line::from("  r             Refresh data"),
        Line::from("  ?             Toggle this help"),
        Line::from("  q / Ctrl+C    Quit"),
        Line::from(""),
        Line::from(Span::styled("Colors: ", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("  Green", Style::default().fg(Color::Green)),
            Span::raw("=Ready "),
            Span::styled("Red", Style::default().fg(Color::Red)),
            Span::raw("=Blocked "),
            Span::styled("Gray", Style::default().fg(Color::DarkGray)),
            Span::raw("=Closed"),
        ]),
    ];

    let help_paragraph = Paragraph::new(help_text)
        .block(Block::default()
            .title(" Help (? to close) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)))
        .style(Style::default().bg(Color::Black));

    frame.render_widget(help_paragraph, help_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal, buffer::Buffer};
    use std::collections::HashSet;

    /// Convert buffer to a string for snapshot comparison
    fn buffer_to_string(buffer: &Buffer) -> String {
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                output.push_str(buffer[(x, y)].symbol());
            }
            // Trim trailing whitespace from each line
            output = output.trim_end().to_string();
            output.push('\n');
        }
        output
    }

    // ==================== Markdown Parsing Tests ====================

    #[test]
    fn test_markdown_header_h1() {
        let lines = markdown_to_lines("# Header One");
        assert_eq!(lines.len(), 1);
        // Check the text content
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "Header One");
    }

    #[test]
    fn test_markdown_header_h2() {
        let lines = markdown_to_lines("## Header Two");
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "Header Two");
    }

    #[test]
    fn test_markdown_header_h3() {
        let lines = markdown_to_lines("### Header Three");
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "Header Three");
    }

    #[test]
    fn test_markdown_code_block() {
        let input = "```rust\nlet x = 1;\n```";
        let lines = markdown_to_lines(input);
        // Should have: language tag line + code line
        assert_eq!(lines.len(), 2);
        let lang_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(lang_text.contains("rust"));
        let code_text: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(code_text.contains("let x = 1;"));
    }

    #[test]
    fn test_markdown_code_block_no_language() {
        let input = "```\ncode here\n```";
        let lines = markdown_to_lines(input);
        // No language tag, just the code
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("code here"));
    }

    #[test]
    fn test_markdown_blockquote() {
        let lines = markdown_to_lines("> This is a quote");
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("This is a quote"));
        assert!(text.contains("│")); // Quote marker
    }

    #[test]
    fn test_markdown_unordered_list() {
        let lines = markdown_to_lines("- Item one\n- Item two");
        assert_eq!(lines.len(), 2);
        let text1: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        let text2: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text1.contains("•") && text1.contains("Item one"));
        assert!(text2.contains("•") && text2.contains("Item two"));
    }

    #[test]
    fn test_markdown_ordered_list() {
        let lines = markdown_to_lines("1. First\n2. Second");
        assert_eq!(lines.len(), 2);
        let text1: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        let text2: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text1.contains("1.") && text1.contains("First"));
        assert!(text2.contains("2.") && text2.contains("Second"));
    }

    #[test]
    fn test_markdown_horizontal_rule() {
        for rule in ["---", "***", "___"] {
            let lines = markdown_to_lines(rule);
            assert_eq!(lines.len(), 1);
            let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.contains("────")); // Should render as line
        }
    }

    #[test]
    fn test_markdown_table() {
        let input = "| Col1 | Col2 |\n|------|------|\n| A    | B    |";
        let lines = markdown_to_lines(input);
        assert_eq!(lines.len(), 3);
        // Table rows should contain the pipe character (rendered as │)
        let header: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(header.contains("Col1") && header.contains("Col2"));
    }

    #[test]
    fn test_markdown_inline_code() {
        let lines = markdown_to_lines("Use `code` here");
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("code"));
    }

    #[test]
    fn test_markdown_bold() {
        let lines = markdown_to_lines("This is **bold** text");
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "This is bold text");
    }

    #[test]
    fn test_markdown_italic() {
        let lines = markdown_to_lines("This is *italic* text");
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "This is italic text");
    }

    #[test]
    fn test_markdown_link() {
        let lines = markdown_to_lines("Click [here](https://example.com)");
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("here"));
        // URL should not appear in rendered text
        assert!(!text.contains("https://"));
    }

    // ==================== Snapshot Tests ====================

    fn make_test_issue(id: &str, title: &str, status: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: title.to_string(),
            description: Some("Test description".to_string()),
            status: status.to_string(),
            priority: 2,
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

    fn make_rich_test_issue() -> Issue {
        use crate::bd::Dependency;
        Issue {
            id: "bsv-rich".to_string(),
            title: "Rich Test Issue".to_string(),
            description: Some("# Header\n\nWith **bold** and `code`".to_string()),
            status: "open".to_string(),
            priority: 1, // Yellow priority
            issue_type: "feature".to_string(),
            created_at: "2024-01-01".to_string(),
            created_by: Some("tester".to_string()),
            updated_at: "2024-01-02".to_string(),
            labels: Some(vec!["bug".to_string(), "urgent".to_string()]),
            parent: Some("bsv-parent".to_string()),
            dependencies: Some(vec![
                Dependency {
                    id: "bsv-dep1".to_string(),
                    title: "Blocking Issue".to_string(),
                    dependency_type: Some("blocks".to_string()),
                },
            ]),
            dependents: Some(vec![
                Dependency {
                    id: "bsv-child1".to_string(),
                    title: "Child Issue".to_string(),
                    dependency_type: None,
                },
            ]),
            notes: Some("- Item one\n  - Nested item\n- Item two".to_string()),
            design: None,
            acceptance_criteria: None,
        }
    }

    #[test]
    fn test_detail_panel_snapshot() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let issue = make_test_issue("bsv-123", "Test Issue Title", "open");
        let ready_ids: HashSet<String> = HashSet::new();

        terminal.draw(|frame| {
            render_detail_panel(frame, Some(&issue), &ready_ids, frame.area(), true, 0, None);
        }).unwrap();

        let output = buffer_to_string(terminal.backend().buffer());

        // Verify key elements are present
        assert!(output.contains("Details"));
        assert!(output.contains("Test Issue Title"));
        assert!(output.contains("bsv-123"));
        assert!(output.contains("BLOCKED")); // Not in ready_ids
    }

    #[test]
    fn test_detail_panel_ready_issue() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let issue = make_test_issue("bsv-456", "Ready Issue", "open");
        let mut ready_ids: HashSet<String> = HashSet::new();
        ready_ids.insert("bsv-456".to_string());

        terminal.draw(|frame| {
            render_detail_panel(frame, Some(&issue), &ready_ids, frame.area(), true, 0, None);
        }).unwrap();

        let output = buffer_to_string(terminal.backend().buffer());
        assert!(output.contains("READY"));
    }

    #[test]
    fn test_detail_panel_closed_issue() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let issue = make_test_issue("bsv-789", "Closed Issue", "closed");
        let ready_ids: HashSet<String> = HashSet::new();

        terminal.draw(|frame| {
            render_detail_panel(frame, Some(&issue), &ready_ids, frame.area(), true, 0, None);
        }).unwrap();

        let output = buffer_to_string(terminal.backend().buffer());
        // Closed issues should not show READY or BLOCKED
        assert!(!output.contains("READY"));
        assert!(!output.contains("BLOCKED"));
    }

    #[test]
    fn test_tree_panel_snapshot() {
        use crate::HierarchyMode;

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let issues = vec![
            make_test_issue("bsv-a", "First Issue", "open"),
            make_test_issue("bsv-b", "Second Issue", "open"),
            make_test_issue("bsv-a.1", "Child Issue", "open"),
        ];

        let mut expanded = HashSet::new();
        expanded.insert("bsv-a".to_string());
        let ready_ids = HashSet::new();

        let tree = IssueTree::from_issues(issues, expanded, HashSet::new(), ready_ids, HierarchyMode::IdBased);

        terminal.draw(|frame| {
            render_tree_panel(frame, &tree, frame.area(), true);
        }).unwrap();

        let output = buffer_to_string(terminal.backend().buffer());

        // Verify structure
        assert!(output.contains("Issues"));
        assert!(output.contains("bsv-a"));
        assert!(output.contains("First Issue"));
        assert!(output.contains("bsv-a.1")); // Child should be visible when expanded
        assert!(output.contains("Child Issue"));
    }

    #[test]
    fn test_help_overlay_snapshot() {
        let backend = TestBackend::new(60, 35);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| {
            render_help_overlay(frame);
        }).unwrap();

        let output = buffer_to_string(terminal.backend().buffer());

        // Verify help content
        assert!(output.contains("Tree Panel"));
        assert!(output.contains("Details Panel"));
        assert!(output.contains("Global"));
        assert!(output.contains("j / k"));
        assert!(output.contains("Quit"));
    }

    #[test]
    fn test_detail_panel_rich_issue() {
        let backend = TestBackend::new(70, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let issue = make_rich_test_issue();
        let ready_ids: HashSet<String> = HashSet::new();

        terminal.draw(|frame| {
            render_detail_panel(frame, Some(&issue), &ready_ids, frame.area(), true, 0, None);
        }).unwrap();

        let output = buffer_to_string(terminal.backend().buffer());

        // Verify rich content is rendered
        assert!(output.contains("Rich Test Issue"));
        assert!(output.contains("bsv-rich"));
        assert!(output.contains("BLOCKED")); // Has dependencies
        assert!(output.contains("bsv-dep1")); // Blocker ID shown
        assert!(output.contains("Labels:"));
        assert!(output.contains("bug"));
        assert!(output.contains("Notes:"));
        assert!(output.contains("Dependencies:"));
        assert!(output.contains("Children:"));
        assert!(output.contains("bsv-child1"));
    }

    #[test]
    fn test_detail_panel_priorities() {
        // Test different priority colors are rendered
        for priority in [0, 1, 2, 3, 4] {
            let backend = TestBackend::new(60, 15);
            let mut terminal = Terminal::new(backend).unwrap();

            let mut issue = make_test_issue("bsv-p", "Priority Test", "closed");
            issue.priority = priority;
            let ready_ids: HashSet<String> = HashSet::new();

            terminal.draw(|frame| {
                render_detail_panel(frame, Some(&issue), &ready_ids, frame.area(), true, 0, None);
            }).unwrap();

            let output = buffer_to_string(terminal.backend().buffer());
            assert!(output.contains(&format!("P{}", priority)));
        }
    }

    #[test]
    fn test_markdown_nested_list() {
        let lines = markdown_to_lines("- Top level\n  - Nested item\n- Another top");
        assert_eq!(lines.len(), 3);
        let nested: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(nested.contains("◦")); // Nested bullet
        assert!(nested.contains("Nested item"));
    }

    #[test]
    fn test_full_render_function() {
        use crate::HierarchyMode;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let issues = vec![
            make_test_issue("bsv-a", "First Issue", "open"),
            make_test_issue("bsv-b", "Second Issue", "open"),
        ];
        let expanded = HashSet::new();
        let mut ready_ids = HashSet::new();
        ready_ids.insert("bsv-a".to_string());

        let tree = IssueTree::from_issues(issues, expanded, HashSet::new(), ready_ids, HierarchyMode::IdBased);
        let selected = make_test_issue("bsv-a", "First Issue", "open");

        terminal.draw(|frame| {
            render(frame, &tree, Some(&selected), false, crate::Focus::Tree, 0, None, 0.4);
        }).unwrap();

        let output = buffer_to_string(terminal.backend().buffer());

        // Verify both panels rendered
        assert!(output.contains("Issues")); // Tree panel title
        assert!(output.contains("Details")); // Detail panel title
        assert!(output.contains("bsv-a"));
        assert!(output.contains("First Issue"));
    }

    #[test]
    fn test_full_render_with_help() {
        use crate::HierarchyMode;

        let backend = TestBackend::new(100, 35);
        let mut terminal = Terminal::new(backend).unwrap();

        let issues = vec![make_test_issue("bsv-a", "Test", "open")];
        let tree = IssueTree::from_issues(issues, HashSet::new(), HashSet::new(), HashSet::new(), HierarchyMode::IdBased);

        terminal.draw(|frame| {
            render(frame, &tree, None, true, crate::Focus::Tree, 0, None, 0.4); // show_help = true
        }).unwrap();

        let output = buffer_to_string(terminal.backend().buffer());

        // Help overlay should be visible
        assert!(output.contains("Help"));
        assert!(output.contains("Quit"));
    }
}
