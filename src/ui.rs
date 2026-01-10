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
    text.lines()
        .map(|line| markdown_line_to_spans(line))
        .collect()
}

/// Convert a single line of markdown to styled Spans
fn markdown_line_to_spans(line: &str) -> Line<'static> {
    // Handle headers
    if line.starts_with("### ") {
        return Line::from(Span::styled(
            line[4..].to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }
    if line.starts_with("## ") {
        return Line::from(Span::styled(
            line[3..].to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }
    if line.starts_with("# ") {
        return Line::from(Span::styled(
            line[2..].to_string(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }

    // Handle list items (just pass through with slight styling)
    if line.starts_with("- ") || line.starts_with("* ") {
        let rest = &line[2..];
        return Line::from(vec![
            Span::styled("• ", Style::default().fg(Color::Cyan)),
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

pub fn render(frame: &mut Frame, tree: &IssueTree, selected_details: Option<&Issue>, show_help: bool) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(frame.area());

    render_tree_panel(frame, tree, chunks[0]);

    // Use full details if available (has dependencies), otherwise fall back to tree node
    let issue_for_details = selected_details.or_else(|| tree.selected_node().map(|n| &n.issue));
    render_detail_panel(frame, issue_for_details, &tree.ready_ids, chunks[1]);

    if show_help {
        render_help_overlay(frame);
    }
}

fn render_tree_panel(frame: &mut Frame, tree: &IssueTree, area: Rect) {
    let items: Vec<ListItem> = tree.visible_items
        .iter()
        .enumerate()
        .filter_map(|(idx, id)| {
            tree.nodes.get(id).map(|node| {
                let is_selected = idx == tree.cursor;
                let has_children = !node.children.is_empty();
                let is_expanded = tree.is_expanded(id);
                let is_closed = node.issue.status == "closed";
                let is_ready = tree.ready_ids.contains(id);

                // Build the tree prefix with indentation
                let indent = "  ".repeat(node.depth);
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

                let line = Line::from(vec![
                    Span::styled(format!("{}{}", indent, icon), text_style),
                    Span::styled(format!("{} ", node.issue.id), Style::default().fg(Color::DarkGray)),
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

    let list = List::new(items)
        .block(Block::default()
            .title(" Issues ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)));

    frame.render_widget(list, area);
}

fn render_detail_panel(frame: &mut Frame, issue: Option<&Issue>, ready_ids: &std::collections::HashSet<String>, area: Rect) {
    let content = match issue {
        Some(issue) => format_issue_detail(issue, ready_ids),
        None => vec![Line::from("No issue selected")],
    };

    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .title(" Details ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)))
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
    let help_height = 20.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(help_width)) / 2;
    let y = (area.height.saturating_sub(help_height)) / 2;
    let help_area = Rect::new(x, y, help_width, help_height);

    // Clear the area first
    frame.render_widget(Clear, help_area);

    let help_text = vec![
        Line::from(Span::styled("Keybindings", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  j / ↓         Move down"),
        Line::from("  k / ↑         Move up"),
        Line::from("  g / Home      Go to top"),
        Line::from("  G / End       Go to bottom"),
        Line::from("  l / → / Enter Expand"),
        Line::from("  h / ←         Collapse / go to parent"),
        Line::from("  Space         Toggle expand/collapse"),
        Line::from("  Tab           Toggle expand/collapse all"),
        Line::from("  c             Toggle show/hide closed"),
        Line::from("  r             Refresh data"),
        Line::from("  ?             Toggle this help"),
        Line::from("  q / Ctrl+C    Quit"),
        Line::from(""),
        Line::from(Span::styled("Colors", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("  Green", Style::default().fg(Color::Green)),
            Span::raw(" = Ready  "),
            Span::styled("Red", Style::default().fg(Color::Red)),
            Span::raw(" = Blocked  "),
            Span::styled("Gray", Style::default().fg(Color::DarkGray)),
            Span::raw(" = Closed"),
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
