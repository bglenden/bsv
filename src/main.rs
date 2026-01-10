mod bd;
mod state;
mod tree;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
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
        })
    }

    fn update_selected_details(&mut self) {
        let current_id = self.tree.selected_id().map(|s| s.to_string());
        if current_id != self.last_selected_id {
            self.selected_details = current_id.as_ref()
                .and_then(|id| bd::get_issue_details(id).ok().flatten());
            self.last_selected_id = current_id;
        }
    }

    fn refresh(&mut self) {
        // Preserve current selection
        let selected_id = self.tree.selected_id().map(|s| s.to_string());

        if let Ok(issues) = bd::list_issues() {
            let ready_ids = bd::get_ready_ids().unwrap_or_default();
            self.tree = IssueTree::from_issues(issues, self.tree.expanded.clone(), ready_ids);

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
        match (code, modifiers) {
            // Quit
            (KeyCode::Char('q'), KeyModifiers::NONE) |
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }

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
            (KeyCode::Enter, KeyModifiers::NONE) |
            (KeyCode::Char(' '), KeyModifiers::NONE) => {
                self.tree.toggle_expand();
                let _ = save_expanded(&self.tree.expanded);
            }

            // Toggle expand/collapse all
            (KeyCode::Tab, KeyModifiers::NONE) => {
                self.tree.toggle_expand_all();
                let _ = save_expanded(&self.tree.expanded);
            }

            // Toggle show/hide closed
            (KeyCode::Char('c'), KeyModifiers::NONE) => {
                self.tree.toggle_show_closed();
            }

            // Refresh data
            (KeyCode::Char('r'), KeyModifiers::NONE) => {
                self.refresh();
            }

            // Help
            (KeyCode::Char('?'), KeyModifiers::NONE) |
            (KeyCode::Char('?'), KeyModifiers::SHIFT) => {
                self.show_help = !self.show_help;
            }

            // Escape closes help if open
            (KeyCode::Esc, KeyModifiers::NONE) => {
                if self.show_help {
                    self.show_help = false;
                }
            }

            _ => {}
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
    println!("KEYBINDINGS:");
    println!("    j/↓        Move cursor down");
    println!("    k/↑        Move cursor up");
    println!("    g/Home     Go to top");
    println!("    G/End      Go to bottom");
    println!("    l/→/Enter  Expand node");
    println!("    h/←        Collapse node (or go to parent)");
    println!("    Space      Toggle expand/collapse");
    println!("    Tab        Toggle expand/collapse all");
    println!("    c          Toggle show/hide closed");
    println!("    r          Refresh data from bd");
    println!("    ?          Show this help");
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
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;
    let mut last_refresh = Instant::now();
    let refresh_cooldown = Duration::from_millis(500);

    // Main loop
    loop {
        terminal.draw(|frame| {
            ui::render(frame, &app.tree, app.selected_details.as_ref(), app.show_help);
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

        // Poll for key events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key.code, key.modifiers);
                app.update_selected_details();
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}
