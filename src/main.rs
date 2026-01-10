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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Tree,
    Details,
}
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::prelude::*;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use state::{load_expanded, save_expanded};
use tree::IssueTree;

struct App {
    tree: IssueTree,
    should_quit: bool,
    show_help: bool,
    selected_details: Option<bd::Issue>,
    last_selected_id: Option<String>,
    focus: Focus,
    detail_scroll: u16,
}

impl App {
    fn new() -> Result<Self> {
        let issues = bd::list_issues()?;
        let expanded = load_expanded();
        let ready_ids = bd::get_ready_ids().unwrap_or_default();
        let tree = IssueTree::from_issues(issues, expanded, ready_ids);

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

        if let Ok(issues) = bd::list_issues() {
            let ready_ids = bd::get_ready_ids().unwrap_or_default();
            self.tree = IssueTree::from_issues(issues, self.tree.expanded.clone(), ready_ids);
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

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
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

            _ => {}
        }

        // Handle focus-specific keys
        match self.focus {
            Focus::Tree => self.handle_tree_key(code, modifiers),
            Focus::Details => self.handle_details_key(code, modifiers),
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
    println!("    Click      Focus panel");
    println!();
    println!("GLOBAL:");
    println!("    c          Toggle show/hide closed");
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
        let issues = bd::list_issues()?;
        let expanded = load_expanded();
        let ready_ids = bd::get_ready_ids().unwrap_or_default();
        let tree = IssueTree::from_issues(issues, expanded, ready_ids);
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
            ui::render(frame, &app.tree, app.selected_details.as_ref(), app.show_help, app.focus, app.detail_scroll);
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
