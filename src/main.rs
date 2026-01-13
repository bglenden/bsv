mod bd;
mod state;
mod tree;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Tree,
    Details,
}

/// Hierarchy mode for the tree view
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum HierarchyMode {
    #[default]
    IdBased,        // Current: dotted ID hierarchy (bsv-epic.1 is child of bsv-epic)
    DependencyBased, // New: dependency chain hierarchy (blocked issues are children)
}

/// Which field is currently being edited
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditField {
    Title,
    Description,
}

/// State for inline editing of an issue
#[derive(Debug, Clone)]
pub struct EditState {
    /// The issue ID being edited
    pub issue_id: String,
    /// Which field is being edited
    pub field: EditField,
    /// The original value (for cancel/revert)
    pub original: String,
    /// The current edited value
    pub buffer: String,
    /// Cursor position within the buffer (byte offset)
    pub cursor: usize,
    /// For multiline: which line the cursor is on (for display)
    pub cursor_line: usize,
    /// For multiline: column position within the line
    pub cursor_col: usize,
}

impl EditState {
    /// Create a new edit state for a field
    pub fn new(issue_id: String, field: EditField, value: String) -> Self {
        let cursor = value.len();
        let (cursor_line, cursor_col) = Self::compute_line_col(&value, cursor);
        EditState {
            issue_id,
            field,
            original: value.clone(),
            buffer: value,
            cursor,
            cursor_line,
            cursor_col,
        }
    }

    /// Compute line and column from byte offset
    fn compute_line_col(text: &str, byte_offset: usize) -> (usize, usize) {
        let prefix = &text[..byte_offset.min(text.len())];
        let lines: Vec<&str> = prefix.split('\n').collect();
        let line = lines.len().saturating_sub(1);
        let col = lines.last().map(|l| l.chars().count()).unwrap_or(0);
        (line, col)
    }

    /// Update cursor line/col after cursor movement
    fn update_cursor_position(&mut self) {
        let (line, col) = Self::compute_line_col(&self.buffer, self.cursor);
        self.cursor_line = line;
        self.cursor_col = col;
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.update_cursor_position();
    }

    /// Insert a string at cursor position
    pub fn insert_str(&mut self, s: &str) {
        self.buffer.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.update_cursor_position();
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            // Find the previous character boundary
            let prev_char_start = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.buffer.remove(prev_char_start);
            self.cursor = prev_char_start;
            self.update_cursor_position();
        }
    }

    /// Delete character at cursor (delete key)
    pub fn delete_char_at(&mut self) {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
            self.update_cursor_position();
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.update_cursor_position();
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            let next = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.buffer.len());
            self.cursor = next;
            self.update_cursor_position();
        }
    }

    /// Move cursor to start of line (for multiline) or start of buffer (for single line)
    pub fn move_to_line_start(&mut self) {
        // Find the start of the current line
        let before_cursor = &self.buffer[..self.cursor];
        if let Some(newline_pos) = before_cursor.rfind('\n') {
            self.cursor = newline_pos + 1;
        } else {
            self.cursor = 0;
        }
        self.update_cursor_position();
    }

    /// Move cursor to end of line (for multiline) or end of buffer (for single line)
    pub fn move_to_line_end(&mut self) {
        // Find the end of the current line
        let after_cursor = &self.buffer[self.cursor..];
        if let Some(newline_pos) = after_cursor.find('\n') {
            self.cursor += newline_pos;
        } else {
            self.cursor = self.buffer.len();
        }
        self.update_cursor_position();
    }

    /// Move cursor up one line (for multiline fields)
    pub fn move_up(&mut self) {
        if self.cursor_line > 0 {
            let lines: Vec<&str> = self.buffer.split('\n').collect();
            let prev_line = lines[self.cursor_line - 1];
            let target_col = self.cursor_col.min(prev_line.chars().count());

            // Calculate byte offset for previous line
            let mut offset = 0;
            for (i, line) in lines.iter().enumerate() {
                if i == self.cursor_line - 1 {
                    // Add target column offset
                    offset += line.char_indices()
                        .nth(target_col)
                        .map(|(i, _)| i)
                        .unwrap_or(line.len());
                    break;
                }
                offset += line.len() + 1; // +1 for newline
            }
            self.cursor = offset;
            self.update_cursor_position();
        }
    }

    /// Move cursor down one line (for multiline fields)
    pub fn move_down(&mut self) {
        let lines: Vec<&str> = self.buffer.split('\n').collect();
        if self.cursor_line < lines.len() - 1 {
            let next_line = lines[self.cursor_line + 1];
            let target_col = self.cursor_col.min(next_line.chars().count());

            // Calculate byte offset for next line
            let mut offset = 0;
            for (i, line) in lines.iter().enumerate() {
                if i == self.cursor_line + 1 {
                    // Add target column offset
                    offset += line.char_indices()
                        .nth(target_col)
                        .map(|(i, _)| i)
                        .unwrap_or(line.len());
                    break;
                }
                offset += line.len() + 1; // +1 for newline
            }
            self.cursor = offset;
            self.update_cursor_position();
        }
    }

    /// Check if the buffer has been modified from the original
    pub fn is_modified(&self) -> bool {
        self.buffer != self.original
    }

    /// Revert to the original value
    pub fn revert(&mut self) {
        self.buffer = self.original.clone();
        self.cursor = self.buffer.len();
        self.update_cursor_position();
    }
}

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::prelude::*;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use state::save_expanded;
use tree::IssueTree;

