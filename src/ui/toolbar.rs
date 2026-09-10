//! A reusable toolbar: a list of buttons, arranged in groups.
//!
//! Buttons are text only, icon only, or icon and text together; the component
//! offers all three whether or not a given toolbar uses them.
#![allow(dead_code)]

use eframe::egui::{self, vec2, Align2, Color32, FontId, Rect, Response, Sense, Stroke, Ui};

use super::icon::{Icon, ICON_SIZE};
use super::theme;

/// What a button shows.
#[derive(Debug, Clone)]
pub enum ButtonContent {
    Text(String),
    Icon(Icon),
    IconText(Icon, String),
}

#[derive(Debug, Clone)]
pub struct ToolbarButton {
    /// Identifies the button when it is clicked.
    pub id: &'static str,
    pub content: ButtonContent,
    pub tooltip: Option<String>,
    pub enabled: bool,
    pub active: bool,
    pub danger: bool,
}

impl ToolbarButton {
    pub fn text(id: &'static str, label: impl Into<String>) -> Self {
        Self::new(id, ButtonContent::Text(label.into()))
    }

    pub fn icon(id: &'static str, icon: Icon) -> Self {
        Self::new(id, ButtonContent::Icon(icon))
    }

    pub fn icon_text(id: &'static str, icon: Icon, label: impl Into<String>) -> Self {
        Self::new(id, ButtonContent::IconText(icon, label.into()))
    }

    fn new(id: &'static str, content: ButtonContent) -> Self {
        ToolbarButton {
            id,
            content,
            tooltip: None,
            enabled: true,
            active: false,
            danger: false,
        }
    }

    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Marks the button as showing a state that is currently on.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }
}

/// Buttons that belong together; groups are drawn with a separator between.
#[derive(Debug, Clone, Default)]
pub struct ToolbarGroup {
    pub buttons: Vec<ToolbarButton>,
}

