//! A slim row of grouped text and icons describing the current state.

use eframe::egui::{self, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};

use super::icon::{Icon, ICON_SIZE};
use super::theme;

#[derive(Debug, Clone)]
pub struct StatusItem {
    pub icon: Option<Icon>,
    pub text: String,
    pub color: Color32,
    pub tooltip: Option<String>,
}

impl StatusItem {
    pub fn text(text: impl Into<String>) -> Self {
        StatusItem {
            icon: None,
            text: text.into(),
            color: theme::TEXT_DIM,
            tooltip: None,
        }
    }

    pub fn icon_text(icon: Icon, text: impl Into<String>) -> Self {
        StatusItem {
            icon: Some(icon),
            ..StatusItem::text(text)
        }
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// Items shown together, separated from neighbouring groups.
#[derive(Debug, Clone, Default)]
pub struct StatusGroup {
    pub items: Vec<StatusItem>,
}

impl StatusGroup {
    pub fn new(items: Vec<StatusItem>) -> Self {
        StatusGroup { items }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StatusBar {
    /// Groups laid out from the left; the first is the far left group.
    pub left: Vec<StatusGroup>,
    /// Groups laid out from the right.
    pub right: Vec<StatusGroup>,
}

impl StatusBar {
    pub fn show(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (index, group) in self.left.iter().enumerate() {
                if index > 0 {
                    separator(ui);
                }
                for item in &group.items {
                    show_item(ui, item);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (index, group) in self.right.iter().enumerate() {
                    if index > 0 {
                        separator(ui);
                    }
                    for item in group.items.iter().rev() {
                        show_item(ui, item);
                    }
                }
            });
        });
    }
}

fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(11.0, theme::FOOTER_HEIGHT), Sense::hover());
    let x = rect.center().x;
    ui.painter().line_segment(
        [
            egui::pos2(x, rect.top() + 5.0),
            egui::pos2(x, rect.bottom() - 5.0),
        ],
        Stroke::new(1.0, theme::BORDER),
    );
}

fn show_item(ui: &mut Ui, item: &StatusItem) {
    let font = FontId::proportional(11.5);
    let galley = ui
        .painter()
        .layout_no_wrap(item.text.clone(), font.clone(), item.color);

    let gap = 5.0;
    let mut width = 8.0;
    if item.icon.is_some() {
        width += ICON_SIZE * 0.8 + gap;
    }
    width += galley.size().x + 8.0;

    let (rect, response) =
        ui.allocate_exact_size(vec2(width, theme::FOOTER_HEIGHT), Sense::hover());

    let mut cursor = rect.left() + 8.0;
    if let Some(icon) = item.icon {
        let size = ICON_SIZE * 0.8;
        let icon_rect = Rect::from_min_size(
            egui::pos2(cursor, rect.center().y - size / 2.0),
            vec2(size, size),
        );
        icon.paint(ui.painter(), icon_rect, item.color);
        cursor += size + gap;
    }
    ui.painter().text(
        egui::pos2(cursor, rect.center().y),
        Align2::LEFT_CENTER,
        &item.text,
        font,
        item.color,
    );

    if let Some(tooltip) = &item.tooltip {
        response.on_hover_text(tooltip.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_is_text_with_an_optional_icon() {
        let plain = StatusItem::text("42 files");
        assert!(plain.icon.is_none());
        assert_eq!(plain.text, "42 files");
        assert_eq!(plain.color, theme::TEXT_DIM);

        let with_icon = StatusItem::icon_text(Icon::File, "index.pi");
        assert_eq!(with_icon.icon, Some(Icon::File));
        assert_eq!(with_icon.text, "index.pi");
    }

    #[test]
    fn an_item_can_be_recoloured_and_given_a_tooltip() {
        let item = StatusItem::text("unreached")
            .color(theme::WARN)
            .tooltip("Not reached from the entry point");
        assert_eq!(item.color, theme::WARN);
        assert_eq!(
            item.tooltip.as_deref(),
            Some("Not reached from the entry point")
        );
    }

    #[test]
    fn a_bar_keeps_its_left_and_right_groups_apart() {
        let mut bar = StatusBar::default();
        bar.left.push(StatusGroup::new(vec![StatusItem::text("/tmp")]));
        bar.left.push(StatusGroup::new(vec![StatusItem::text("3 files")]));
        bar.right.push(StatusGroup::new(vec![StatusItem::text("100%")]));

        assert_eq!(bar.left.len(), 2);
        assert_eq!(bar.right.len(), 1);
        assert_eq!(
            bar.left[0].items[0].text, "/tmp",
            "the first left group is the far left one"
        );
    }
}