struct App {
    tree: IssueTree,
    should_quit: bool,
    show_help: bool,
    selected_details: Option<bd::Issue>,
    last_selected_id: Option<String>,
    focus: Focus,
    detail_scroll: u16,
    /// Active edit state (None when not editing)
    edit_state: Option<EditState>,
    /// Current hierarchy view mode
    hierarchy_mode: HierarchyMode,
}

impl App {
    fn new() -> Result<Self> {
        let issues = bd::list_issues_with_details()?;
        let (expanded, dep_expanded, hierarchy_mode) = state::load_tree_state();
        let ready_ids = bd::get_ready_ids().unwrap_or_default();
        let tree = IssueTree::from_issues(issues, expanded, dep_expanded, ready_ids, hierarchy_mode);

        // Fetch details for initially selected issue
        let selected_details = tree.selected_id()
            .and_then(|id| bd::get_issue_details(id).ok().flatten());
        let last_selected_id = tree.selected_id().map(|s| s.to_string());

        Ok(App {
            tree,
            should_quit: false,
            show_help: false,
            selected_details,
            last_selected_id,
            focus: Focus::Tree,
            detail_scroll: 0,
            edit_state: None,
            hierarchy_mode,
        })
    }

    fn update_selected_details(&mut self) {
        let current_id = self.tree.selected_id().map(|s| s.to_string());
        if current_id != self.last_selected_id {
            self.selected_details = current_id.as_ref()
                .and_then(|id| bd::get_issue_details(id).ok().flatten());
            self.last_selected_id = current_id;
            self.detail_scroll = 0; // Reset scroll when selection changes
        }
    }

    fn scroll_details(&mut self, delta: i16) {
        let new_scroll = self.detail_scroll as i16 + delta;
        self.detail_scroll = new_scroll.max(0) as u16;
    }

    fn refresh(&mut self) {
        // Preserve current state
        let selected_id = self.tree.selected_id().map(|s| s.to_string());
        let show_closed = self.tree.show_closed;

        if let Ok(issues) = bd::list_issues_with_details() {
            let ready_ids = bd::get_ready_ids().unwrap_or_default();
            self.tree = IssueTree::from_issues(
                issues,
                self.tree.expanded.clone(),
                self.tree.dep_expanded.clone(),
                ready_ids,
                self.hierarchy_mode,
            );
            self.tree.show_closed = show_closed;
            self.tree.rebuild_visible();

            // Restore cursor to previously selected item if it still exists
            if let Some(id) = selected_id {
                if let Some(pos) = self.tree.visible_items.iter().position(|x| x == &id) {
                    self.tree.cursor = pos;
                }
            }

            // Force refresh of selected details
            self.last_selected_id = None;
            self.update_selected_details();
        }
    }

    /// Check if we're currently in edit mode
    fn is_editing(&self) -> bool {
        self.edit_state.is_some()
    }

    /// Toggle between ID-based and Dependency-based hierarchy views
    fn toggle_hierarchy_mode(&mut self) {
        self.hierarchy_mode = match self.hierarchy_mode {
            HierarchyMode::IdBased => HierarchyMode::DependencyBased,
            HierarchyMode::DependencyBased => HierarchyMode::IdBased,
        };
        self.tree.set_hierarchy_mode(self.hierarchy_mode);
        // Save the updated mode
        let _ = state::save_tree_state(
            &self.tree.expanded,
            &self.tree.dep_expanded,
            self.hierarchy_mode,
        );
    }

