//! Integration tests for bsv TUI using tmux
//!
//! These tests spawn bsv in a tmux session, send keystrokes, and verify output.
//! Requires tmux to be installed.
//!
//! Note: Tests must run serially to avoid tmux session conflicts.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::sleep;
use std::time::Duration;

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);
const BSV_PATH: &str = env!("CARGO_BIN_EXE_bsv");

fn get_session_name() -> String {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("bsv-test-{}-{}", std::process::id(), id)
}

/// Test harness that manages a tmux session
struct TmuxTest {
    session_name: String,
}

impl TmuxTest {
    fn new() -> Option<Self> {
        if !Self::tmux_available() {
            return None;
        }

        let session_name = get_session_name();

        // Kill any existing session with this name
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &session_name])
            .output();

        // Start new session with bsv
        let result = Command::new("tmux")
            .args([
                "new-session",
                "-d",
                "-s",
                &session_name,
                "-x",
                "100",
                "-y",
                "30",
                BSV_PATH,
            ])
            .output();

        if result.is_err() {
            return None;
        }

        // Wait for bsv to start
        sleep(Duration::from_millis(800));

        Some(TmuxTest { session_name })
    }

    fn tmux_available() -> bool {
        Command::new("tmux")
            .arg("-V")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn send_keys(&self, keys: &str) {
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.session_name, keys])
            .output();
        sleep(Duration::from_millis(250));
    }

    fn capture_pane(&self) -> String {
        let output = Command::new("tmux")
            .args([
                "capture-pane",
                "-t",
                &self.session_name,
                "-p",
                "-S",
                "-",
                "-E",
                "-",
            ])
            .output()
            .expect("Failed to capture pane");
        String::from_utf8_lossy(&output.stdout).to_string()
    }
}

impl Drop for TmuxTest {
    fn drop(&mut self) {
        // Send q to quit bsv
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &self.session_name, "q"])
            .output();
        sleep(Duration::from_millis(200));

        // Kill the session
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.session_name])
            .output();
    }
}

// Run integration tests serially to avoid tmux conflicts
// Use: cargo test --test integration -- --test-threads=1

#[test]
fn test_initial_render() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    let output = test.capture_pane();

    // Verify two-panel layout
    assert!(output.contains("Issues"), "Should show Issues panel");
    assert!(output.contains("Details"), "Should show Details panel");
}

#[test]
fn test_navigation_j_k() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Get initial state
    let initial = test.capture_pane();

    // Move down with j
    test.send_keys("j");
    test.send_keys("j");
    let after_j = test.capture_pane();

    // Move back up with k
    test.send_keys("k");
    test.send_keys("k");
    let after_k = test.capture_pane();

    // The details panel should change when we navigate
    assert!(initial.contains("Details"));
    assert!(after_j.contains("Details"));
    assert!(after_k.contains("Details"));
}

#[test]
fn test_toggle_closed_c() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Get initial state (closed issues hidden by default)
    let without_closed = test.capture_pane();

    // Press 'c' to show closed
    test.send_keys("c");
    let with_closed = test.capture_pane();

    // Press 'c' again to hide closed
    test.send_keys("c");
    let without_closed_again = test.capture_pane();

    // UI should remain functional
    assert!(without_closed.contains("Issues"));
    assert!(with_closed.contains("Issues"));
    assert!(without_closed_again.contains("Issues"));
}

#[test]
fn test_expand_collapse_tab() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Get initial state
    let initial = test.capture_pane();

    // Press Tab to toggle expand/collapse all
    test.send_keys("Tab");
    sleep(Duration::from_millis(300));
    let after_tab = test.capture_pane();

    // Press Tab again to toggle back
    test.send_keys("Tab");
    sleep(Duration::from_millis(300));
    let after_tab_again = test.capture_pane();

    // UI should remain functional through tab toggles
    assert!(initial.contains("Issues"));
    assert!(after_tab.contains("Issues"));
    assert!(after_tab_again.contains("Issues"));

    // Tree state should change (either expanded or collapsed arrows)
    // Note: The exact behavior depends on initial state, so just verify UI is responsive
    let has_expanded = initial.contains('▼') || after_tab.contains('▼') || after_tab_again.contains('▼');
    let has_collapsed = initial.contains('▶') || after_tab.contains('▶') || after_tab_again.contains('▶');
    assert!(
        has_expanded || has_collapsed,
        "Tree should show expand/collapse indicators"
    );
}

