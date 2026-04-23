use super::LandingScreen;
use super::types::MENU_ITEMS;
use crate::mascot::frames::{MASCOT_ROWS, MASCOT_WIDTH as MASCOT_WIDTH_USIZE};
use crate::mascot::widget::MascotWidget;
use crate::tui::theme::Theme;
use crate::tui::widgets::header_brand::LOGO;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const MASCOT_HEIGHT: u16 = MASCOT_ROWS as u16;
const MASCOT_WIDTH: u16 = MASCOT_WIDTH_USIZE as u16;
const LOGO_HEIGHT: u16 = 8;
const SPLASH_COLOR: Color = Color::Rgb(0, 255, 65);

impl LandingScreen {
    pub(super) fn draw_impl(&self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let menu_height = MENU_ITEMS.len() as u16;
        let total_height = MASCOT_HEIGHT + 1 + LOGO_HEIGHT + 1 + 1 + 2 + menu_height;
        let y_start = area.y + area.height.saturating_sub(total_height) / 2;

        let x_mascot = area.x + area.width.saturating_sub(MASCOT_WIDTH) / 2;
        if y_start + MASCOT_HEIGHT <= area.y + area.height
            && x_mascot + MASCOT_WIDTH <= area.x + area.width
        {
            let mascot_rect = Rect::new(x_mascot, y_start, MASCOT_WIDTH, MASCOT_HEIGHT);
            let widget = MascotWidget::new(self.mascot_state, self.mascot_frame, SPLASH_COLOR);
            f.render_widget(widget, mascot_rect);
        }

        let logo_y = y_start + MASCOT_HEIGHT + 1;
        if logo_y + LOGO_HEIGHT <= area.y + area.height {
            let logo_rect = Rect::new(area.x, logo_y, area.width, LOGO_HEIGHT);
            let logo = Paragraph::new(LOGO)
                .style(Style::default().fg(SPLASH_COLOR))
                .alignment(Alignment::Center);
            f.render_widget(logo, logo_rect);
        }

        let version_y = logo_y + LOGO_HEIGHT;
        if version_y < area.y + area.height {
            let version_line = Line::from(vec![Span::styled(
                format!("v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(Color::DarkGray),
            )]);
            let version_para = Paragraph::new(version_line).alignment(Alignment::Center);
            let version_rect = Rect::new(area.x, version_y, area.width, 1);
            f.render_widget(version_para, version_rect);
        }

        let menu_y = version_y + 2;
        for (i, item) in MENU_ITEMS.iter().enumerate() {
            let row_y = menu_y + i as u16;
            if row_y >= area.y + area.height {
                break;
            }
            let selected = i == self.selected;
            let style = if selected {
                Style::default()
                    .fg(SPLASH_COLOR)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(SPLASH_COLOR)
            };
            let marker = if selected { ">" } else { " " };
            let line = Line::from(vec![Span::styled(
                format!("{} [{}] {}", marker, item.shortcut, item.label),
                style,
            )]);
            let para = Paragraph::new(line).alignment(Alignment::Center);
            let row_rect = Rect::new(area.x, row_y, area.width, 1);
            f.render_widget(para, row_rect);
        }
    }
}
