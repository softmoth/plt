use crate::{PltError, FileFormat, Color, Backend, CairoBackend};
use crate::subplot::{
    Subplot, TickDirection, TickSpacing, TickLabels, Grid, Limits, LineStyle,
    AxisType, Line, MarkerStyle,
};

use std::{iter, ops, path};
use std::collections::HashMap;

/// Describes the configuration of a [`Figure`].
#[derive(Clone, Debug)]
pub struct FigureDescriptor {
    /// The size, in dots (pixels), of the figure.
    pub figsize: (f32, f32),
    /// The dots (pixels) per inch of the figure.
    pub dpi: u16,
    /// The background color of the figure.
    pub face_color: Color,
}
impl Default for FigureDescriptor {
    fn default() -> Self {
        Self {
            figsize: (6.75, 2.),
            dpi: 100,
            face_color: Color::WHITE,
        }
    }
}

/// Represents a whole figure, containing subplots, which can be drawn as an image.
#[derive(Debug)]
pub struct Figure<'a, B: Backend = CairoBackend> {
    subplots: Vec<Subplot<'a>>,
    subplot_areas: Vec<draw::Area>,
    size: draw::Size,
    scaling: f32,
    dpi: u16,
    face_color: Color,
    phantom: std::marker::PhantomData<B>,
}
impl<'a, B: Backend> Figure<'a, B> {
    /// The main constructor.
    pub fn new(desc: &FigureDescriptor) -> Self {
        // scaling factor for different DPIs
        let scaling = desc.dpi as f32 / FigureDescriptor::default().dpi as f32;

        // size of figure in pixels
        let width = (desc.figsize.0 * desc.dpi as f32).floor() as u32;
        let height = (desc.figsize.0 * desc.dpi as f32).floor() as u32;

        Self {
            subplots: Vec::new(),
            subplot_areas: Vec::new(),
            size: draw::Size { width, height },
            scaling,
            dpi: desc.dpi,
            face_color: desc.face_color,
            phantom: std::marker::PhantomData,
        }
    }

    /// Adds a subplot to the figure in a 1-indexed location defined by a grid scheme.
    pub fn add_subplot<'b>(
        &'b mut self,
        (nrows, ncols, index): (u32, u32, u32),
        subplot: Subplot<'a>,
    ) -> Result<&mut Subplot<'a>, PltError> where 'a: 'b {
        // check that index is valid
        if index > nrows * ncols || index == 0 {
            return Err(PltError::InvalidIndex { index, nrows, ncols })
        }

        // get zero indexed row and column numbers
        let row = (index - 1) / ncols;
        let col = (index - 1) % ncols;

        // get extents in pixel counts
        let xextent = (self.size.width / ncols) as f32;
        let yextent = (self.size.height / nrows) as f32;

        let xmin = (xextent * col as f32).ceil() as u32;
        let xmax = (xmin as f32 + xextent).floor() as u32;
        let ymin = (yextent * (nrows - 1 - row) as f32).ceil() as u32;
        let ymax = (ymin as f32 + yextent).floor() as u32;

        self.subplot_areas.push(draw::Area { xmin, xmax, ymin, ymax });
        self.subplots.push(subplot);

        Ok(self.subplots.last_mut().unwrap())
    }

    /// Draw figure to provided backend.
    pub fn draw_to_backend(
        &mut self, backend: &mut B,
    ) -> Result<(), PltError> {
        let old_size = self.size;
        self.size = backend.size();

        for (subplot, subplot_area) in iter::zip(&self.subplots, &self.subplot_areas) {
            draw_subplot(backend, subplot, subplot_area, self.scaling)?;
        }

        self.size = old_size;

        Ok(())
    }

    /// Draw figure to a file.
    pub fn draw_file<P: AsRef<path::Path>>(
        &self, format: FileFormat, filename: P,
    ) -> Result<(), PltError> {
        // create canvas to draw to
        let graphics_type = match format {
            FileFormat::Png | FileFormat::Jpeg => draw::GraphicsType::Bitmap,
            FileFormat::Svg => draw::GraphicsType::Vector,
        };
        let mut canvas = B::new(draw::CanvasDescriptor {
            size: self.size,
            face_color: self.face_color,
            graphics_type,
        });

        for (subplot, subplot_area) in iter::zip(&self.subplots, &self.subplot_areas) {
            draw_subplot(&mut canvas, subplot, subplot_area, self.scaling)?;
        }

        // save to file
        canvas.save_file(draw::SaveFileDescriptor {
            filename: filename.as_ref(),
            format,
            dpi: self.dpi,
        });

        Ok(())
    }

    /// Get reference to held subplots.
    pub fn subplots<'b>(
        &'b mut self
    ) -> &mut Vec<Subplot<'a>> where 'a: 'b {
        &mut self.subplots
    }
}

// private

struct SubplotList<'a> {
    subplots: &'a mut Vec<Subplot<'a>>,
    rows: usize,
}
impl<'a> ops::Index<(usize, usize)> for SubplotList<'a> {
    type Output = Subplot<'a>;