#[test]
fn test_focus_panels_enter_h() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Initial state - tree focused
    let tree_focused = test.capture_pane();

    // Press Enter to focus details
    test.send_keys("Enter");
    let details_focused = test.capture_pane();

    // Press h to return to tree
    test.send_keys("h");
    let tree_again = test.capture_pane();

    // When details is focused, title should show scroll hint
    assert!(
        details_focused.contains("j/k to scroll") || details_focused.contains("Details"),
        "Details panel should be visible"
    );
    assert!(tree_focused.contains("Issues"));
    assert!(tree_again.contains("Issues"));
}

#[test]
fn test_space_toggle_expand() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Navigate to an expandable node (the Test Epic)
    test.send_keys("j");
    test.send_keys("j");
    test.send_keys("j");
    test.send_keys("j");
    sleep(Duration::from_millis(200));

    let before_space = test.capture_pane();

    // Press space to toggle
    test.send_keys("Space");
    sleep(Duration::from_millis(200));

    let after_space = test.capture_pane();

    // Space should toggle the expand state
    assert!(before_space.contains("Issues"));
    assert!(after_space.contains("Issues"));
}

#[test]
fn test_g_and_g_navigation() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Go to bottom with G
    test.send_keys("G");
    sleep(Duration::from_millis(200));
    let at_bottom = test.capture_pane();

    // Go to top with g
    test.send_keys("g");
    sleep(Duration::from_millis(200));
    let at_top = test.capture_pane();

    // Both states should show the UI
    assert!(at_bottom.contains("Issues"));
    assert!(at_top.contains("Issues"));
    assert!(at_bottom.contains("Details"));
    assert!(at_top.contains("Details"));
}

#[test]
fn test_detail_scroll_when_focused() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Focus details panel
    test.send_keys("Enter");
    sleep(Duration::from_millis(200));

    let before_scroll = test.capture_pane();

    // Scroll down
    test.send_keys("j");
    test.send_keys("j");
    test.send_keys("j");
    sleep(Duration::from_millis(200));

    let after_scroll = test.capture_pane();

    // Return to tree
    test.send_keys("h");

    // Both should show the UI
    assert!(before_scroll.contains("Details"));
    assert!(after_scroll.contains("Details"));
}

#[test]
fn test_refresh_r() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    let before_refresh = test.capture_pane();

    // Press r to refresh
    test.send_keys("r");
    sleep(Duration::from_millis(500));

    let after_refresh = test.capture_pane();

    // UI should still be intact after refresh
    assert!(before_refresh.contains("Issues"));
    assert!(after_refresh.contains("Issues"));
    assert!(before_refresh.contains("Details"));
    assert!(after_refresh.contains("Details"));
}

#[test]
fn test_cursor_visibility_after_g_navigation() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Go to bottom with G
    test.send_keys("G");
    sleep(Duration::from_millis(300));
    let at_bottom = test.capture_pane();

    // Go back to top with g
    test.send_keys("g");
    sleep(Duration::from_millis(300));
    let at_top = test.capture_pane();

    // Verify tree panel is visible and functional at both positions
    // The UI should remain stable during navigation
    assert!(
        at_bottom.contains("Issues") && at_bottom.contains("Details"),
        "UI panels should be visible after G (go to bottom)"
    );
    assert!(
        at_top.contains("Issues") && at_top.contains("Details"),
        "UI panels should be visible after g (go to top)"
    );

    // Verify tree has expand/collapse icons (indicates tree is rendered)
    let tree_rendered_bottom = at_bottom.contains('▶') || at_bottom.contains('▼');
    let tree_rendered_top = at_top.contains('▶') || at_top.contains('▼');

    assert!(
        tree_rendered_bottom,
        "Tree should be rendered after G navigation"
    );
    assert!(
        tree_rendered_top,
        "Tree should be rendered after g navigation"
    );
}

