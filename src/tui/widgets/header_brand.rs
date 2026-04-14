use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::mascot;
use crate::tui::icons::{self, IconId};
use crate::tui::theme::Theme;

const LOGO_WIDTH: u16 = 62;

pub const LOGO: &str = r#"
 ███╗   ███╗ █████╗ ███████╗███████╗████████╗██████╗  ██████╗
 ████╗ ████║██╔══██╗██╔════╝██╔════╝╚══██╔══╝██╔══██╗██╔═══██╗
 ██╔████╔██║███████║█████╗  ███████╗   ██║   ██████╔╝██║   ██║
 ██║╚██╔╝██║██╔══██║██╔══╝  ╚════██║   ██║   ██╔══██╗██║   ██║
 ██║ ╚═╝ ██║██║  ██║███████╗███████║   ██║   ██║  ██║╚██████╔╝
 ╚═╝     ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝   ╚═╝   ╚═╝  ╚═╝ ╚═════╝
"#;

/// Configuration for what parts of the header brand to display.
#[derive(Debug, Clone)]
pub struct HeaderBrandProps {
    pub show_mascot: bool,
    pub show_repo_info: bool,
    pub mascot_state: mascot::MascotState,
    pub mascot_frame: usize,
    pub repo: String,
    pub branch: String,
    pub username: Option<String>,
}

/// Reusable header brand widget: MAESTRO logo + optional mascot + repo info bar.
pub struct HeaderBrand<'a> {
    props: HeaderBrandProps,
    theme: &'a Theme,
}

impl<'a> HeaderBrand<'a> {
    pub fn new(props: HeaderBrandProps, theme: &'a Theme) -> Self {
        Self { props, theme }
    }

    fn render_logo(&self, area: Rect, buf: &mut Buffer) {
        let logo = Paragraph::new(LOGO)
            .style(Style::default().fg(self.theme.accent_success))
            .alignment(Alignment::Center);
        logo.render(area, buf);

        if self.props.show_mascot && area.width >= 40 && area.height >= 6 {
            let logo_width = LOGO_WIDTH;
            let logo_end_x = area.x + area.width.saturating_sub(logo_width) / 2 + logo_width;
            let mascot_w = mascot::frames::MASCOT_WIDTH as u16;
            let mascot_x = logo_end_x + 1;
            let mascot_h = (mascot::frames::MASCOT_ROWS as u16).min(area.height);
            let mascot_y = area.y + area.height.saturating_sub(mascot_h) / 2;

            let sep_x = mascot_x;
            let mascot_x = sep_x + 2;

            if mascot_x + mascot_w <= area.x + area.width {
                let sep_style = Style::default().fg(self.theme.text_secondary);
                for row in 0..mascot_h {
                    let y = mascot_y + row;
                    if y < area.y + area.height {
                        buf.set_string(sep_x, y, "\u{2502}", sep_style);
                    }
                }

                let mascot_rect = Rect::new(mascot_x, mascot_y, mascot_w, mascot_h);
                mascot::widget::MascotWidget::new(
                    self.props.mascot_state,
                    self.props.mascot_frame,
                    self.theme.accent_success,
                )
                .render(mascot_rect, buf);
            }
        }
    }

    fn render_repo_info(&self, area: Rect, buf: &mut Buffer) {
        let username_display = self.props.username.as_deref().unwrap_or("unknown");

        let info = Line::from(vec![
            Span::styled(
                format!("  {} ", icons::get(IconId::Repo)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                &self.props.repo,
                Style::default().fg(self.theme.accent_info),
            ),
            Span::raw("  |  "),
            Span::styled(
                format!("{} ", icons::get(IconId::Branch)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                &self.props.branch,
                Style::default().fg(self.theme.accent_warning),
            ),
            Span::raw("  |  "),
            Span::styled(
                format!("{} ", icons::get(IconId::User)),
                Style::default().fg(self.theme.text_secondary),
            ),
            Span::styled(
                format!("@{}", username_display),
                Style::default().fg(self.theme.accent_success),
            ),
        ]);
        let block = Block::default().borders(Borders::BOTTOM);
        let para = Paragraph::new(info)
            .block(block)
            .alignment(Alignment::Center);
        para.render(area, buf);
    }
}

impl<'a> Widget for HeaderBrand<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 2 {
            return;
        }