    /// Start editing a field of the current issue
    fn start_edit(&mut self, field: EditField) {
        if let Some(issue) = &self.selected_details {
            let value = match field {
                EditField::Title => issue.title.clone(),
                EditField::Description => issue.description.clone().unwrap_or_default(),
            };
            self.edit_state = Some(EditState::new(
                issue.id.clone(),
                field,
                value,
            ));
            self.focus = Focus::Details;
        }
    }

    /// Cancel editing and discard changes
    fn cancel_edit(&mut self) {
        self.edit_state = None;
    }

    /// Save the current edit using bd update
    fn save_edit(&mut self) -> Result<()> {
        if let Some(ref edit) = self.edit_state {
            if edit.is_modified() {
                match edit.field {
                    EditField::Title => {
                        bd::update_issue_title(&edit.issue_id, &edit.buffer)?;
                    }
                    EditField::Description => {
                        bd::update_issue_description(&edit.issue_id, &edit.buffer)?;
                    }
                }
                // Refresh to pick up the changes
                self.last_selected_id = None; // Force refresh of details
                self.update_selected_details();
            }
        }
        self.edit_state = None;
        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        // If in edit mode, handle edit keys first
        if self.is_editing() {
            self.handle_edit_key(code, modifiers);
            return;
        }

        // Handle focus-independent keys first
        match (code, modifiers) {
            // Quit
            (KeyCode::Char('q'), KeyModifiers::NONE) |
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return;
            }

            // Help
            (KeyCode::Char('?'), KeyModifiers::NONE) |
            (KeyCode::Char('?'), KeyModifiers::SHIFT) => {
                self.show_help = !self.show_help;
                return;
            }

            // Escape - close help or return to tree
            (KeyCode::Esc, KeyModifiers::NONE) => {
                if self.show_help {
                    self.show_help = false;
                } else {
                    self.focus = Focus::Tree;
                }
                return;
            }

            // Refresh data
            (KeyCode::Char('r'), KeyModifiers::NONE) => {
                self.refresh();
                return;
            }

            // Toggle show/hide closed (works from either panel)
            (KeyCode::Char('c'), KeyModifiers::NONE) => {
                self.tree.toggle_show_closed();
                return;
            }

            // Toggle hierarchy mode (ID-based vs Dependency-based)
            (KeyCode::Char('d'), KeyModifiers::NONE) => {
                self.toggle_hierarchy_mode();
                return;
            }

            _ => {}
        }

