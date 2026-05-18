//! Pure render functions for the spike TUI. State lives in `main.rs::App`.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs},
};
use toml_edit::DocumentMut;

use crate::schema::{EditedValue, Schema};

pub struct ViewState<'a> {
    pub schema: &'a Schema,
    pub doc: &'a DocumentMut,
    pub focused_tab: usize,
    pub focused_field: usize,
    pub edit_buffer: Option<&'a EditBuffer>,
    pub status: &'a str,
}

#[derive(Debug, Clone)]
pub enum EditBuffer {
    Bool(bool),
    Int(String),
    Str(String),
    Enum {
        options: &'static [&'static str],
        cursor: usize,
    },
}

impl EditBuffer {
    pub fn into_value(self) -> Option<EditedValue> {
        match self {
            EditBuffer::Bool(b) => Some(EditedValue::Bool(b)),
            EditBuffer::Int(s) => s.parse::<i64>().ok().map(EditedValue::Int),
            EditBuffer::Str(s) => Some(EditedValue::Str(s)),
            EditBuffer::Enum { options, cursor } => options
                .get(cursor)
                .map(|s| EditedValue::Str((*s).to_string())),
        }
    }

    pub fn display(&self) -> String {
        match self {
            EditBuffer::Bool(b) => b.to_string(),
            EditBuffer::Int(s) | EditBuffer::Str(s) => s.clone(),
            EditBuffer::Enum { options, cursor } => {
                options.get(*cursor).copied().unwrap_or("").to_string()
            }
        }
    }
}

pub fn draw(frame: &mut Frame, view: &ViewState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_tabs(frame, chunks[0], view);
    render_fields(frame, chunks[1], view);
    render_status(frame, chunks[2], view);

    if let Some(buf) = view.edit_buffer {
        render_edit_overlay(frame, frame.area(), view, buf);
    }
}

fn render_tabs(frame: &mut Frame, area: Rect, view: &ViewState) {
    let titles: Vec<Line> = view
        .schema
        .fields
        .iter()
        .map(|f| Line::from(f.name))
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Tabs"))
        .select(view.focused_tab)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(tabs, area);
}

fn render_fields(frame: &mut Frame, area: Rect, view: &ViewState) {
    let Some(tab) = view.schema.fields.get(view.focused_tab) else {
        return;
    };
    let fields = view.schema.tab_fields(view.focused_tab);
    if fields.is_empty() {
        return;
    }

    let items: Vec<ListItem> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let current = view.doc[tab.name][field.name].to_string();
            let label = format!("{:<20} = {}", field.name, current.trim());
            let style = if idx == view.focused_field {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("[{}]", tab.name)),
    );
    frame.render_widget(list, area);
}

fn render_status(frame: &mut Frame, area: Rect, view: &ViewState) {
    let hint = if view.edit_buffer.is_some() {
        "enter commit | esc cancel | space/up/down toggle"
    } else {
        "q quit | s save | tab next-tab | ↑/↓ field | enter edit"
    };
    let line = Line::from(vec![
        Span::raw(hint),
        Span::raw("  |  "),
        Span::raw(view.status),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_edit_overlay(frame: &mut Frame, area: Rect, view: &ViewState, buf: &EditBuffer) {
    let popup_area = centered_rect(60, 30, area);
    frame.render_widget(Clear, popup_area);

    let tab = &view.schema.fields[view.focused_tab];
    let fields = view.schema.tab_fields(view.focused_tab);
    let Some(field) = fields.get(view.focused_field) else {
        return;
    };

    let body = format!("{}.{}\n\nValue: {}", tab.name, field.name, buf.display());
    let para = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(" Edit "));
    frame.render_widget(para, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(v[1])[1]
}