    fn index(&self, indicies: (usize, usize)) -> &Self::Output {
        &self.subplots[indicies.0 + self.rows * indicies.1]
    }
}
impl ops::IndexMut<(usize, usize)> for SubplotList<'_> {
    fn index_mut(&mut self, indicies: (usize, usize)) -> &mut Self::Output {
        &mut self.subplots[indicies.0 + self.rows * indicies.1]
    }
}

struct AxisFinalized {
    pub label: String,
    pub major_tick_locs: Vec<f64>,
    pub major_tick_labels: Vec<String>,
    pub minor_tick_locs: Vec<f64>,
    pub minor_tick_labels: Vec<String>,
    pub label_multiplier: i32,
    pub label_offset: f64,
    pub major_grid: bool,
    pub minor_grid: bool,
    pub limits: (f64, f64),
}

fn sigdigit(mut num: f64) -> i32 {
    if num == 0.0 {
        return i32::MIN
    }

    if num > 1.0 {
        let mut ret = 0;
        while num >= 10.0 {
            num /= 10.0;
            ret += 1;
        }
        ret
    } else {
        let mut ret = 0;
        while num < 1.0 {
            num *= 10.0;
            ret -= 1;
        }
        ret
    }
}

fn decimals(mut num: f64, ndigits: u8) -> Vec<u8> {
    let mut decimals = Vec::with_capacity(ndigits as usize);
    for _ in 0..ndigits {
        num -= num.floor();
        num *= 10.0;
        decimals.push(num.floor() as u8);
    }

    decimals
}

fn round_to(num: f64, place: i32) -> f64 {
    (num * f64::powi(10.0, place)).round() / f64::powi(10.0, place)
}

fn superscript(n: u16) -> String {
    if n == 0 {
        "⁰".to_owned()
    } else if n == 1 {
        "¹".to_owned()
    } else if n == 2 {
        "²".to_owned()
    } else if n == 3 {
        "³".to_owned()
    } else if n == 4 {
        "⁴".to_owned()
    } else if n == 5 {
        "⁵".to_owned()
    } else if n == 6 {
        "⁶".to_owned()
    } else if n == 7 {
        "⁷".to_owned()
    } else if n == 8 {
        "⁸".to_owned()
    } else if n == 9 {
        "⁹".to_owned()
    } else if n >= 10 {
        superscript(n / 10) + &superscript(n % 10)
    } else {
        "".to_owned()
    }
}

fn tick_modifiers(ticks: &[f64]) -> Result<(f64, i32, usize), PltError> {
    // make sure there are no NaNs
    if ticks.iter().any(|&tick| tick.is_nan()) {
        return Err(PltError::BadTickPlacement("tick is NaN".to_owned()));
    }

    // return empty labels for empty ticks
    if ticks.is_empty() {
        return Ok((0.0, 0, 0));
    }

    // find the highest most significant digit location
    let mut max_multiplier = sigdigit(*ticks.last().unwrap());

    // get differences between ticks
    let difs = ticks
        .windows(2)
        .map(|window| {
            window[1] - window[0]
        })
        .collect::<Vec<_>>();
    // find the largest difference between any two consecutive ticks
    let max_dif = *difs.iter()
        .reduce(|max, dif| if dif > max {dif} else {max})
        .unwrap();
    // find the highest most significant digit of the max tick difference
    let dif_multiplier = if max_dif != 0.0 { sigdigit(max_dif) } else { max_multiplier };

    // if multiplier of max dif is less than max_multiplier - 3, use offset
    let offset = if dif_multiplier < max_multiplier - 3 {
        ticks[0]
    } else {
        0.0
    };

    // get true multiplier
    max_multiplier = sigdigit(round_to(*ticks.last().unwrap() - offset, 3 - dif_multiplier));
    let multiplier = if !(-2..=3).contains(&max_multiplier) {
        max_multiplier
    } else {
        0
    };

    // get precision
    let max_precision = if multiplier != 0 || max_multiplier < 0 {
        3
    } else {
        3 - max_multiplier
    };
    let shifted_ticks = if multiplier != 0 {
        ticks.iter()
            .map(|&tick| {
                let rounded = (tick * f64::powi(10.0, 3 - multiplier)).round();
                rounded * f64::powi(10.0, -3)
            })
            .collect::<Vec<_>>()
    } else {
        ticks.to_vec()
    };
    let precision = shifted_ticks.iter()
        .map(|&tick| {
            decimals(tick, max_precision as u8)
                .iter()
                .rposition(|&digit| {
                    digit != 0
                })
                .map(|prec| {
                    prec + 1
                })
                .unwrap_or(0)
        })
        .max()
        .unwrap();

    Ok((offset, multiplier, precision))
}