        let logo_height = 8u16.min(area.height);
        let info_height = if self.props.show_repo_info {
            3u16.min(area.height.saturating_sub(logo_height))
        } else {
            0
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(logo_height),
                Constraint::Length(info_height),
            ])
            .split(area);

        self.render_logo(chunks[0], buf);

        if self.props.show_repo_info && info_height > 0 {
            self.render_repo_info(chunks[1], buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mascot::MascotState;
    use crate::tui::icons;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_to_string(props: HeaderBrandProps, theme: &Theme, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                f.render_widget(HeaderBrand::new(props, theme), f.area());
            })
            .unwrap();
        format!("{:?}", terminal.backend())
    }

    fn make_full_props() -> HeaderBrandProps {
        HeaderBrandProps {
            show_mascot: false,
            show_repo_info: true,
            mascot_state: MascotState::Idle,
            mascot_frame: 0,
            repo: "owner/myrepo".to_string(),
            branch: "feature/x".to_string(),
            username: Some("botar".to_string()),
        }
    }

    #[test]
    fn renders_logo_text() {
        let theme = Theme::default();
        let props = HeaderBrandProps {
            show_mascot: false,
            show_repo_info: false,
            mascot_state: MascotState::Idle,
            mascot_frame: 0,
            repo: String::new(),
            branch: String::new(),
            username: None,
        };
        let out = render_to_string(props, &theme, 120, 10);
        assert!(out.contains('█'), "logo block must be rendered");
    }

    #[test]
    fn repo_info_rendered_when_show_repo_info_true() {
        let theme = Theme::default();
        let props = make_full_props();
        let out = render_to_string(props, &theme, 120, 12);
        assert!(out.contains("owner/myrepo"), "repo name must appear");
        assert!(out.contains("feature/x"), "branch must appear");
        assert!(out.contains("botar"), "username must appear");
    }

    #[test]
    fn repo_info_absent_when_show_repo_info_false() {
        let theme = Theme::default();
        let mut props = make_full_props();
        props.show_repo_info = false;
        let out = render_to_string(props, &theme, 120, 12);
        assert!(!out.contains("owner/myrepo"), "repo name must be absent");
    }

    #[test]
    fn username_none_renders_unknown_fallback() {
        let theme = Theme::default();
        let props = HeaderBrandProps {
            show_repo_info: true,
            username: None,
            repo: "r".to_string(),
            branch: "main".to_string(),
            show_mascot: false,
            mascot_state: MascotState::Idle,
            mascot_frame: 0,
        };
        let out = render_to_string(props, &theme, 120, 12);
        assert!(
            out.contains("unknown"),
            "must show 'unknown' when username is None"
        );
    }

    #[test]
    fn mascot_not_rendered_when_show_mascot_false() {
        let theme = Theme::default();
        let props = HeaderBrandProps {
            show_mascot: false,
            mascot_state: MascotState::Conducting,
            mascot_frame: 0,
            show_repo_info: false,
            repo: String::new(),
            branch: String::new(),
            username: None,
        };
        let out = render_to_string(props, &theme, 120, 10);
        assert!(
            !out.contains('\u{2502}'),
            "mascot separator must be absent when show_mascot is false"
        );
    }

    #[test]
    fn logo_width_constant_matches_actual_width() {
        let actual = LOGO.lines().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
        assert_eq!(
            LOGO_WIDTH, actual,
            "LOGO_WIDTH constant must match actual logo width"
        );
    }

    #[test]
    fn renders_without_panic_at_minimum_size() {
        let theme = Theme::default();
        let props = make_full_props();
        let _ = render_to_string(props, &theme, 1, 1);
    }

    #[test]
    fn repo_info_uses_ascii_icons_in_ascii_mode() {
        icons::init_from_config(true);
        let theme = Theme::default();
        let props = make_full_props();
        let out = render_to_string(props, &theme, 120, 12);
        assert!(
            out.contains("owner/myrepo"),
            "repo info must render in ascii mode"
        );
        icons::init_from_config(false);
    }
}
