//! Spike: schema-driven TOML editor TUI (issue #711).
//!
//! Launch:  cargo run -- [path/to/maestro.toml]
//! Default fixture: ./fixtures/maestro.toml

use std::{
    io::{self, Stdout},
    path::PathBuf,
};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use spike_toml_editor::widgets::{self, EditBuffer, ViewState};
use spike_toml_editor::{
    FieldType, Schema, edit_value, load_document, maestro_schema, save_document,
};
use toml_edit::DocumentMut;

struct App {
    schema: Schema,
    doc: DocumentMut,
    fixture_path: PathBuf,
    focused_tab: usize,
    focused_field: usize,
    edit_buffer: Option<EditBuffer>,
    status: String,
    quit: bool,
}

impl App {
    fn next_tab(&mut self) {
        self.focused_tab = (self.focused_tab + 1) % self.schema.fields.len();
        self.focused_field = 0;
    }

    fn prev_tab(&mut self) {
        self.focused_tab =
            (self.focused_tab + self.schema.fields.len() - 1) % self.schema.fields.len();
        self.focused_field = 0;
    }

    fn next_field(&mut self) {
        let len = self.schema.tab_fields(self.focused_tab).len();
        if len > 0 {
            self.focused_field = (self.focused_field + 1) % len;
        }
    }

    fn prev_field(&mut self) {
        let len = self.schema.tab_fields(self.focused_tab).len();
        if len > 0 {
            self.focused_field = (self.focused_field + len - 1) % len;
        }
    }

    fn enter_edit(&mut self) {
        let tab_name = self.schema.fields[self.focused_tab].name;
        let fields = self.schema.tab_fields(self.focused_tab);
        let Some(field) = fields.get(self.focused_field) else {
            return;
        };
        let buffer = match &field.field_type {
            FieldType::Bool => {
                let current = self.doc[tab_name][field.name].as_bool().unwrap_or(false);
                EditBuffer::Bool(current)
            }
            FieldType::Int { .. } => {
                let current = self.doc[tab_name][field.name].as_integer().unwrap_or(0);
                EditBuffer::Int(current.to_string())
            }
            FieldType::String => {
                let current = self.doc[tab_name][field.name]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                EditBuffer::Str(current)
            }
            FieldType::Enum(options) => {
                let current = self.doc[tab_name][field.name].as_str().unwrap_or("");
                let cursor = options.iter().position(|o| *o == current).unwrap_or(0);
                EditBuffer::Enum { options, cursor }
            }
            FieldType::Table(_) => return,
        };
        self.edit_buffer = Some(buffer);
    }

    fn commit_edit(&mut self) {
        let tab_name = self.schema.fields[self.focused_tab].name;
        let fields = self.schema.tab_fields(self.focused_tab);
        let Some(field) = fields.get(self.focused_field) else {
            return;
        };
        let field_name = field.name;
        let Some(buffer) = self.edit_buffer.take() else {
            return;
        };
        if let Some(v) = buffer.into_value() {
            edit_value(&mut self.doc, tab_name, field_name, v);
            self.status = format!("edited {tab_name}.{field_name}");
        } else {
            self.status = "invalid value, edit discarded".to_string();
        }
    }

    fn save(&mut self) {
        match save_document(&self.fixture_path, &self.doc) {
            Ok(()) => self.status = format!("saved → {}", self.fixture_path.display()),
            Err(e) => self.status = format!("save failed: {e}"),
        }
    }
}

fn main() -> Result<()> {
    let fixture_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            PathBuf::from(manifest_dir)
                .join("fixtures")
                .join("maestro.toml")
        });

    let doc = load_document(&fixture_path)?;
    let app = App {
        schema: maestro_schema(),
        doc,
        fixture_path,
        focused_tab: 0,
        focused_field: 0,
        edit_buffer: None,
        status: String::from("ready"),
        quit: false,
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut app: App) -> Result<()> {
    while !app.quit {
        terminal.draw(|f| {
            let view = ViewState {
                schema: &app.schema,
                doc: &app.doc,
                focused_tab: app.focused_tab,
                focused_field: app.focused_field,
                edit_buffer: app.edit_buffer.as_ref(),
                status: &app.status,
            };
            widgets::draw(f, &view);
        })?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if app.edit_buffer.is_some() {
            handle_edit_key(&mut app, key.code);
        } else {
            handle_nav_key(&mut app, key.code, key.modifiers);
        }
    }
    Ok(())
}

fn handle_nav_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    match code {
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Char('s') => app.save(),
        KeyCode::Tab => app.next_tab(),
        KeyCode::BackTab => app.prev_tab(),
        KeyCode::Char('h') => app.prev_tab(),
        KeyCode::Char('l') => app.next_tab(),
        KeyCode::Up | KeyCode::Char('k') => app.prev_field(),
        KeyCode::Down | KeyCode::Char('j') => app.next_field(),
        KeyCode::Enter => app.enter_edit(),
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => app.quit = true,
        _ => {}
    }
}

fn handle_edit_key(app: &mut App, code: KeyCode) {
    let Some(buf) = app.edit_buffer.as_mut() else {
        return;
    };
    match (buf, code) {
        (_, KeyCode::Esc) => {
            app.edit_buffer = None;
        }
        (_, KeyCode::Enter) => app.commit_edit(),
        (EditBuffer::Bool(b), KeyCode::Char(' ')) => {
            *b = !*b;
        }
        (EditBuffer::Int(s) | EditBuffer::Str(s), KeyCode::Backspace) => {
            s.pop();
        }
        (EditBuffer::Int(s), KeyCode::Char(c)) if c.is_ascii_digit() || c == '-' => {
            s.push(c);
        }
        (EditBuffer::Str(s), KeyCode::Char(c)) => {
            s.push(c);
        }
        (EditBuffer::Enum { options, cursor }, KeyCode::Up) => {
            if *cursor > 0 {
                *cursor -= 1;
            } else {
                *cursor = options.len().saturating_sub(1);
            }
        }
        (EditBuffer::Enum { options, cursor }, KeyCode::Down) => {
            *cursor = (*cursor + 1) % options.len().max(1);
        }
        _ => {}
    }
}