        // Handle focus-specific keys
        match self.focus {
            Focus::Tree => self.handle_tree_key(code, modifiers),
            Focus::Details => self.handle_details_key(code, modifiers),
        }
    }

    fn handle_edit_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            // Escape cancels editing
            (KeyCode::Esc, KeyModifiers::NONE) => {
                self.cancel_edit();
            }

            // Ctrl+S or Ctrl+Enter saves
            (KeyCode::Char('s'), KeyModifiers::CONTROL) |
            (KeyCode::Enter, KeyModifiers::CONTROL) => {
                let _ = self.save_edit();
            }

            // Enter in title field saves and moves to description
            // Enter in description field inserts newline
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    match edit.field {
                        EditField::Title => {
                            // Save title and start editing description
                            let _ = self.save_edit();
                            self.start_edit(EditField::Description);
                        }
                        EditField::Description => {
                            // Insert newline in description
                            edit.insert_char('\n');
                        }
                    }
                }
            }

            // Backspace deletes character before cursor
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    edit.delete_char_before();
                }
            }

            // Delete key deletes character at cursor
            (KeyCode::Delete, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    edit.delete_char_at();
                }
            }

            // Arrow keys for cursor movement
            (KeyCode::Left, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    edit.move_left();
                }
            }
            (KeyCode::Right, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    edit.move_right();
                }
            }
            (KeyCode::Up, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    if edit.field == EditField::Description {
                        edit.move_up();
                    }
                }
            }
            (KeyCode::Down, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    if edit.field == EditField::Description {
                        edit.move_down();
                    }
                }
            }

            // Home/End for line navigation
            (KeyCode::Home, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    edit.move_to_line_start();
                }
            }
            (KeyCode::End, KeyModifiers::NONE) => {
                if let Some(ref mut edit) = self.edit_state {
                    edit.move_to_line_end();
                }
            }

            // Regular character input
            (KeyCode::Char(c), KeyModifiers::NONE) |
            (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                if let Some(ref mut edit) = self.edit_state {
                    edit.insert_char(c);
                }
            }

            // Tab: in title mode, move to description; in description, insert spaces
            (KeyCode::Tab, KeyModifiers::NONE) => {
                if let Some(ref edit) = self.edit_state {
                    match edit.field {
                        EditField::Title => {
                            // Save title and move to description
                            let _ = self.save_edit();
                            self.start_edit(EditField::Description);
                        }
                        EditField::Description => {
                            // Insert spaces
                            if let Some(ref mut edit) = self.edit_state {
                                edit.insert_str("    ");
                            }
                        }
                    }
                }
            }

            // Shift+Tab: go back to title from description
            (KeyCode::BackTab, KeyModifiers::SHIFT) |
            (KeyCode::BackTab, KeyModifiers::NONE) => {
                if let Some(ref edit) = self.edit_state {
                    if edit.field == EditField::Description {
                        // Save description and move back to title
                        let _ = self.save_edit();
                        self.start_edit(EditField::Title);
                    }
                }
            }

            _ => {}
        }
    }

    fn handle_tree_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            // Movement - vim style
            (KeyCode::Char('j'), KeyModifiers::NONE) |
            (KeyCode::Down, KeyModifiers::NONE) => {
                self.tree.move_down();
            }
            (KeyCode::Char('k'), KeyModifiers::NONE) |
            (KeyCode::Up, KeyModifiers::NONE) => {
                self.tree.move_up();
            }

            // Top/Bottom - vim style
            (KeyCode::Char('g'), KeyModifiers::NONE) => {
                self.tree.move_to_top();
            }
            (KeyCode::Char('G'), KeyModifiers::SHIFT) |
            (KeyCode::Char('G'), KeyModifiers::NONE) => {
                self.tree.move_to_bottom();
            }
            (KeyCode::Home, KeyModifiers::NONE) => {
                self.tree.move_to_top();
            }
            (KeyCode::End, KeyModifiers::NONE) => {
                self.tree.move_to_bottom();
            }

            // Expand/Collapse
            (KeyCode::Char('l'), KeyModifiers::NONE) |
            (KeyCode::Right, KeyModifiers::NONE) => {
                self.tree.expand();
                let _ = save_expanded(&self.tree.expanded);
            }
            (KeyCode::Char('h'), KeyModifiers::NONE) |
            (KeyCode::Left, KeyModifiers::NONE) => {
                self.tree.collapse();
                let _ = save_expanded(&self.tree.expanded);
            }
            (KeyCode::Char(' '), KeyModifiers::NONE) => {
                self.tree.toggle_expand();
                let _ = save_expanded(&self.tree.expanded);
            }

            // Enter focuses details panel
            (KeyCode::Enter, KeyModifiers::NONE) => {
                self.focus = Focus::Details;
            }

            // Toggle expand/collapse all
            (KeyCode::Tab, KeyModifiers::NONE) => {
                self.tree.toggle_expand_all();
                let _ = save_expanded(&self.tree.expanded);
            }

            _ => {}
        }
    }

    fn handle_details_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            // Scroll details
            (KeyCode::Char('j'), KeyModifiers::NONE) |
            (KeyCode::Down, KeyModifiers::NONE) => {
                self.scroll_details(1);
            }
            (KeyCode::Char('k'), KeyModifiers::NONE) |
            (KeyCode::Up, KeyModifiers::NONE) => {
                self.scroll_details(-1);
            }

            // Page up/down
            (KeyCode::PageDown, KeyModifiers::NONE) |
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                self.scroll_details(10);
            }
            (KeyCode::PageUp, KeyModifiers::NONE) |
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.scroll_details(-10);
            }

            // Top/Bottom
            (KeyCode::Char('g'), KeyModifiers::NONE) |
            (KeyCode::Home, KeyModifiers::NONE) => {
                self.detail_scroll = 0;
            }
            (KeyCode::Char('G'), KeyModifiers::SHIFT) |
            (KeyCode::Char('G'), KeyModifiers::NONE) |
            (KeyCode::End, KeyModifiers::NONE) => {
                self.detail_scroll = u16::MAX; // Will be clamped in render
            }

            // h or left returns to tree
            (KeyCode::Char('h'), KeyModifiers::NONE) |
            (KeyCode::Left, KeyModifiers::NONE) => {
                self.focus = Focus::Tree;
            }

            // 'e' starts editing description
            (KeyCode::Char('e'), KeyModifiers::NONE) => {
                self.start_edit(EditField::Description);
            }

            // 'i' starts editing title (like vim insert)
            (KeyCode::Char('i'), KeyModifiers::NONE) => {
                self.start_edit(EditField::Title);
            }

            _ => {}
        }
    }

    fn handle_mouse(&mut self, column: u16, row: u16, screen_width: u16, screen_height: u16) {
        // Roughly split at 40% for tree panel
        let tree_width = screen_width * 40 / 100;
        if column < tree_width {
            self.focus = Focus::Tree;
            // Click on an issue to select it (account for border)
            if row > 0 && row < screen_height - 1 {
                let clicked_index = (row - 1) as usize;
                if clicked_index < self.tree.visible_items.len() {
                    self.tree.cursor = clicked_index;
                }
            }
        } else {
            self.focus = Focus::Details;
        }
    }
}