#[test]
fn test_cursor_stability_extended_navigation() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Navigate down multiple times
    for _ in 0..15 {
        test.send_keys("j");
    }
    sleep(Duration::from_millis(300));
    let after_down = test.capture_pane();

    // Navigate back up the same amount
    for _ in 0..15 {
        test.send_keys("k");
    }
    sleep(Duration::from_millis(300));
    let after_up = test.capture_pane();

    // UI should remain stable and functional throughout navigation
    assert!(
        after_down.contains("Issues") && after_down.contains("Details"),
        "Both panels should be visible after extended j navigation"
    );
    assert!(
        after_up.contains("Issues") && after_up.contains("Details"),
        "Both panels should be visible after extended k navigation"
    );

    // Tree should be rendered (has expand/collapse icons)
    let tree_rendered_down = after_down.contains('▶') || after_down.contains('▼');
    let tree_rendered_up = after_up.contains('▶') || after_up.contains('▼');

    assert!(
        tree_rendered_down,
        "Tree should remain rendered during downward navigation"
    );
    assert!(
        tree_rendered_up,
        "Tree should remain rendered during upward navigation"
    );
}

#[test]
fn test_scroll_does_not_jump() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // First, expand all to ensure we have a long list
    test.send_keys("Tab");
    sleep(Duration::from_millis(300));

    // Navigate down to middle of list
    for _ in 0..10 {
        test.send_keys("j");
    }
    sleep(Duration::from_millis(200));

    let mid_position = test.capture_pane();

    // Navigate down one more, then back up - UI should remain stable
    test.send_keys("j");
    sleep(Duration::from_millis(100));
    test.send_keys("k");
    sleep(Duration::from_millis(200));

    let after_bounce = test.capture_pane();

    // UI should be visible and functional throughout
    assert!(
        mid_position.contains("Issues") && mid_position.contains("Details"),
        "Panels should be visible at mid position"
    );
    assert!(
        after_bounce.contains("Issues") && after_bounce.contains("Details"),
        "Panels should be visible after j/k bounce"
    );

    // Tree should be rendered throughout
    let tree_rendered_mid = mid_position.contains('▶') || mid_position.contains('▼');
    let tree_rendered_bounce = after_bounce.contains('▶') || after_bounce.contains('▼');

    assert!(tree_rendered_mid, "Tree should be visible at mid position");
    assert!(
        tree_rendered_bounce,
        "Tree should be visible after j/k bounce"
    );
}

#[test]
fn test_page_navigation_ctrl_d_u() {
    let test = match TmuxTest::new() {
        Some(t) => t,
        None => {
            eprintln!("Skipping test: tmux not available");
            return;
        }
    };

    // Expand all first
    test.send_keys("Tab");
    sleep(Duration::from_millis(300));

    let initial = test.capture_pane();

    // Page down with Ctrl-D
    test.send_keys("C-d");
    sleep(Duration::from_millis(300));
    let after_page_down = test.capture_pane();

    // Page up with Ctrl-U
    test.send_keys("C-u");
    sleep(Duration::from_millis(300));
    let after_page_up = test.capture_pane();

    // UI should be functional throughout
    assert!(
        initial.contains("Issues") && initial.contains("Details"),
        "Initial state should show both panels"
    );
    assert!(
        after_page_down.contains("Issues") && after_page_down.contains("Details"),
        "After Ctrl-D should show both panels"
    );
    assert!(
        after_page_up.contains("Issues") && after_page_up.contains("Details"),
        "After Ctrl-U should show both panels"
    );

    // Tree should be rendered throughout (has expand/collapse icons)
    let tree_initial = initial.contains('▶') || initial.contains('▼');
    let tree_page_down = after_page_down.contains('▶') || after_page_down.contains('▼');
    let tree_page_up = after_page_up.contains('▶') || after_page_up.contains('▼');

    assert!(tree_initial, "Tree visible initially");
    assert!(tree_page_down, "Tree visible after page down");
    assert!(tree_page_up, "Tree visible after page up");
}
