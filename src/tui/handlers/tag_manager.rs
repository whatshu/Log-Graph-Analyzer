use crossterm::event::{KeyCode, KeyEvent};
use crate::tui::app::{App, InputMode, ViewKind};
pub fn handle_tag_manager_popup(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            app.show_tag_manager = false;
            app.status_message = String::from("Tag manager closed");
        }
        KeyCode::Enter => {
            // Jump to selected tag's start line
            let tags = app.tag_store.get_tags(&app.repo_name).to_vec();
            if app.tag_manager_cursor < tags.len() {
                let tag = &tags[app.tag_manager_cursor];
                let mut jumped = false;
                if let Some(&(start, _)) = tag.ranges.first() {
                    app.cursor_line = start;
                    if start < app.scroll_offset || start >= app.scroll_offset + 50 {
                        app.scroll_offset = start.saturating_sub(5);
                    }
                    app.load_viewport();
                    app.status_message = format!("Jumped to tag '{}' (line {})", tag.name, start + 1);
                    jumped = true;
                }
                if !jumped {
                    app.status_message = format!("Tag '{}' has no ranges", tag.name);
                }
                app.show_tag_manager = false;
                app.active_view = ViewKind::LogView;
            }
        }
        KeyCode::Char('r') => {
            // Rename selected tag
            let tags = app.tag_store.get_tags(&app.repo_name).to_vec();
            if app.tag_manager_cursor < tags.len() {
                let old_name = tags[app.tag_manager_cursor].name.clone();
                app.input_mode = InputMode::Input;
                app.input_buffer = old_name.clone();
                app.input_prompt = format!("Rename tag '{}' to: ", old_name);
                app.pending_tag_rename = Some(old_name);
            }
        }
        KeyCode::Char('d') => {
            // Delete selected tag
            let tags = app.tag_store.get_tags(&app.repo_name).to_vec();
            if app.tag_manager_cursor < tags.len() {
                let name = tags[app.tag_manager_cursor].name.clone();
                app.tag_store.remove_tag(&app.repo_name, &name);
                if app.tag_manager_cursor >= tags.len().saturating_sub(1) {
                    app.tag_manager_cursor = app.tag_manager_cursor.saturating_sub(1);
                }
                let _ = app.tag_store.save(&app.workspace.root());
                app.status_message = format!("Tag '{}' deleted", name);
            }
        }
        KeyCode::Char('y') => {
            // Copy tag (duplicate with auto name)
            let tags = app.tag_store.get_tags(&app.repo_name).to_vec();
            if app.tag_manager_cursor < tags.len() {
                let tag = &tags[app.tag_manager_cursor];
                let new_name = app.tag_store.next_auto_name(&app.repo_name);
                let new_tag = lograph::tag::Tag {
                    name: new_name.clone(),
                    ranges: tag.ranges.clone(),
                    created_at: chrono::Utc::now(),
                };
                app.tag_store.add_tag(&app.repo_name, new_tag);
                let _ = app.tag_store.save(&app.workspace.root());
                app.status_message = format!("Tag '{}' copied to '{}'", tag.name, new_name);
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            let tags = app.tag_store.get_tags(&app.repo_name);
            if app.tag_manager_cursor + 1 < tags.len() {
                app.tag_manager_cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.tag_manager_cursor > 0 {
                app.tag_manager_cursor -= 1;
            }
        }
        KeyCode::Left => {
            app.tag_manager_h_scroll = app.tag_manager_h_scroll.saturating_sub(8);
        }
        KeyCode::Right => {
            app.tag_manager_h_scroll = app.tag_manager_h_scroll.saturating_add(8);
        }
        _ => {}
    }
}

