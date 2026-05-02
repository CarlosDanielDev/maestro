use super::LandingScreen;
use super::types::MENU_ITEMS;
use crate::tui::icons;
use crate::tui::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const NARROW_WIDTH: u16 = 60;
const MIN_MENU_PADDING: u16 = 2;
const COLUMN_GAP: u16 = 4;
const MENU_KEY_LABEL_GAP: &str = "  ";
const FOOTER_HINT: &str = "<q> quit";

const ASCII_HEADER: &[&str] = &[
    " __  __    _    _____ ____ _____ ____   ___ ",
    "|  \\/  |  / \\  | ____/ ___|_   _|  _ \\ / _ \\",
    "| |\\/| | / _ \\ |  _| \\___ \\ | | | |_) | | | |",
    "| |  | |/ ___ \\| |___ ___) || | |  _ <| |_| |",
    "|_|  |_/_/   \\_\\_____|____/ |_| |_| \\_\\\\___/",
];

const NERD_HEADER: &[&str] = &[
    "███╗   ███╗ █████╗ ███████╗███████╗████████╗██████╗  ██████╗",
    "████╗ ████║██╔══██╗██╔════╝██╔════╝╚══██╔══╝██╔══██╗██╔═══██╗",
    "██╔████╔██║███████║█████╗  ███████╗   ██║   ██████╔╝██║   ██║",
    "██║╚██╔╝██║██╔══██║██╔══╝  ╚════██║   ██║   ██╔══██╗██║   ██║",
    "██║ ╚═╝ ██║██║  ██║███████╗███████║   ██║   ██║  ██║╚██████╔╝",
    "╚═╝     ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝   ╚═╝   ╚═╝  ╚═╝ ╚═════╝",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WelcomeLayout {
    pub menu_x: u16,
    pub menu_y: u16,
    pub menu_width: u16,
    pub left_padding: u16,
    pub right_padding: u16,
    pub columns: u16,
    pub show_header: bool,
    pub footer_y: u16,
}

#[derive(Debug, Clone, Copy)]
struct RenderMode {
    use_nerd_font: bool,
    styled: bool,
}

struct MenuItemRender<'a> {
    x: u16,
    y: u16,
    width: u16,
    shortcut: char,
    label: &'a str,
    selected: bool,
}

pub(crate) fn welcome_layout(area: Rect) -> WelcomeLayout {
    let show_header = area.width >= NARROW_WIDTH;
    let columns = if area.width >= 80 { 2 } else { 1 };
    let menu_width = menu_block_width(columns)
        .min(area.width.saturating_sub(MIN_MENU_PADDING * 2))
        .max(1);
    let left_padding = area.width.saturating_sub(menu_width) / 2;
    let right_padding = area.width.saturating_sub(menu_width + left_padding);
    let header_height = if show_header {
        NERD_HEADER.len() as u16 + 2
    } else {
        0
    };
    let menu_height = menu_rows(columns);
    let total_height = header_height + menu_height + 2;
    let y_start = area.y + area.height.saturating_sub(total_height) / 2;
    let menu_y = y_start + header_height;

    WelcomeLayout {
        menu_x: area.x + left_padding,
        menu_y,
        menu_width,
        left_padding,
        right_padding,
        columns,
        show_header,
        footer_y: menu_y + menu_height + 1,
    }
}

impl LandingScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mode = RenderMode {
            use_nerd_font: icons::use_nerd_font(),
            styled: should_emit_styles(),
        };
        self.draw_welcome(f, area, theme, mode);
    }

    #[cfg(test)]
    pub(crate) fn draw_impl_for_test(
        &self,
        f: &mut Frame,
        area: Rect,
        theme: &Theme,
        use_nerd_font: bool,
        styled: bool,
    ) {
        self.draw_welcome(
            f,
            area,
            theme,
            RenderMode {
                use_nerd_font,
                styled,
            },
        );
    }

    fn draw_welcome(&self, f: &mut Frame, area: Rect, theme: &Theme, mode: RenderMode) {
        let layout = welcome_layout(area);
        if layout.show_header {
            self.draw_header(f, area, &layout, theme, mode);
        }
        self.draw_menu(f, area, &layout, theme, mode);
        self.draw_footer(f, area, &layout, theme, mode);
    }

    fn draw_header(
        &self,
        f: &mut Frame,
        area: Rect,
        layout: &WelcomeLayout,
        theme: &Theme,
        mode: RenderMode,
    ) {
        let header = header_for_width(mode.use_nerd_font, area.width);
        let header_y = layout
            .menu_y
            .saturating_sub(header.len() as u16)
            .saturating_sub(2);
        let style = if mode.styled {
            Style::default()
                .fg(theme.accent_success)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        for (idx, text) in header.iter().enumerate() {
            render_centered_text(f, area, header_y + idx as u16, text, style);
        }

        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
        let version_style = if mode.styled {
            Style::default().fg(theme.text_secondary)
        } else {
            Style::default()
        };
        render_centered_text(
            f,
            area,
            header_y + header.len() as u16,
            &version,
            version_style,
        );
    }

    fn draw_menu(
        &self,
        f: &mut Frame,
        area: Rect,
        layout: &WelcomeLayout,
        theme: &Theme,
        mode: RenderMode,
    ) {
        let rows = menu_rows(layout.columns);
        let column_width = column_width_for_layout(layout);

        for row in 0..rows {
            let y = layout.menu_y + row;
            if y >= area.y + area.height {
                break;
            }

            for col in 0..layout.columns {
                let idx = row as usize + (col as usize * rows as usize);
                let Some(item) = MENU_ITEMS.get(idx) else {
                    continue;
                };

                let x = layout.menu_x + col * (column_width + COLUMN_GAP);
                let selected = idx == self.selected;
                let item_width = if col + 1 == layout.columns {
                    layout
                        .menu_width
                        .saturating_sub(col * (column_width + COLUMN_GAP))
                } else {
                    column_width
                };
                render_menu_item(
                    f,
                    MenuItemRender {
                        x,
                        y,
                        width: item_width,
                        shortcut: item.shortcut,
                        label: item.label,
                        selected,
                    },
                    theme,
                    mode,
                );
            }
        }
    }

    fn draw_footer(
        &self,
        f: &mut Frame,
        area: Rect,
        layout: &WelcomeLayout,
        theme: &Theme,
        mode: RenderMode,
    ) {
        if layout.footer_y >= area.y + area.height {
            return;
        }

        let style = if mode.styled {
            Style::default().fg(theme.text_secondary)
        } else {
            Style::default()
        };
        render_text(
            f,
            layout.menu_x,
            layout.footer_y,
            layout.menu_width,
            FOOTER_HINT,
            style,
        );
    }
}

