//! Line icons, drawn with the painter so they scale and follow the theme colors.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, Widget};

use crate::theme::ThemeExt;

/// The grid every icon is designed on, in its own units.
const GRID: f32 = 24.0;

/// Stroke width on the grid. It scales with the icon.
const STROKE: f32 = 1.75;

/// Segments used to approximate a circle or an arc.
const SEGMENTS: usize = 32;

/// Which icon to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    /// A gear.
    Settings,
    /// A bin.
    Trash,
    /// Three dots in a row, for a "more" menu.
    More,
    /// A plus sign.
    Plus,
    /// A cross.
    Close,
    /// A paper plane.
    Send,
    /// A square, for stopping.
    Stop,
    /// A speech bubble: stories.
    Chat,
    /// A person: characters.
    User,
    /// A globe: worlds.
    Globe,
    /// A branching line: the story tree.
    Branch,
    /// A picture frame with a mountain and a sun.
    Image,
    /// A floppy disk: save.
    Save,
    /// A folder: load.
    Folder,
    /// A circular arrow: retry.
    Refresh,
    /// A backwards arrow: undo.
    Undo,
    /// An arrow pointing right: go to.
    ArrowRight,
    /// A bookmark ribbon.
    Bookmark,
    /// A bulleted list.
    List,
    /// A page with text.
    File,
    /// A stack of disks: stored data.
    Database,
    /// A pencil: writing style.
    Pen,
    /// A question mark in a circle: help.
    Help,
    /// A line along the bottom: minimize the window.
    Minimize,
    /// Four corner brackets pointing out: enter full screen.
    Fullscreen,
    /// Four corner brackets pointing in: leave full screen.
    ExitFullscreen,
    /// An arrow into a tray: download, update.
    Download,
    /// Two overlapping sheets: copy.
    Copy,
    /// A letter i in a circle: information.
    Info,
    /// An arrow pointing left: back.
    ArrowLeft,
    /// A window with a side column: show or hide the sidebar.
    Sidebar,
    /// A shop front: the marketplace.
    Store,
    /// Arrows pressing together: compact.
    Compress,
}

impl IconKind {
    /// Every icon, for galleries and tests.
    pub const ALL: [IconKind; 33] = [
        IconKind::Settings,
        IconKind::Trash,
        IconKind::More,
        IconKind::Plus,
        IconKind::Close,
        IconKind::Send,
        IconKind::Stop,
        IconKind::Chat,
        IconKind::User,
        IconKind::Globe,
        IconKind::Branch,
        IconKind::Image,
        IconKind::Save,
        IconKind::Folder,
        IconKind::Refresh,
        IconKind::Undo,
        IconKind::ArrowRight,
        IconKind::Bookmark,
        IconKind::List,
        IconKind::File,
        IconKind::Database,
        IconKind::Pen,
        IconKind::Help,
        IconKind::Minimize,
        IconKind::Fullscreen,
        IconKind::ExitFullscreen,
        IconKind::Download,
        IconKind::Copy,
        IconKind::Info,
        IconKind::ArrowLeft,
        IconKind::Sidebar,
        IconKind::Store,
        IconKind::Compress,
    ];
}

/// Points of a circle or arc, from `start` to `end` radians, around `center`.
fn arc(center: (f32, f32), radius: f32, start: f32, end: f32) -> Vec<(f32, f32)> {
    (0..=SEGMENTS)
        .map(|i| {
            let angle = start + (end - start) * i as f32 / SEGMENTS as f32;
            (
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            )
        })
        .collect()
}

/// Points of a closed ellipse. The last point is not a repeat of the first: a repeated
/// point makes a zero-length segment, which the stroke tessellator draws as a long spike.
fn ellipse(center: (f32, f32), rx: f32, ry: f32) -> Vec<(f32, f32)> {
    (0..SEGMENTS)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / SEGMENTS as f32;
            (center.0 + rx * angle.cos(), center.1 + ry * angle.sin())
        })
        .collect()
}