fn print_help() {
    println!("bsv - beads simple viewer");
    println!();
    println!("USAGE:");
    println!("    bsv [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --help     Print this help message");
    println!("    --debug    Dump tree structure and exit");
    println!();
    println!("TREE PANEL:");
    println!("    j/↓        Move cursor down");
    println!("    k/↑        Move cursor up");
    println!("    g/Home     Go to top");
    println!("    G/End      Go to bottom");
    println!("    l/→/Enter  Expand node / focus details");
    println!("    h/←        Collapse node (or go to parent)");
    println!("    Space      Toggle expand/collapse");
    println!("    Tab        Toggle expand/collapse all");
    println!();
    println!("DETAILS PANEL:");
    println!("    j/k        Scroll up/down");
    println!("    g/G        Go to top/bottom");
    println!("    h/←        Return to tree");
    println!("    e          Edit description");
    println!("    i          Edit title");
    println!("    Click      Focus panel");
    println!();
    println!("EDIT MODE:");
    println!("    Esc        Cancel editing");
    println!("    Ctrl+S     Save changes");
    println!("    Tab        Move to description (from title)");
    println!("    Shift+Tab  Move to title (from description)");
    println!("    Enter      Newline (description) / Save & next (title)");
    println!();
    println!("GLOBAL:");
    println!("    c          Toggle show/hide closed");
    println!("    d          Toggle Epics/Deps view");
    println!("    r          Refresh data from bd");
    println!("    ?          Show help overlay");
    println!("    q/Ctrl+C   Quit");
    println!();
    println!("COLORS:");
    println!("    Green      Ready (no blockers)");
    println!("    Red        Blocked");
    println!("    Gray       Closed");
}

fn find_beads_dir() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let beads_dir = dir.join(".beads");
        if beads_dir.is_dir() {
            return Some(beads_dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Help mode
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }

    // Debug mode: dump tree and exit
    if args.iter().any(|a| a == "--debug") {
        let issues = bd::list_issues_with_details()?;
        let (expanded, dep_expanded, hierarchy_mode) = state::load_tree_state();
        let ready_ids = bd::get_ready_ids().unwrap_or_default();
        let tree = IssueTree::from_issues(issues, expanded, dep_expanded, ready_ids, hierarchy_mode);
        tree.debug_dump();
        return Ok(());
    }

    // Set up file watcher for .beads directory
    let (fs_tx, fs_rx) = mpsc::channel();
    let mut _watcher: Option<RecommendedWatcher> = None;

    if let Some(beads_dir) = find_beads_dir() {
        let watcher_result = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if res.is_ok() {
                    let _ = fs_tx.send(());
                }
            },
            Config::default(),
        );

        if let Ok(mut watcher) = watcher_result {
            let _ = watcher.watch(&beads_dir, RecursiveMode::Recursive);
            _watcher = Some(watcher);
        }
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;
    let mut last_refresh = Instant::now();
    let refresh_cooldown = Duration::from_millis(500);

    // Main loop
    loop {
        let size = terminal.size()?;
        terminal.draw(|frame| {
            ui::render(frame, &app.tree, app.selected_details.as_ref(), app.show_help, app.focus, app.detail_scroll, app.edit_state.as_ref());
        })?;

        // Check for file changes (non-blocking) with debounce
        if fs_rx.try_recv().is_ok() {
            // Drain any additional pending events
            while fs_rx.try_recv().is_ok() {}

            // Only refresh if cooldown has passed
            if last_refresh.elapsed() >= refresh_cooldown {
                app.refresh();
                last_refresh = Instant::now();
            }
        }

        // Poll for events with timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    app.handle_key(key.code, key.modifiers);
                    app.update_selected_details();
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::Down(_) | MouseEventKind::Up(MouseButton::Left) => {
                            app.handle_mouse(mouse.column, mouse.row, size.width, size.height);
                            app.update_selected_details();
                        }
                        MouseEventKind::ScrollDown => {
                            if app.focus == Focus::Details {
                                app.scroll_details(3);
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if app.focus == Focus::Details {
                                app.scroll_details(-3);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    Ok(())
}