fn ticks_to_labels(ticks: &[f64], modifiers: (f64, i32, usize)) -> Result<Vec<String>, PltError> {
    // make sure there are no NaNs
    if ticks.iter().any(|&tick| tick.is_nan()) {
        return Err(PltError::BadTickPlacement("tick is NaN".to_owned()));
    }

    // return empty labels for empty ticks
    if ticks.is_empty() {
        return Ok(vec![]);
    }

    let (offset, multiplier, precision) = modifiers;

    // sort ticks
    let mut ticks = ticks.to_vec();
    ticks.sort_by(|a, b| a.partial_cmp(b).unwrap());

    for tick in ticks.iter_mut() {
        *tick = round_to(*tick - offset, 4 - multiplier);
    }

    // shift numbers if necessary
    let shifted_ticks = if multiplier != 0 {
        ticks.iter()
            .map(|&tick| {
                let rounded = (tick * f64::powi(10.0, 3 - multiplier)).round();
                rounded * f64::powi(10.0, -3)
            })
            .collect::<Vec<_>>()
    } else {
        ticks.to_vec()
    };

    let labels = shifted_ticks.iter()
        .map(|tick| {
            format!("{0:.1$}", tick, precision)
        })
        .collect::<Vec<_>>();

    Ok(labels)
}