/// Points of part of an ellipse, from `start` to `end` radians.
fn ellipse_arc(center: (f32, f32), rx: f32, ry: f32, start: f32, end: f32) -> Vec<(f32, f32)> {
    (0..=SEGMENTS)
        .map(|i| {
            let angle = start + (end - start) * i as f32 / SEGMENTS as f32;
            (center.0 + rx * angle.cos(), center.1 + ry * angle.sin())
        })
        .collect()
}

fn circle(center: (f32, f32), radius: f32) -> Vec<(f32, f32)> {
    ellipse(center, radius, radius)
}

/// The strokes of an icon, as open or closed polylines on the 24 x 24 grid, plus the points
/// that are drawn as filled dots.
struct Drawing {
    lines: Vec<(Vec<(f32, f32)>, bool)>,
    dots: Vec<((f32, f32), f32)>,
}

impl Drawing {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            dots: Vec::new(),
        }
    }

    fn open(mut self, points: &[(f32, f32)]) -> Self {
        self.lines.push((points.to_vec(), false));
        self
    }

    fn closed(mut self, points: &[(f32, f32)]) -> Self {
        self.lines.push((points.to_vec(), true));
        self
    }

    fn circle(mut self, center: (f32, f32), radius: f32) -> Self {
        self.lines.push((circle(center, radius), true));
        self
    }

    fn dot(mut self, center: (f32, f32), radius: f32) -> Self {
        self.dots.push((center, radius));
        self
    }
}