fn menu_rows(columns: u16) -> u16 {
    let columns = columns.max(1) as usize;
    MENU_ITEMS.len().div_ceil(columns) as u16
}

fn menu_block_width(columns: u16) -> u16 {
    let item_width = MENU_ITEMS
        .iter()
        .map(|item| menu_item_width(item.shortcut, item.label))
        .max()
        .unwrap_or(1) as u16;
    if columns > 1 {
        item_width * columns + COLUMN_GAP
    } else {
        item_width
    }
}

fn menu_item_width(shortcut: char, label: &str) -> usize {
    format!("[{}]{}{}", shortcut, MENU_KEY_LABEL_GAP, label).width()
}

fn column_width_for_layout(layout: &WelcomeLayout) -> u16 {
    if layout.columns > 1 {
        layout.menu_width.saturating_sub(COLUMN_GAP) / 2
    } else {
        layout.menu_width
    }
}

fn header_for_width(use_nerd_font: bool, width: u16) -> &'static [&'static str] {
    let nerd_header_width = NERD_HEADER
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0) as u16;
    if use_nerd_font && nerd_header_width <= width {
        NERD_HEADER
    } else {
        ASCII_HEADER
    }
}

fn should_emit_styles() -> bool {
    should_emit_styles_from_env(
        |key| std::env::var(key).ok(),
        std::env::var_os("NO_COLOR").is_some(),
    )
}

fn should_emit_styles_from_env(
    get_env: impl Fn(&str) -> Option<String>,
    no_color_present: bool,
) -> bool {
    if no_color_present {
        return false;
    }
    get_env("TERM").is_none_or(|term| term != "dumb")
}

fn render_menu_item(f: &mut Frame, item: MenuItemRender<'_>, theme: &Theme, mode: RenderMode) {
    let key = format!("[{}]", item.shortcut);
    let label_width = item
        .width
        .saturating_sub(key.width() as u16)
        .saturating_sub(MENU_KEY_LABEL_GAP.width() as u16) as usize;
    let label = ellipsize(item.label, label_width);
    let selected_style = if mode.styled {
        Style::default()
            .fg(theme.selection_fg)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
    };
    let key_style = if !mode.styled {
        Style::default()
    } else if item.selected {
        selected_style
    } else {
        Style::default().fg(theme.text_secondary)
    };
    let label_style = if !mode.styled {
        Style::default()
    } else if item.selected {
        selected_style
    } else {
        Style::default().fg(theme.text_primary)
    };

    let line = Line::from(vec![
        Span::styled(key, key_style),
        Span::styled(MENU_KEY_LABEL_GAP, label_style),
        Span::styled(label, label_style),
    ]);
    let rect = Rect::new(item.x, item.y, item.width, 1);
    f.render_widget(Paragraph::new(line), rect);
}

fn render_centered_text(f: &mut Frame, area: Rect, y: u16, text: &str, style: Style) {
    if y >= area.y + area.height {
        return;
    }
    let text_width = text.width() as u16;
    let x = area.x + area.width.saturating_sub(text_width) / 2;
    let width = text_width.min(area.width);
    render_text(f, x, y, width, text, style);
}

fn render_text(f: &mut Frame, x: u16, y: u16, width: u16, text: &str, style: Style) {
    if width == 0 {
        return;
    }
    let text = ellipsize(text, width as usize);
    let rect = Rect::new(x, y, width, 1);
    f.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), rect);
}

fn ellipsize(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let mut out = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width + ch_width > max_width - 1 {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push('…');
    out
}

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;
