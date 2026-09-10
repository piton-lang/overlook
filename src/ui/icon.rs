//! Small vector icons, painted rather than loaded, so the application has no
//! image or icon font dependency.

use eframe::egui::{vec2, Color32, Painter, Pos2, Rect, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    FolderOpen,
    Info,
    Power,
    Fit,
    ZoomIn,
    ZoomOut,
    File,
    Graph,
    Warning,
    Focus,
    Save,
    Revert,
    Close,
}

impl Icon {
    /// Paint the icon centred in `rect`, scaled to fit it.
    pub fn paint(self, painter: &Painter, rect: Rect, color: Color32) {
        let size = rect.width().min(rect.height());
        let unit = size / 16.0;
        let stroke = Stroke::new((unit * 1.4).max(1.0), color);
        let origin = rect.center() - vec2(size / 2.0, size / 2.0);
        let p = |x: f32, y: f32| -> Pos2 { origin + vec2(x * unit, y * unit) };

        match self {
            Icon::FolderOpen => {
                painter.add(line(
                    vec![p(2.0, 13.0), p(2.0, 4.0), p(6.5, 4.0), p(8.0, 6.0), p(13.0, 6.0)],
                    stroke,
                ));
                painter.add(line(
                    vec![p(2.0, 13.0), p(4.5, 8.0), p(15.0, 8.0), p(12.5, 13.0), p(2.0, 13.0)],
                    stroke,
                ));
            }
            Icon::Info => {
                painter.circle_stroke(rect.center(), size * 0.42, stroke);
                painter.line_segment([p(8.0, 7.5), p(8.0, 11.5)], stroke);
                painter.circle_filled(p(8.0, 5.0), unit * 0.9, color);
            }
            Icon::Power => {
                let center = p(8.0, 9.0);
                let radius = unit * 5.0;
                let mut arc = Vec::new();
                for step in 0..=24 {
                    let t = step as f32 / 24.0;
                    let angle = -std::f32::consts::FRAC_PI_2 + 0.7 + t * (std::f32::consts::TAU - 1.4);
                    arc.push(center + vec2(angle.cos() * radius, angle.sin() * radius));
                }
                painter.add(line(arc, stroke));
                painter.line_segment([p(8.0, 1.5), p(8.0, 7.0)], stroke);
            }
            Icon::Fit => {
                let corners = [
                    (p(2.0, 6.0), p(2.0, 2.0), p(6.0, 2.0)),
                    (p(10.0, 2.0), p(14.0, 2.0), p(14.0, 6.0)),
                    (p(14.0, 10.0), p(14.0, 14.0), p(10.0, 14.0)),
                    (p(6.0, 14.0), p(2.0, 14.0), p(2.0, 10.0)),
                ];
                for (a, b, c) in corners {
                    painter.add(line(vec![a, b, c], stroke));
                }
            }
            Icon::ZoomIn | Icon::ZoomOut => {
                let center = p(7.0, 7.0);
                painter.circle_stroke(center, unit * 4.5, stroke);
                painter.line_segment([p(10.4, 10.4), p(14.0, 14.0)], stroke);
                painter.line_segment([p(4.5, 7.0), p(9.5, 7.0)], stroke);
                if self == Icon::ZoomIn {
                    painter.line_segment([p(7.0, 4.5), p(7.0, 9.5)], stroke);
                }
            }
            Icon::File => {
                painter.add(line(
                    vec![p(4.0, 14.0), p(4.0, 2.0), p(9.5, 2.0), p(12.5, 5.0), p(12.5, 14.0), p(4.0, 14.0)],
                    stroke,
                ));
                painter.add(line(vec![p(9.5, 2.0), p(9.5, 5.0), p(12.5, 5.0)], stroke));
            }
            Icon::Graph => {
                painter.circle_stroke(p(4.0, 4.0), unit * 2.0, stroke);
                painter.circle_stroke(p(12.0, 7.0), unit * 2.0, stroke);
                painter.circle_stroke(p(5.5, 12.5), unit * 2.0, stroke);
                painter.line_segment([p(5.8, 5.1), p(10.2, 6.4)], stroke);
                painter.line_segment([p(10.7, 8.6), p(7.0, 11.3)], stroke);
            }
            Icon::Focus => {
                painter.circle_stroke(rect.center(), size * 0.27, stroke);
                painter.line_segment([p(8.0, 1.0), p(8.0, 4.0)], stroke);
                painter.line_segment([p(8.0, 12.0), p(8.0, 15.0)], stroke);
                painter.line_segment([p(1.0, 8.0), p(4.0, 8.0)], stroke);
                painter.line_segment([p(12.0, 8.0), p(15.0, 8.0)], stroke);
            }
            Icon::Save => {
                painter.add(line(
                    vec![
                        p(3.0, 3.0),
                        p(10.5, 3.0),
                        p(13.0, 5.5),
                        p(13.0, 13.0),
                        p(3.0, 13.0),
                        p(3.0, 3.0),
                    ],
                    stroke,
                ));
                // The slot at the top and the label at the bottom of a disk.
                painter.add(line(
                    vec![p(5.5, 3.0), p(5.5, 6.5), p(10.0, 6.5), p(10.0, 3.0)],
                    stroke,
                ));
                painter.add(line(
                    vec![p(5.0, 13.0), p(5.0, 9.5), p(11.0, 9.5), p(11.0, 13.0)],
                    stroke,
                ));
            }
            Icon::Revert => {
                // An arrow curling back the way it came.
                let center = p(8.0, 9.5);
                let radius = unit * 4.5;
                let mut arc = Vec::new();
                for step in 0..=20 {
                    let t = step as f32 / 20.0;
                    let angle = std::f32::consts::PI + t * std::f32::consts::PI * 0.85;
                    arc.push(center + vec2(angle.cos() * radius, angle.sin() * radius));
                }
                painter.add(line(arc, stroke));
                painter.add(line(
                    vec![p(1.3, 7.2), p(3.5, 10.2), p(5.7, 7.2)],
                    stroke,
                ));
            }
            Icon::Close => {
                painter.line_segment([p(4.0, 4.0), p(12.0, 12.0)], stroke);
                painter.line_segment([p(12.0, 4.0), p(4.0, 12.0)], stroke);
            }
            Icon::Warning => {
                painter.add(line(
                    vec![p(8.0, 2.0), p(15.0, 13.5), p(1.0, 13.5), p(8.0, 2.0)],
                    stroke,
                ));
                painter.line_segment([p(8.0, 6.5), p(8.0, 10.0)], stroke);
                painter.circle_filled(p(8.0, 11.8), unit * 0.8, color);
            }
        }
    }
}

fn line(points: Vec<Pos2>, stroke: Stroke) -> eframe::egui::Shape {
    eframe::egui::Shape::line(points, stroke)
}

/// The nominal square an icon is drawn in.
pub const ICON_SIZE: f32 = 16.0;