fn drawing(kind: IconKind) -> Drawing {
    use std::f32::consts::{PI, TAU};
    let d = Drawing::new();
    match kind {
        IconKind::Settings => {
            let mut d = d.circle((12.0, 12.0), 3.0).circle((12.0, 12.0), 7.0);
            for i in 0..8 {
                let a = TAU * i as f32 / 8.0;
                let (c, s) = (a.cos(), a.sin());
                d = d.open(&[
                    (12.0 + 7.0 * c, 12.0 + 7.0 * s),
                    (12.0 + 10.0 * c, 12.0 + 10.0 * s),
                ]);
            }
            d
        }
        IconKind::Trash => d
            .open(&[(4.0, 7.0), (20.0, 7.0)])
            .open(&[(9.0, 7.0), (9.0, 4.0), (15.0, 4.0), (15.0, 7.0)])
            .open(&[(6.0, 7.0), (7.0, 20.0), (17.0, 20.0), (18.0, 7.0)])
            .open(&[(10.0, 11.0), (10.0, 16.0)])
            .open(&[(14.0, 11.0), (14.0, 16.0)]),
        IconKind::More => d
            .dot((5.0, 12.0), 1.75)
            .dot((12.0, 12.0), 1.75)
            .dot((19.0, 12.0), 1.75),
        IconKind::Plus => d
            .open(&[(12.0, 5.0), (12.0, 19.0)])
            .open(&[(5.0, 12.0), (19.0, 12.0)]),
        IconKind::Close => d
            .open(&[(6.0, 6.0), (18.0, 18.0)])
            .open(&[(18.0, 6.0), (6.0, 18.0)]),
        IconKind::Send => d
            .closed(&[(3.0, 11.0), (21.0, 3.0), (14.0, 21.0), (11.0, 13.0)])
            .open(&[(21.0, 3.0), (11.0, 13.0)]),
        IconKind::Stop => d.closed(&[(6.0, 6.0), (18.0, 6.0), (18.0, 18.0), (6.0, 18.0)]),
        IconKind::Chat => d.closed(&[
            (4.0, 5.0),
            (20.0, 5.0),
            (20.0, 16.0),
            (12.0, 16.0),
            (7.0, 20.0),
            (7.0, 16.0),
            (4.0, 16.0),
        ]),
        IconKind::User => d
            .circle((12.0, 8.0), 4.0)
            .open(&arc((12.0, 21.0), 8.0, PI, TAU)),
        IconKind::Globe => d
            .circle((12.0, 12.0), 9.0)
            .open(&[(3.0, 12.0), (21.0, 12.0)])
            .closed(&ellipse((12.0, 12.0), 4.0, 9.0)),
        IconKind::Save => d
            .closed(&[
                (4.0, 4.0),
                (17.0, 4.0),
                (20.0, 7.0),
                (20.0, 20.0),
                (4.0, 20.0),
            ])
            .open(&[(8.0, 4.0), (8.0, 9.0), (15.0, 9.0), (15.0, 4.0)])
            .open(&[(7.0, 20.0), (7.0, 14.0), (17.0, 14.0), (17.0, 20.0)]),
        IconKind::Folder => d.closed(&[
            (3.0, 6.0),
            (10.0, 6.0),
            (12.0, 8.0),
            (21.0, 8.0),
            (21.0, 19.0),
            (3.0, 19.0),
        ]),
        IconKind::Refresh => {
            // Three quarters of a circle, with an arrow head at its end.
            let end = PI * 1.75;
            let tip = (12.0 + 8.0 * end.cos(), 12.0 + 8.0 * end.sin());
            d.open(&arc((12.0, 12.0), 8.0, PI * 0.25, end)).open(&[
                (tip.0 + 4.5, tip.1 - 1.0),
                tip,
                (tip.0 + 0.5, tip.1 + 4.5),
            ])
        }
        IconKind::Undo => d.open(&[(9.0, 6.0), (4.0, 11.0), (9.0, 16.0)]).open(&[
            (4.0, 11.0),
            (14.0, 11.0),
            (20.0, 14.0),
            (20.0, 19.0),
        ]),
        IconKind::ArrowRight => {
            d.open(&[(4.0, 12.0), (20.0, 12.0)])
                .open(&[(14.0, 6.0), (20.0, 12.0), (14.0, 18.0)])
        }
        IconKind::Bookmark => d.closed(&[
            (6.0, 3.0),
            (18.0, 3.0),
            (18.0, 21.0),
            (12.0, 16.0),
            (6.0, 21.0),
        ]),
        IconKind::List => d
            .open(&[(9.0, 6.0), (20.0, 6.0)])
            .open(&[(9.0, 12.0), (20.0, 12.0)])
            .open(&[(9.0, 18.0), (20.0, 18.0)])
            .dot((4.5, 6.0), 1.25)
            .dot((4.5, 12.0), 1.25)
            .dot((4.5, 18.0), 1.25),
        IconKind::File => d
            .closed(&[
                (5.0, 3.0),
                (14.0, 3.0),
                (19.0, 8.0),
                (19.0, 21.0),
                (5.0, 21.0),
            ])
            .open(&[(14.0, 3.0), (14.0, 8.0), (19.0, 8.0)])
            .open(&[(8.0, 13.0), (16.0, 13.0)])
            .open(&[(8.0, 17.0), (16.0, 17.0)]),
        IconKind::Database => d
            .closed(&ellipse((12.0, 6.0), 7.0, 3.0))
            .open(&[(5.0, 6.0), (5.0, 18.0)])
            .open(&[(19.0, 6.0), (19.0, 18.0)])
            .open(&ellipse_arc((12.0, 12.0), 7.0, 3.0, 0.0, PI))
            .open(&ellipse_arc((12.0, 18.0), 7.0, 3.0, 0.0, PI)),
        IconKind::Pen => d
            .closed(&[
                (15.0, 4.0),
                (20.0, 9.0),
                (9.0, 20.0),
                (3.0, 21.0),
                (4.0, 15.0),
            ])
            .open(&[(12.0, 7.0), (17.0, 12.0)]),
        IconKind::Help => d
            .circle((12.0, 12.0), 9.0)
            .open(&arc((12.0, 10.0), 2.75, PI, PI * 2.5))
            .open(&[(12.0, 12.75), (12.0, 14.5)])
            .dot((12.0, 17.5), 1.1),
        IconKind::Minimize => d.open(&[(5.0, 18.0), (19.0, 18.0)]),
        IconKind::Fullscreen => d
            .open(&[(4.0, 9.0), (4.0, 4.0), (9.0, 4.0)])
            .open(&[(15.0, 4.0), (20.0, 4.0), (20.0, 9.0)])
            .open(&[(20.0, 15.0), (20.0, 20.0), (15.0, 20.0)])
            .open(&[(9.0, 20.0), (4.0, 20.0), (4.0, 15.0)]),
        IconKind::ExitFullscreen => d
            .open(&[(4.0, 9.0), (9.0, 9.0), (9.0, 4.0)])
            .open(&[(15.0, 4.0), (15.0, 9.0), (20.0, 9.0)])
            .open(&[(20.0, 15.0), (15.0, 15.0), (15.0, 20.0)])
            .open(&[(9.0, 20.0), (9.0, 15.0), (4.0, 15.0)]),
        IconKind::Download => d
            .open(&[(12.0, 4.0), (12.0, 15.0)])
            .open(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)])
            .open(&[(4.0, 16.0), (4.0, 20.0), (20.0, 20.0), (20.0, 16.0)]),
        IconKind::Copy => d
            .closed(&[(9.0, 9.0), (20.0, 9.0), (20.0, 20.0), (9.0, 20.0)])
            .open(&[
                (15.0, 9.0),
                (15.0, 4.0),
                (4.0, 4.0),
                (4.0, 15.0),
                (9.0, 15.0),
            ]),
        IconKind::Info => d
            .circle((12.0, 12.0), 9.0)
            .dot((12.0, 7.75), 1.1)
            .open(&[(12.0, 11.0), (12.0, 17.0)]),
        IconKind::ArrowLeft => {
            d.open(&[(20.0, 12.0), (4.0, 12.0)])
                .open(&[(10.0, 6.0), (4.0, 12.0), (10.0, 18.0)])
        }
        IconKind::Sidebar => d
            .closed(&[(3.0, 4.0), (21.0, 4.0), (21.0, 20.0), (3.0, 20.0)])
            .open(&[(9.0, 4.0), (9.0, 20.0)]),
        IconKind::Store => d
            .closed(&[(4.0, 10.0), (4.0, 20.0), (20.0, 20.0), (20.0, 10.0)])
            .closed(&[(3.0, 4.0), (21.0, 4.0), (22.0, 10.0), (2.0, 10.0)])
            .open(&[(10.0, 20.0), (10.0, 14.0), (14.0, 14.0), (14.0, 20.0)]),
        IconKind::Compress => d
            .open(&[(12.0, 2.5), (12.0, 10.0)])
            .open(&[(8.5, 7.0), (12.0, 10.5), (15.5, 7.0)])
            .open(&[(12.0, 21.5), (12.0, 14.0)])
            .open(&[(8.5, 17.0), (12.0, 13.5), (15.5, 17.0)])
            .open(&[(4.0, 12.0), (20.0, 12.0)]),
        IconKind::Image => d
            .closed(&[(3.0, 5.0), (21.0, 5.0), (21.0, 19.0), (3.0, 19.0)])
            .circle((9.0, 10.0), 1.75)
            .open(&[
                (3.0, 17.0),
                (9.0, 13.0),
                (14.0, 17.0),
                (17.0, 14.5),
                (21.0, 17.5),
            ]),
        IconKind::Branch => d
            .circle((6.0, 5.0), 2.0)
            .circle((6.0, 19.0), 2.0)
            .circle((18.0, 8.0), 2.0)
            .open(&[(6.0, 7.0), (6.0, 17.0)])
            .open(&[(18.0, 10.0), (18.0, 12.0), (15.0, 15.0), (6.0, 15.0)]),
    }
}