fn draw_subplot<B: Backend>(
    canvas: &mut B,
    subplot: &Subplot,
    subplot_area: &draw::Area,
    scaling: f32,
) -> Result<(), PltError> {
    // set formatting parameters

    // line formatting
    let line_width = subplot.format.line_width * scaling.round() as u32;
    let line_color = subplot.format.line_color;

    let grid_color = subplot.format.grid_color;

    // text formatting
    let font_name = subplot.format.font_name;
    let font_size = subplot.format.font_size * scaling;
    let font_color = subplot.format.text_color;

    // colors
    let default_marker_color = subplot.format.default_marker_color;

    // major tick formatting
    let inner_major_tick_length = match subplot.format.tick_direction {
        TickDirection::Inner | TickDirection::Both => {
            subplot.format.tick_length * scaling.round() as u32
        },
        _ => 0,
    };
    let outer_major_tick_length = match subplot.format.tick_direction {
        TickDirection::Outer | TickDirection::Both => {
            subplot.format.tick_length * scaling.round() as u32
        },
        _ => 0,
    };
    // minor tick formatting
    let inner_minor_tick_length = match subplot.format.tick_direction {
        TickDirection::Inner | TickDirection::Both => {
            if let Some(length) = subplot.format.override_minor_tick_length {
                length * scaling.round() as u32
            } else {
                subplot.format.tick_length * scaling.round() as u32 / 2
            }
        },
        _ => 0,
    };
    let outer_minor_tick_length = match subplot.format.tick_direction {
        TickDirection::Outer | TickDirection::Both => {
            if let Some(length) = subplot.format.override_minor_tick_length {
                length * scaling.round() as u32
            } else {
                subplot.format.tick_length * scaling.round() as u32 / 2
            }
        },
        _ => 0,
    };

    // layout depends on the font size
    let letter_size = canvas.text_size(draw::TextDescriptor {
        text: format!("{}", 0),
        font: draw::Font {
            name: font_name,
            size: font_size / scaling,
            ..Default::default()
        },
        ..Default::default()
    });
    let letter_size = draw::Size {
        width: (letter_size.width as f32 * scaling) as u32,
        height: (letter_size.height as f32 * scaling) as u32,
    };

    // the pixel buffer sizes for fitting text on the figure sides
    let buffer_offset = ( (letter_size.height as f64) * 0.6 ) as u32;
    let mut subplot_buffer = HashMap::from([
        (AxisType::Y, 0),
        (AxisType::SecondaryY, 0),
        (AxisType::SecondaryX, 0),
        (AxisType::X, 0),
    ]);
    let mut label_buffer = HashMap::from([
        (AxisType::Y, 0),
        (AxisType::SecondaryY, 0),
        (AxisType::SecondaryX, 0),
        (AxisType::X, 0),
    ]);
    let mut modifier_buffer = HashMap::from([
        (AxisType::Y, 0),
        (AxisType::SecondaryY, 0),
        (AxisType::SecondaryX, 0),
        (AxisType::X, 0),
    ]);
    let mut tick_label_buffer = HashMap::from([
        (AxisType::Y, 0),
        (AxisType::SecondaryY, 0),
        (AxisType::SecondaryX, 0),
        (AxisType::X, 0),
    ]);
    let mut tick_buffer = HashMap::from([
        (AxisType::Y, 0),
        (AxisType::X, 0),
        (AxisType::SecondaryY, 0),
        (AxisType::SecondaryX, 0),
    ]);

    // get span and limit requirements from each plot, for each axis
    type SpanTuple = (f64, f64);
    type LimitsTuple = (f64, f64);
    let span_limits: HashMap<AxisType, Option<(SpanTuple, LimitsTuple)>> = AxisType::iter()
        .map(|placement| {
            let axis = match placement {
                AxisType::Y => &subplot.yaxis,
                AxisType::X => &subplot.xaxis,
                AxisType::SecondaryY => &subplot.secondary_yaxis,
                AxisType::SecondaryX => &subplot.secondary_xaxis,
            };

            // span is extent of the data being plotted
            // limits is the extent of the actual plot area
            // span <= limits
            let span_lims = if let Limits::Manual { min, max } = axis.limits {
                Some((
                    (min, max), // span
                    (min, max), // limits
                ))
            } else {
                let lim = subplot.plot_infos.iter()
                    .filter(|info| info.xaxis == placement || info.yaxis == placement)
                    .map(|info| match placement {
                        AxisType::Y | AxisType::SecondaryY => (info.data.ymin(), info.data.ymax()),
                        AxisType::X | AxisType::SecondaryX => (info.data.xmin(), info.data.xmax()),
                    })
                    .reduce(|(min, max), (next_min, next_max)| {
                        let min = if next_min <= min { next_min } else { min };
                        let max = if next_max >= max { next_max } else { max };

                        (min, max)
                    });

                if let Some((min, max)) = lim {
                    let extent = max - min;

                    Some((
                        (min, max), // span
                        (min - 0.05*extent, max + 0.05*extent), // limits
                    ))
                } else {
                    None
                }
            };

            (placement, span_lims)
        })
        .collect();

    // get ticks and tick labels
    let mut finalized_axes = HashMap::<AxisType, AxisFinalized>::new();
    for placement in AxisType::iter() {
        let axis = match placement {
            AxisType::Y => &subplot.yaxis,
            AxisType::X => &subplot.xaxis,
            AxisType::SecondaryY => &subplot.secondary_yaxis,
            AxisType::SecondaryX => &subplot.secondary_xaxis,
        };

        // get span and limits for each axis, if None, use values from opposite side
        let (span, limits) = if let Some((span, limits)) = span_limits[&placement] {
            (span, limits)
        } else {
            match placement {
                // use opposite side, if it has a value, otherwise default to (-1.0, 1.0)
                AxisType::Y => span_limits[&AxisType::SecondaryY].unwrap_or(
                    ( (-1.0, 1.0), (-1.0, 1.0) )
                ),
                AxisType::SecondaryY => span_limits[&AxisType::Y].unwrap_or(
                    ( (-1.0, 1.0), (-1.0, 1.0) )
                ),
                AxisType::X => span_limits[&AxisType::SecondaryX].unwrap_or(
                    ( (-1.0, 1.0), (-1.0, 1.0) )
                ),
                AxisType::SecondaryX => span_limits[&AxisType::X].unwrap_or(
                    ( (-1.0, 1.0), (-1.0, 1.0) )
                ),
            }
        };

        // get major tick marks
        let major_ticks = if let TickSpacing::Manual(
            ticks
        ) = &axis.major_ticks.spacing {
            ticks.clone()
        } else {
            let nticks = match &axis.major_ticks.spacing {
                TickSpacing::Count(n) => *n,
                TickSpacing::Auto => 5,
                _ => 0,
            };

            (0..nticks)
                .map(|n| {
                    span.0 + (span.1 - span.0) * (n as f64 / (nticks - 1) as f64)
                })
                .collect::<Vec<_>>()
        };
        // get minor tick marks
        let minor_ticks = if let TickSpacing::Manual(
            ticks
        ) = &axis.minor_ticks.spacing {
            ticks.clone()
        } else {
            let nticks = match &axis.minor_ticks.spacing {
                TickSpacing::Count(n) => *n,
                TickSpacing::Auto => 5 * major_ticks.len() as u16,
                _ => 0,
            };

            (0..nticks)
                .map(|n| {
                    span.0 + (span.1 - span.0) * (n as f64 / (nticks - 1) as f64)
                })
                .collect::<Vec<_>>()
        };
        // remove overlap between major and minor ticks
        let minor_ticks = minor_ticks.iter()
            .filter(|tick| !major_ticks.contains(tick))
            .copied()
            .collect::<Vec<_>>();

        // get tick labels
        let (major_labels, multiplier, offset) = if let TickLabels::Manual {
            labels, multiplier, offset,
        } = &axis.major_ticks.labels {
            (labels.clone(), *multiplier, *offset)
        } else {
            let modifiers = tick_modifiers(major_ticks.as_slice())?;
            let labels = ticks_to_labels(major_ticks.as_slice(), modifiers)?;
            (labels, modifiers.1, modifiers.0)
        };
        let minor_labels = if let TickLabels::Manual {
            labels, multiplier: _, offset: _,
        } = &axis.minor_ticks.labels {
            labels.clone()
        } else {
            let modifiers = tick_modifiers(major_ticks.as_slice())?; // use major modifiers
            ticks_to_labels(minor_ticks.as_slice(), modifiers)?
        };

        let (major_grid, minor_grid) = match axis.grid {
            Grid::None => (false, false),
            Grid::Major => (true, false),
            Grid::Full => (true, true),
        };

        // adjust buffers

        // add space for outer tick marks if necessary
        if !major_ticks.is_empty() {
            *tick_buffer.get_mut(&placement).unwrap() += outer_major_tick_length;
        } else if !minor_ticks.is_empty() {
            *tick_buffer.get_mut(&placement).unwrap() += outer_minor_tick_length;
        }

        // add space for tick labels if necessary
        if !major_labels.is_empty() {
            let tick_label_size = match placement {
                AxisType::Y | AxisType::SecondaryY => {
                    5 * letter_size.width
                },
                AxisType::SecondaryX | AxisType::X => {
                    letter_size.height
                },
            };
            *modifier_buffer.get_mut(&placement).unwrap() += tick_label_size;
            *tick_buffer.get_mut(&placement).unwrap() += buffer_offset;
        } else if !minor_labels.is_empty() {
            let tick_label_size = match placement {
                AxisType::Y | AxisType::SecondaryY => {
                    5 * letter_size.width
                },
                AxisType::SecondaryX | AxisType::X => {
                    letter_size.height
                },
            };
            *modifier_buffer.get_mut(&placement).unwrap() += tick_label_size;
            *tick_buffer.get_mut(&placement).unwrap() += buffer_offset;
        }

        // add space for multiplier and offset if necessary
        if multiplier != 0 || offset != 0.0 {
            match placement {
                AxisType::Y => {
                    *modifier_buffer.get_mut(&AxisType::SecondaryX).unwrap() += letter_size.height * 2 / 3;
                    *tick_label_buffer.get_mut(&AxisType::SecondaryX).unwrap() += buffer_offset;
                },
                AxisType::X => {
                    *modifier_buffer.get_mut(&AxisType::X).unwrap() += letter_size.height * 2 / 3;
                    *tick_label_buffer.get_mut(&AxisType::X).unwrap() += buffer_offset;
                },
                _ => {},
            };
        }

        // add space for axis label if necessary
        if !axis.label.is_empty() {
            //*label_buffer.get_mut(&placement).unwrap() += letter_size.height * 3 / 2;
            *label_buffer.get_mut(&placement).unwrap() += letter_size.height;
            *tick_label_buffer.get_mut(&placement).unwrap() += buffer_offset;
        }

        // adjust total subplot buffer
        *subplot_buffer.get_mut(&placement).unwrap() = if (
            tick_buffer[&placement] + tick_label_buffer[&placement]
            + modifier_buffer[&placement] + label_buffer[&placement]
        ) < letter_size.width * 2 {
            letter_size.width * 3
        } else {
            buffer_offset
        };

        // save finalized axis info
        finalized_axes.insert(
            placement,
            AxisFinalized {
                label: axis.label.clone(),
                major_tick_locs: major_ticks,
                major_tick_labels: major_labels,
                minor_tick_locs: minor_ticks,
                minor_tick_labels: minor_labels,
                label_multiplier: multiplier,
                label_offset: offset,
                major_grid,
                minor_grid,
                limits,
            },
        );
    }

    // add space for title
    let mut title_buffer = 0;
    if !subplot.title.is_empty() {
        title_buffer += letter_size.height;
        *label_buffer.get_mut(&AxisType::SecondaryX).unwrap() += buffer_offset;
    }

    // setup figure areas

    let title_boundary = subplot_area.ymax - subplot_buffer[&AxisType::SecondaryX] - title_buffer;

    let label_boundary = draw::Area {
        xmin: subplot_area.xmin + subplot_buffer[&AxisType::Y] + label_buffer[&AxisType::Y],
        xmax: subplot_area.xmax - subplot_buffer[&AxisType::SecondaryY] - label_buffer[&AxisType::SecondaryY],
        ymin: subplot_area.ymin + subplot_buffer[&AxisType::X] + label_buffer[&AxisType::X],
        ymax: title_boundary - label_buffer[&AxisType::SecondaryX],
    };
    let modifier_boundary = draw::Area {
        xmin: label_boundary.xmin + modifier_buffer[&AxisType::Y],
        xmax: label_boundary.xmax - modifier_buffer[&AxisType::SecondaryY],
        ymin: label_boundary.ymin + modifier_buffer[&AxisType::X],
        ymax: label_boundary.ymax - modifier_buffer[&AxisType::SecondaryX],
    };
    let tick_label_boundary = draw::Area {
        xmin: modifier_boundary.xmin + tick_label_buffer[&AxisType::Y],
        xmax: modifier_boundary.xmax - tick_label_buffer[&AxisType::SecondaryY],
        ymin: modifier_boundary.ymin + tick_label_buffer[&AxisType::X],
        ymax: modifier_boundary.ymax - tick_label_buffer[&AxisType::SecondaryX],
    };
    let tick_boundary = draw::Area {
        xmin: tick_label_boundary.xmin + tick_buffer[&AxisType::Y],
        xmax: tick_label_boundary.xmax - tick_buffer[&AxisType::SecondaryY],
        ymin: tick_label_boundary.ymin + tick_buffer[&AxisType::X],
        ymax: tick_label_boundary.ymax - tick_buffer[&AxisType::SecondaryX],
    };

    // plot area in figure as pixel indices
    let plot_area = draw::Area {
        xmin: tick_boundary.xmin,
        xmax: tick_boundary.xmax,
        ymin: tick_boundary.ymin,
        ymax: tick_boundary.ymax,
    };

    // set plot color
    canvas.draw_shape(draw::ShapeDescriptor {
        point: draw::Point {
            x: plot_area.xmin as f64 + plot_area.xsize() as f64 / 2.0,
            y: plot_area.ymin as f64 + plot_area.ysize() as f64 / 2.0,
        },
        shape: draw::Shape::Rectangle {
            h: plot_area.ysize(),
            w: plot_area.xsize(),
        },
        fill_color: subplot.format.plot_color,
        line_color: Color::TRANSPARENT,
        ..Default::default()
    });

    // draw grid lines
    for (placement, axis) in finalized_axes.iter() {
        // draw ticks
        for (ticks, grid) in [
            (&axis.major_tick_locs, &axis.major_grid),
            (&axis.minor_tick_locs, &axis.minor_grid),
        ] {
            // convert tick numbers to pixel locations
            let tick_locs = ticks.iter()
                // convert to fraction
                .map(|tick| {
                    (tick - axis.limits.0) / (axis.limits.1 - axis.limits.0)
                })
                // convert to pixel
                .map(|frac| {
                    plot_area.fractional_to_point(draw::Point { x: frac, y: frac })
                })
                .collect::<Vec<_>>();

            // draw grid lines
            if *grid {
                for loc in tick_locs.iter() {
                    let line = match placement {
                        AxisType::Y | AxisType::SecondaryY => draw::Line {
                            p1: draw::Point {
                                x: plot_area.xmin as f64,
                                y: loc.y.round(),
                            },
                            p2: draw::Point {
                                x: plot_area.xmax as f64,
                                y: loc.y.round(),
                            },
                        },
                        AxisType::X | AxisType::SecondaryX => draw::Line {
                            p1: draw::Point {
                                x: loc.x.round(),
                                y: plot_area.ymin as f64,
                            },
                            p2: draw::Point {
                                x: loc.x.round(),
                                y: plot_area.ymax as f64,
                            },
                        },
                    };
                    canvas.draw_line(draw::LineDescriptor {
                        line,
                        line_color: grid_color,
                        line_width,
                        ..Default::default()
                    });
                }
            }
        }
    }

    // draw data curve

    // if there is a color cycle, default to those colors, otherwise default to black
    let default_color = if !subplot.format.color_cycle.is_empty() {
        subplot.format.color_cycle.clone()
    } else {
        vec![default_marker_color]
    };
    let mut default_color = default_color.iter().cycle();

    // draw each plot
    for plot_info in subplot.plot_infos.iter() {
        let xlim = finalized_axes[&plot_info.xaxis].limits;
        let ylim = finalized_axes[&plot_info.yaxis].limits;
        let plot_data = &plot_info.data;

        // draw line
        if let Some(line) = plot_info.line {
            let line_color = if let Some(color) = line.color_override {
                color
            } else {
                *default_color.next().unwrap()
            };
            let dashes = match line.style {
                LineStyle::Solid => vec![
                ],
                LineStyle::Dashed => vec![
                    (10.0 * scaling).into(),
                    (10.0 * scaling).into(),
                    (10.0 * scaling).into(),
                    (10.0 * scaling).into(),
                ],
                LineStyle::ShortDashed => vec![
                    (4.0 * scaling).into(),
                    (4.0 * scaling).into(),
                    (4.0 * scaling).into(),
                    (4.0 * scaling).into(),
                ],
            };
            canvas.draw_curve(draw::CurveDescriptor {
                points: plot_data.data()
                    .map(|(x, y)| {
                        let xfrac = (x - xlim.0) / (xlim.1 - xlim.0);
                        let yfrac = (y - ylim.0) / (ylim.1 - ylim.0);

                        plot_area.fractional_to_point(draw::Point { x: xfrac, y: yfrac })
                    })
                    .collect::<Vec<_>>(),
                line_color,
                line_width: line.width * scaling.round() as u32,
                dashes: dashes.as_slice(),
                clip_area: Some(plot_area),
            });
        }

        // draw markers
        if let Some(marker) = &plot_info.marker {
            let mut shape = match marker.style {
                MarkerStyle::Circle => draw::Shape::Circle { r: marker.size },
                MarkerStyle::Square => draw::Shape::Square { l: marker.size },
            };
            shape.scale(scaling.round() as u32);
            let fill_color = if let Some(color) = marker.color_override {
                color
            } else {
                *default_color.next().unwrap()
            };
            let line = if let Some(line) = marker.outline {
                line
            } else {
                Line {
                    style: LineStyle::Solid,
                    width: Line::default().width,
                    color_override: Some(Color::TRANSPARENT),
                }
            };
            let line_color = if let Some(color) = line.color_override {
                color
            } else {
                fill_color
            };
            let line_dashes = match line.style {
                LineStyle::Solid => vec![
                ],
                LineStyle::Dashed => vec![
                    (10.0 * scaling).into(),
                    (10.0 * scaling).into(),
                    (10.0 * scaling).into(),
                    (10.0 * scaling).into(),
                ],
                LineStyle::ShortDashed => vec![
                    (4.0 * scaling).into(),
                    (4.0 * scaling).into(),
                    (4.0 * scaling).into(),
                    (4.0 * scaling).into(),
                ],
            };
            for point in plot_data.data().map(|(x, y)| {
                let xfrac = (x - xlim.0) / (xlim.1 - xlim.0);
                let yfrac = (y - ylim.0) / (ylim.1 - ylim.0);

                plot_area.fractional_to_point(draw::Point { x: xfrac, y: yfrac })
            }) {
                canvas.draw_shape(draw::ShapeDescriptor {
                    point,
                    shape,
                    fill_color,
                    line_color,
                    line_width: line.width * scaling.round() as u32,
                    line_dashes: line_dashes.as_slice(),
                    clip_area: Some(plot_area),
                });
            }
        }
    }

    // draw axis lines, labels, ticks, and tick labels for each axis
    for (placement, axis) in finalized_axes {
        // get line placement
        let axis_offset = line_width as f64 / 2.0;
        let line = match placement {
            AxisType::Y => {
                draw::Line {
                    p1: draw::Point {
                        x: plot_area.xmin as f64,
                        y: plot_area.ymin as f64 + axis_offset
                    },
                    p2: draw::Point {
                        x: plot_area.xmin as f64,
                        y: plot_area.ymax as f64 + axis_offset
                    },
                }
            },
            AxisType::SecondaryY => {
                draw::Line {
                    p1: draw::Point {
                        x: plot_area.xmax as f64,
                        y: plot_area.ymin as f64 - axis_offset
                    },
                    p2: draw::Point {
                        x: plot_area.xmax as f64,
                        y: plot_area.ymax as f64 - axis_offset
                    },
                }
            },
            AxisType::X => {
                draw::Line {
                    p1: draw::Point {
                        x: plot_area.xmin as f64 - axis_offset,
                        y: plot_area.ymin as f64
                    },
                    p2: draw::Point {
                        x: plot_area.xmax as f64 - axis_offset,
                        y: plot_area.ymin as f64
                    },
                }
            },
            AxisType::SecondaryX => {
                draw::Line {
                    p1: draw::Point {
                        x: plot_area.xmin as f64 + axis_offset,
                        y: plot_area.ymax as f64
                    },
                    p2: draw::Point {
                        x: plot_area.xmax as f64 + axis_offset,
                        y: plot_area.ymax as f64
                    },
                }
            },
        };

        // draw axis
        canvas.draw_line(draw::LineDescriptor {
            line,
            line_width,
            line_color,
            ..Default::default()
        });

        // draw tick label modifiers if necessary
        let mult_offset_text = if axis.label_multiplier != 0 && axis.label_offset != 0.0 {
            let exponent = superscript(axis.label_multiplier as u16);
            format!("x10{} + {}", exponent, axis.label_offset)
        } else if axis.label_multiplier != 0 {
            let exponent = superscript(axis.label_multiplier as u16);
            format!("x10{}", exponent)
        } else if axis.label_offset != 0.0 {
            format!("+ {}", axis.label_offset)
        } else {
            String::new()
        };
        // determine position of modifier
        let (modifier_position, modifier_alignment) = match placement {
            AxisType::Y => (
                draw::Point {
                    x: plot_area.xmin as f64 - letter_size.width as f64 / 2.0,
                    y: modifier_boundary.ymax as f64,
                },
                draw::Alignment::BottomLeft,
            ),
            AxisType::SecondaryY => (
                draw::Point {
                    x: plot_area.xmax as f64 - letter_size.width as f64 / 2.0,
                    y: modifier_boundary.ymax as f64,
                },
                draw::Alignment::BottomLeft,
            ),
            AxisType::SecondaryX => (
                draw::Point {
                    x: tick_label_boundary.xmax as f64 + letter_size.width as f64,
                    y: tick_label_boundary.ymax as f64,
                },
                draw::Alignment::BottomLeft,
            ),
            AxisType::X => (
                draw::Point {
                    x: plot_area.xmax as f64,
                    y: modifier_boundary.ymin as f64,
                },
                draw::Alignment::TopRight,
            ),
        };
        canvas.draw_text(draw::TextDescriptor {
            text: mult_offset_text,
            position: modifier_position,
            alignment: modifier_alignment,
            color: font_color,
            font: draw::Font {
                name: font_name,
                size: font_size,
                ..Default::default()
            },
            ..Default::default()
        });

        // draw axis label
        let label_font = draw::Font {
            name: font_name,
            size: font_size,
            ..Default::default()
        };
        match placement {
            AxisType::Y => canvas.draw_text(draw::TextDescriptor {
                text: axis.label,
                position: draw::Point {
                    x: label_boundary.xmin as f64,
                    y: (plot_area.ymax + plot_area.ymin) as f64 / 2.0,
                },
                alignment: draw::Alignment::Right,
                rotation: 1.5 * std::f64::consts::PI,
                color: font_color,
                font: label_font,
                ..Default::default()
            }),
            AxisType::X => canvas.draw_text(draw::TextDescriptor {
                text: axis.label,
                position: draw::Point {
                    x: (plot_area.xmax + plot_area.xmin) as f64 / 2.0,
                    y: label_boundary.ymin as f64,
                },
                alignment: draw::Alignment::Top,
                rotation: 0.0,
                color: font_color,
                font: label_font,
                ..Default::default()
            }),
            AxisType::SecondaryY => canvas.draw_text(draw::TextDescriptor {
                text: axis.label,
                position: draw::Point {
                    x: label_boundary.xmax as f64,
                    y: (plot_area.ymax + plot_area.ymin) as f64 / 2.0,
                },
                alignment: draw::Alignment::Left,
                rotation: 0.5 * std::f64::consts::PI,
                color: font_color,
                font: label_font,
                ..Default::default()
            }),
            AxisType::SecondaryX => canvas.draw_text(draw::TextDescriptor {
                text: axis.label,
                position: draw::Point {
                    x: (plot_area.xmax + plot_area.xmin) as f64 / 2.0,
                    y: label_boundary.ymax as f64,
                },
                alignment: draw::Alignment::Bottom,
                rotation: 0.0,
                color: font_color,
                font: label_font,
                ..Default::default()
            }),
        }

        // draw ticks
        for (ticks, labels, outer_tick_length, inner_tick_length) in [
            (
                axis.major_tick_locs, axis.major_tick_labels,
                outer_major_tick_length, inner_major_tick_length,
            ),
            (
                axis.minor_tick_locs, axis.minor_tick_labels,
                outer_minor_tick_length, inner_minor_tick_length,
            ),
        ] {
            // deal with cases of no provided labels or wrong number of labels
            let labels = if labels.is_empty() {
                (0..ticks.len()).map(|_| String::new()).collect()
            } else if labels.len() != ticks.len() {
                let axis = match placement {
                    AxisType::Y => "y-axis",
                    AxisType::X => "x-axis",
                    AxisType::SecondaryY => "secondary y-axis",
                    AxisType::SecondaryX => "secondary x-axis",
                };
                return Err(PltError::BadTickLabels(format!(
                    "number of tick labels does not match number of ticks on {}",
                    axis,
                )))
            } else {
                labels
            };

            // convert tick numbers to pixel locations
            let tick_locs = ticks.iter()
                // convert to fraction
                .map(|tick| {
                    (tick - axis.limits.0) / (axis.limits.1 - axis.limits.0)
                })
                // convert to pixel
                .map(|frac| {
                    plot_area.fractional_to_point(draw::Point { x: frac, y: frac })
                })
                .collect::<Vec<_>>();

            // draw ticks and labels
            for (tick, loc) in iter::zip(labels, tick_locs) {
                // get positions specific to the axis
                let (tick_line, text_position, text_alignment) = match placement {
                    AxisType::Y => (
                        draw::Line {
                            p1: draw::Point {
                                x: (plot_area.xmin - outer_tick_length) as f64,
                                y: loc.y.round(),
                            },
                            p2: draw::Point {
                                x: (plot_area.xmin + inner_tick_length) as f64,
                                y: loc.y.round(),
                            },
                        },
                        draw::Point {
                            x: tick_label_boundary.xmin as f64,
                            y: loc.y.round(),
                        },
                        draw::Alignment::Right,
                    ),
                    AxisType::X => (
                        draw::Line {
                            p1: draw::Point {
                                x: loc.x.round(),
                                y: (plot_area.ymin - outer_tick_length) as f64,
                            },
                            p2: draw::Point {
                                x: loc.x.round(),
                                y: (plot_area.ymin + inner_tick_length) as f64,
                            },
                        },
                        draw::Point {
                            x: loc.x.round(),
                            y: tick_label_boundary.ymin as f64,
                        },
                        draw::Alignment::Top,
                    ),
                    AxisType::SecondaryY => (
                        draw::Line {
                            p1: draw::Point {
                                x: (plot_area.xmax - inner_tick_length) as f64,
                                y: loc.y.round(),
                            },
                            p2: draw::Point {
                                x: (plot_area.xmax + outer_tick_length) as f64,
                                y: loc.y.round(),
                            },
                        },
                        draw::Point {
                            x: tick_label_boundary.xmax as f64,
                            y: loc.y.round(),
                        },
                        draw::Alignment::Left,
                    ),
                    AxisType::SecondaryX => (
                        draw::Line {
                            p1: draw::Point {
                                x: loc.x.round(),
                                y: (plot_area.ymax - inner_tick_length) as f64,
                            },
                            p2: draw::Point {
                                x: loc.x.round(),
                                y: (plot_area.ymax + outer_tick_length) as f64,
                            },
                        },
                        draw::Point {
                            x: loc.x.round(),
                            y: tick_label_boundary.ymax as f64,
                        },
                        draw::Alignment::Bottom,
                    ),
                };

                // draw line and text
                canvas.draw_line(draw::LineDescriptor {
                    line: tick_line,
                    line_color,
                    line_width,
                    ..Default::default()
                });
                canvas.draw_text(draw::TextDescriptor {
                    text: tick.to_string(),
                    position: text_position,
                    alignment: text_alignment,
                    color: font_color,
                    font: draw::Font {
                        name: font_name,
                        size: font_size,
                        ..Default::default()
                    },
                    ..Default::default()
                });
            };
        }
    }

    // draw title
    canvas.draw_text(draw::TextDescriptor {
        text: subplot.title.clone(),
        position: draw::Point {
            x: (plot_area.xmax + plot_area.xmin) as f64 / 2.0,
            y: title_boundary as f64,
        },
        alignment: draw::Alignment::Bottom,
        color: font_color,
        font: draw::Font {
            name: font_name,
            size: font_size,
            ..Default::default()
        },
        ..Default::default()
    });

    Ok(())
}