impl ToolbarGroup {
    pub fn new(buttons: Vec<ToolbarButton>) -> Self {
        ToolbarGroup { buttons }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarDirection {
    Horizontal,
    Vertical,
}

/// A toolbar can contain many groups.
#[derive(Debug, Clone)]
pub struct Toolbar {
    pub groups: Vec<ToolbarGroup>,
    pub direction: ToolbarDirection,
    pub button_height: f32,
}

impl Toolbar {
    pub fn new(groups: Vec<ToolbarGroup>) -> Self {
        Toolbar {
            groups,
            direction: ToolbarDirection::Horizontal,
            button_height: 26.0,
        }
    }

    pub fn vertical(mut self) -> Self {
        self.direction = ToolbarDirection::Vertical;
        self
    }

    /// Draw the toolbar and report the id of the button that was clicked.
    pub fn show(&self, ui: &mut Ui) -> Option<&'static str> {
        match self.direction {
            ToolbarDirection::Horizontal => {
                let mut clicked = None;
                ui.horizontal(|ui| {
                    for (index, group) in self.groups.iter().enumerate() {
                        if index > 0 {
                            self.separator(ui);
                        }
                        for button in &group.buttons {
                            if self.button(ui, button).clicked() {
                                clicked = Some(button.id);
                            }
                        }
                    }
                });
                clicked
            }
            ToolbarDirection::Vertical => {
                let mut clicked = None;
                ui.vertical(|ui| {
                    for (index, group) in self.groups.iter().enumerate() {
                        if index > 0 {
                            self.separator(ui);
                        }
                        for button in &group.buttons {
                            if self.button(ui, button).clicked() {
                                clicked = Some(button.id);
                            }
                        }
                    }
                });
                clicked
            }
        }
    }

    fn separator(&self, ui: &mut Ui) {
        match self.direction {
            ToolbarDirection::Horizontal => {
                let (rect, _) = ui.allocate_exact_size(
                    vec2(9.0, self.button_height),
                    Sense::hover(),
                );
                let x = rect.center().x;
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.top() + 4.0),
                        egui::pos2(x, rect.bottom() - 4.0),
                    ],
                    Stroke::new(1.0, theme::BORDER),
                );
            }
            ToolbarDirection::Vertical => {
                let width = ui.available_width().min(self.button_height);
                let (rect, _) = ui.allocate_exact_size(vec2(width, 9.0), Sense::hover());
                let y = rect.center().y;
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left() + 4.0, y),
                        egui::pos2(rect.right() - 4.0, y),
                    ],
                    Stroke::new(1.0, theme::BORDER),
                );
            }
        }
    }

    fn button(&self, ui: &mut Ui, button: &ToolbarButton) -> Response {
        let font = FontId::proportional(13.0);
        let padding = 9.0;
        let gap = 6.0;

        let galley = match &button.content {
            ButtonContent::Text(text) | ButtonContent::IconText(_, text) => Some(
                ui.painter()
                    .layout_no_wrap(text.clone(), font.clone(), theme::TEXT),
            ),
            ButtonContent::Icon(_) => None,
        };
        let has_icon = !matches!(button.content, ButtonContent::Text(_));

        let mut width = padding * 2.0;
        if has_icon {
            width += ICON_SIZE;
        }
        if let Some(galley) = &galley {
            if has_icon {
                width += gap;
            }
            width += galley.size().x;
        }
        let height = self.button_height;

        let sense = if button.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);

        let hovered = response.hovered() && button.enabled;
        let pressed = response.is_pointer_button_down_on() && button.enabled;

        let fill = if pressed {
            theme::ACCENT_SOFT
        } else if button.active {
            theme::ACCENT_SOFT.gamma_multiply(0.7)
        } else if hovered {
            Color32::from_rgb(0x22, 0x27, 0x33)
        } else {
            Color32::TRANSPARENT
        };
        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, theme::RADIUS_SMALL, fill);
        }

        let color = if !button.enabled {
            theme::TEXT_FAINT
        } else if button.danger && hovered {
            theme::DANGER
        } else if hovered || button.active {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };

        let mut cursor = rect.left() + padding;
        if let ButtonContent::Icon(icon) | ButtonContent::IconText(icon, _) = &button.content {
            let icon_rect = Rect::from_min_size(
                egui::pos2(cursor, rect.center().y - ICON_SIZE / 2.0),
                vec2(ICON_SIZE, ICON_SIZE),
            );
            icon.paint(ui.painter(), icon_rect, color);
            cursor += ICON_SIZE + gap;
        }
        if let Some(galley) = galley {
            ui.painter().text(
                egui::pos2(cursor, rect.center().y),
                Align2::LEFT_CENTER,
                galley.text(),
                font,
                color,
            );
        }

        if let Some(tooltip) = &button.tooltip {
            response.clone().on_hover_text(tooltip.clone())
        } else {
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_button_can_be_text_icon_or_both() {
        assert!(matches!(
            ToolbarButton::text("a", "Open").content,
            ButtonContent::Text(text) if text == "Open"
        ));
        assert!(matches!(
            ToolbarButton::icon("b", Icon::Fit).content,
            ButtonContent::Icon(Icon::Fit)
        ));
        assert!(matches!(
            ToolbarButton::icon_text("c", Icon::Info, "About").content,
            ButtonContent::IconText(Icon::Info, text) if text == "About"
        ));
    }

    #[test]
    fn a_new_button_is_enabled_and_otherwise_plain() {
        let button = ToolbarButton::text("id", "Label");
        assert_eq!(button.id, "id");
        assert!(button.enabled);
        assert!(!button.active);
        assert!(!button.danger);
        assert!(button.tooltip.is_none());
    }

    #[test]
    fn the_builders_set_one_thing_each() {
        let button = ToolbarButton::icon("focus", Icon::Focus)
            .tooltip("Focus")
            .enabled(false)
            .active(true)
            .danger(true);
        assert_eq!(button.tooltip.as_deref(), Some("Focus"));
        assert!(!button.enabled);
        assert!(button.active);
        assert!(button.danger);
    }

    #[test]
    fn a_toolbar_holds_many_groups_of_buttons() {
        let toolbar = Toolbar::new(vec![
            ToolbarGroup::new(vec![ToolbarButton::text("open", "Open Directory")]),
            ToolbarGroup::new(vec![
                ToolbarButton::text("about", "About"),
                ToolbarButton::text("exit", "Exit"),
            ]),
        ]);
        assert_eq!(toolbar.groups.len(), 2);
        assert_eq!(toolbar.groups[1].buttons.len(), 2);
        assert_eq!(toolbar.direction, ToolbarDirection::Horizontal);
        assert_eq!(toolbar.vertical().direction, ToolbarDirection::Vertical);
    }
}