/// Paint `kind` into `rect` with `color`.
pub fn paint(painter: &Painter, rect: Rect, color: Color32, kind: IconKind) {
    let scale = rect.width().min(rect.height()) / GRID;
    let origin = rect.center() - Vec2::splat(GRID * scale / 2.0);
    let map = |(x, y): (f32, f32)| Pos2::new(origin.x + x * scale, origin.y + y * scale);
    let stroke = Stroke::new(STROKE * scale, color);
    let drawing = drawing(kind);
    for (points, closed) in &drawing.lines {
        let points: Vec<Pos2> = points.iter().copied().map(map).collect();
        painter.add(if *closed {
            Shape::closed_line(points, stroke)
        } else {
            Shape::line(points, stroke)
        });
    }
    for (center, radius) in &drawing.dots {
        painter.circle_filled(map(*center), radius * scale, color);
    }
}

/// An icon as a widget. It is `size` logical pixels square and, unless told otherwise,
/// drawn in the theme's foreground color.
#[must_use = "add the icon with `ui.add(...)`"]
pub struct Icon {
    kind: IconKind,
    size: f32,
    color: Option<Color32>,
}

impl Icon {
    /// An icon, 16 logical pixels square.
    pub fn new(kind: IconKind) -> Self {
        Self {
            kind,
            size: 16.0,
            color: None,
        }
    }

    /// The width and height in logical pixels.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Draw in this color instead of the foreground color.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for Icon {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), Sense::hover());
        let color = self.color.unwrap_or_else(|| ui.ir_theme().foreground);
        paint(ui.painter(), rect, color, self.kind);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::Harness;

    #[test]
    fn every_icon_has_something_to_draw() {
        for kind in IconKind::ALL {
            let d = drawing(kind);
            assert!(!d.lines.is_empty() || !d.dots.is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn no_line_repeats_a_point_back_to_back() {
        for kind in IconKind::ALL {
            for (points, closed) in &drawing(kind).lines {
                let mut pairs: Vec<_> = points.windows(2).map(|w| (w[0], w[1])).collect();
                if *closed {
                    pairs.push((points[points.len() - 1], points[0]));
                }
                assert!(
                    pairs.iter().all(|(a, b)| a != b),
                    "{kind:?} has a zero-length segment"
                );
            }
        }
    }

    #[test]
    fn every_icon_stays_inside_the_grid() {
        for kind in IconKind::ALL {
            let d = drawing(kind);
            let points = d
                .lines
                .iter()
                .flat_map(|(p, _)| p.iter().copied())
                .chain(d.dots.iter().map(|(c, _)| *c));
            for (x, y) in points {
                assert!(
                    (0.0..=GRID).contains(&x) && (0.0..=GRID).contains(&y),
                    "{kind:?} has a point outside the grid: ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn the_widget_is_square_and_paints_shapes() {
        let mut h = Harness::new();
        let mut sizes = Vec::new();
        h.idle(&mut |ui| {
            sizes.clear();
            for kind in IconKind::ALL {
                sizes.push(ui.add(Icon::new(kind).size(20.0)).rect.size());
            }
        });
        assert!(sizes
            .iter()
            .all(|s| (s.x - 20.0).abs() < 0.01 && (s.y - 20.0).abs() < 0.01));
    }

    #[test]
    fn painting_adds_shapes_for_every_icon() {
        let ctx = egui::Context::default();
        for kind in IconKind::ALL {
            let output = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(24.0));
                    paint(ui.painter(), rect, Color32::WHITE, kind);
                });
            });
            let icon_shapes = output
                .shapes
                .iter()
                .filter(|s| !matches!(s.shape, Shape::Noop))
                .count();
            assert!(icon_shapes > 1, "{kind:?} painted {icon_shapes} shapes");
        }
    }
}
