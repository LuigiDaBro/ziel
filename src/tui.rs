use itertools::Itertools;
use std::{array, io, iter, time};

use crossterm::event::{self, KeyCode, KeyEventKind};
use ratatui::{
    layout,
    style::{self, Stylize},
    symbols, text,
    widgets::{self, canvas},
};

use crate::{client, logic};

const SHIPCOLOR: [style::Color; 5] = [
    style::Color::from_u32(0xffcdb2),
    style::Color::from_u32(0xffb4a2),
    style::Color::from_u32(0xe5989b),
    style::Color::from_u32(0xb5838d),
    style::Color::from_u32(0x6d6875),
];

const ATTACKHITCOLOR: style::Color = style::Color::LightRed;
const ATTACKMISSCOLOR: style::Color = style::Color::White;

impl<'s> TryFrom<client::Message> for text::Line<'s> {
    type Error = ();

    fn try_from(value: client::Message) -> Result<Self, Self::Error> {
        match value {
            client::Message::SuccessfullyConnected => {
                Ok(text::Line::from("successfully connected"))
            }
            client::Message::ShipHit => Ok(text::Line::from(vec![
                text::Span::raw("ship "),
                text::Span::styled("hit", style::Style::new().light_red()),
            ])),
            client::Message::ShipSunken => Ok(text::Line::from(vec![
                text::Span::raw("ship "),
                text::Span::styled("sunken", style::Style::new().light_red()),
            ])),
            client::Message::ShipMissed => Ok(text::Line::from(vec![
                text::Span::styled("opp. ", style::Style::new().cyan()),
                text::Span::styled("missed", style::Style::new().yellow()),
            ])),
            client::Message::OppShipHit => Ok(text::Line::from(vec![
                text::Span::styled("opp. ", style::Style::new().cyan()),
                text::Span::raw("ship "),
                text::Span::styled("hit", style::Style::new().yellow()),
            ])),
            client::Message::OppShipSunken => Ok(text::Line::from(vec![
                text::Span::styled("opp.", style::Style::new().cyan()),
                text::Span::raw(" ship "),
                text::Span::styled("sunken", style::Style::new().yellow()),
            ])),
            client::Message::OppShipMissed => Ok(text::Line::from(vec![
                text::Span::raw("you "),
                text::Span::styled("missed", style::Style::new().light_red()),
            ])),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub struct Interface {
    term: ratatui::DefaultTerminal,
    cursorpos: (u8, u8),
}

impl Interface {
    pub fn new() -> Interface {
        Interface {
            term: ratatui::init(),
            cursorpos: (0, 0),
        }
    }
}

impl Default for Interface {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Interface {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

impl client::UI for Interface {
    type Error = io::Error;

    fn buildboard(&mut self) -> Result<logic::Ships, client::UIError<io::Error>> {
        const SHIPLEN: [u8; 5] = [5, 4, 3, 3, 2];
        let mut ships: [logic::Ship; 5] = array::from_fn(|i| {
            logic::ShipPlan::Vertical {
                pos: logic::Position::fromcoords(i as u8, 0).unwrap(),
                len: SHIPLEN[i],
            }
            .try_into()
            .unwrap()
        });

        let mut x = 0;
        let mut y = 0;
        loop {
            match event::read()? {
                event::Event::Key(kevent) if kevent.kind == KeyEventKind::Press => {
                    match kevent.code {
                        KeyCode::Char('a') | KeyCode::Left if x > 0 => x -= 1,
                        KeyCode::Char('w') | KeyCode::Up if y > 0 => y -= 1,
                        KeyCode::Char('d') | KeyCode::Right if x < 9 => x += 1,
                        KeyCode::Char('s') | KeyCode::Down if y < 9 => y += 1,
                        KeyCode::Char('q') => {
                            return Err(
                                io::Error::new(io::ErrorKind::Other, "player interrupted").into()
                            )
                        }
                        KeyCode::Char(' ') => {
                            let cpos = logic::Position::fromcoords(x, y).unwrap();
                            for (i, ship) in ships.into_iter().enumerate() {
                                if ship.into_iter().any(|p| p == cpos) {
                                    moveship(&mut self.term, &mut x, &mut y, &mut ships, i)?;
                                    continue;
                                }
                            }
                        }
                        KeyCode::Enter => break,
                        _ => {}
                    }
                }
                _ => {}
            }

            self.term.draw(|f| {
                let [boardx, boardy] = logic::Position::fromcoords(x, y).unwrap().toboard();
                let canvas = canvas::Canvas::default()
                    .block(
                        widgets::Block::bordered()
                            .border_type(widgets::BorderType::Thick)
                            .title_bottom(text::Line::raw(format!("{boardx}{boardy}"))),
                    )
                    .x_bounds([0.0, 9.0])
                    .y_bounds([0.0, 9.0])
                    .marker(symbols::Marker::HalfBlock)
                    .paint(|ctx| {
                        drawships(ctx, &ships);
                        ctx.draw(&canvas::Points {
                            coords: &[(x as f64, (9 - y) as f64)],
                            color: style::Color::White,
                        });
                    });

                f.render_widget(canvas, centerrectinrect(f.area(), layout::Size::new(12, 7)));
            })?;
        }

        self.cursorpos = (x, y);
        Ok(logic::Ships::try_from(ships).unwrap())
    }

    fn displayboard(&mut self, info: client::ClientInfo) -> Result<(), client::UIError<io::Error>> {
        self.term.draw(|f| {
            let rect = centerrectinrect(
                f.area(),
                layout::Size {
                    width: 23,
                    height: 7,
                },
            );
            let rectleft = layout::Rect {
                x: rect.x,
                y: rect.y,
                width: 11,
                height: rect.height,
            };
            let rectright = layout::Rect {
                x: rectleft.x + rectleft.width,
                y: rect.y,
                width: 12,
                height: rect.height,
            };
            let rectbottom = layout::Rect {
                x: rectleft.x,
                y: rectleft.y + rectleft.height,
                width: rect.width,
                height: f.area().height - rectleft.y - rectleft.height,
            };

            let blockleft = widgets::Block::bordered()
                .border_type(widgets::BorderType::Thick)
                .borders(widgets::Borders::TOP | widgets::Borders::LEFT | widgets::Borders::BOTTOM);

            let blockrightsymbols = symbols::border::Set {
                top_left: symbols::line::THICK_HORIZONTAL_DOWN,
                bottom_left: symbols::line::THICK_HORIZONTAL_UP,
                ..symbols::border::THICK
            };

            let blockright = widgets::Block::bordered()
                .border_type(widgets::BorderType::Thick)
                .border_set(blockrightsymbols);

            let canvasleft = canvas::Canvas::default()
                .block(blockleft)
                .x_bounds([0.0, 9.0])
                .y_bounds([0.0, 9.0])
                .marker(symbols::Marker::HalfBlock)
                .paint(|ctx| {
                    drawships(ctx, info.ships);
                    drawhits(ctx, info.selfhits);
                });

            let canvasright = canvas::Canvas::default()
                .block(blockright)
                .x_bounds([0.0, 9.0])
                .y_bounds([0.0, 9.0])
                .marker(symbols::Marker::HalfBlock)
                .paint(|ctx| {
                    drawhits(ctx, info.opphits);
                });

            f.render_widget(canvasleft, rectleft);
            f.render_widget(canvasright, rectright);
            let msg: Vec<_> = info
                .message
                .iter()
                .rev()
                .cloned()
                .map(text::Line::try_from)
                .filter_map(Result::ok)
                .map(|line| line.style(style::Style::new().gray()))
                .collect();
            f.render_widget(
                widgets::Paragraph::new(msg).wrap(widgets::Wrap { trim: true }),
                rectbottom,
            )
        })?;
        Ok(())
    }

    fn selecttarget(
        &mut self,
        info: client::ClientInfo,
    ) -> Result<logic::Position, client::UIError<io::Error>> {
        let (mut x, mut y) = self.cursorpos;

        while let Ok(true) = event::poll(time::Duration::from_secs(0)) {
            event::read()?;
        }

        loop {
            let mut checkready = false;
            match event::read()? {
                event::Event::Key(kevent) if kevent.kind == KeyEventKind::Press => {
                    match kevent.code {
                        KeyCode::Char('a') | KeyCode::Left if x > 0 => x -= 1,
                        KeyCode::Char('w') | KeyCode::Up if y > 0 => y -= 1,
                        KeyCode::Char('d') | KeyCode::Right if x < 9 => x += 1,
                        KeyCode::Char('s') | KeyCode::Down if y < 9 => y += 1,
                        KeyCode::Char('q') => {
                            return Err(
                                io::Error::new(io::ErrorKind::Other, "player interrupted").into()
                            )
                        }
                        KeyCode::Char(' ') => checkready = true,
                        _ => {}
                    }
                }
                _ => {}
            }

            let valid = info.opphits[y as usize][x as usize].is_none();
            if valid && checkready {
                self.cursorpos = (x, y);
                return Ok(logic::Position::fromcoords(x, y).unwrap());
            }

            self.term.draw(|f| {
                let rect = centerrectinrect(
                    f.area(),
                    layout::Size {
                        width: 23,
                        height: 7,
                    },
                );
                let rectleft = layout::Rect {
                    x: rect.x,
                    y: rect.y,
                    width: 11,
                    height: rect.height,
                };
                let rectright = layout::Rect {
                    x: rectleft.x + rectleft.width,
                    y: rect.y,
                    width: 12,
                    height: rect.height,
                };

                let rectbottom = layout::Rect {
                    x: rectleft.x,
                    y: rectleft.y + rectleft.height,
                    width: rect.width,
                    height: f.area().height - rectleft.y - rectleft.height,
                };

                let blockleft = widgets::Block::bordered()
                    .border_type(widgets::BorderType::Thick)
                    .borders(
                        widgets::Borders::TOP | widgets::Borders::LEFT | widgets::Borders::BOTTOM,
                    )
                    .border_style(if valid {
                        style::Style::new().green()
                    } else {
                        style::Style::new().red()
                    });

                let blockrightsymbols = symbols::border::Set {
                    top_left: symbols::line::THICK_HORIZONTAL_DOWN,
                    bottom_left: symbols::line::THICK_HORIZONTAL_UP,
                    ..symbols::border::THICK
                };

                let blockright = widgets::Block::bordered()
                    .title("select")
                    .border_type(widgets::BorderType::Thick)
                    .border_set(blockrightsymbols)
                    .border_style(if valid {
                        style::Style::new().green()
                    } else {
                        style::Style::new().red()
                    });

                let canvasleft = canvas::Canvas::default()
                    .block(blockleft)
                    .x_bounds([0.0, 9.0])
                    .y_bounds([0.0, 9.0])
                    .marker(symbols::Marker::HalfBlock)
                    .paint(|ctx| {
                        drawships(ctx, info.ships);
                        drawhits(ctx, info.selfhits);
                    });

                let [boardx, boardy] = logic::Position::fromcoords(x, y).unwrap().toboard();
                let canvasright = canvas::Canvas::default()
                    .block(blockright.title_bottom(format! {"{boardx}{boardy}"}))
                    .x_bounds([0.0, 9.0])
                    .y_bounds([0.0, 9.0])
                    .marker(symbols::Marker::HalfBlock)
                    .paint(|ctx| {
                        drawhits(ctx, info.opphits);
                        ctx.draw(&canvas::Points {
                            coords: &[(x as f64, (9 - y) as f64)],
                            color: style::Color::White,
                        });
                    });

                f.render_widget(canvasleft, rectleft);
                f.render_widget(canvasright, rectright);
                let msg: Vec<_> = info
                    .message
                    .iter()
                    .rev()
                    .cloned()
                    .map(text::Line::try_from)
                    .filter_map(Result::ok)
                    .map(|line| line.style(style::Style::new().gray()))
                    .collect();
                f.render_widget(
                    widgets::Paragraph::new(msg).wrap(widgets::Wrap { trim: true }),
                    rectbottom,
                )
            })?;
        }
    }

    fn displayvictory(
        &mut self,
        info: client::ClientInfo,
    ) -> Result<(), client::UIError<io::Error>> {
        const MESSAGE: &str = "V I C T O R Y";

        while let Ok(true) = event::poll(time::Duration::from_secs(0)) {
            event::read()?;
        }

        self.term.draw(|f| {
            let rect = centerrectinrect(
                f.area(),
                layout::Size {
                    width: 23,
                    height: 7,
                },
            );
            let rectleft = layout::Rect {
                x: rect.x,
                y: rect.y,
                width: 11,
                height: rect.height,
            };
            let rectright = layout::Rect {
                x: rectleft.x + rectleft.width,
                y: rect.y,
                width: 12,
                height: rect.height,
            };
            let rectbottom = layout::Rect {
                x: rectleft.x,
                y: rectleft.y + rectleft.height,
                width: rect.width,
                height: f.area().height - rectleft.y - rectleft.height,
            };
            let rectmessage = centerrectinrect(
                rect,
                layout::Size {
                    width: (MESSAGE.len() + 2) as u16,
                    height: 3,
                },
            );

            let blockleft = widgets::Block::bordered()
                .border_type(widgets::BorderType::Thick)
                .borders(widgets::Borders::TOP | widgets::Borders::LEFT | widgets::Borders::BOTTOM);

            let blockrightsymbols = symbols::border::Set {
                top_left: symbols::line::THICK_HORIZONTAL_DOWN,
                bottom_left: symbols::line::THICK_HORIZONTAL_UP,
                ..symbols::border::THICK
            };

            let blockright = widgets::Block::bordered()
                .border_type(widgets::BorderType::Thick)
                .border_set(blockrightsymbols);

            let canvasleft = canvas::Canvas::default()
                .block(blockleft)
                .x_bounds([0.0, 9.0])
                .y_bounds([0.0, 9.0])
                .marker(symbols::Marker::HalfBlock)
                .paint(|ctx| {
                    drawships(ctx, info.ships);
                    drawhits(ctx, info.selfhits);
                });

            let canvasright = canvas::Canvas::default()
                .block(blockright)
                .x_bounds([0.0, 9.0])
                .y_bounds([0.0, 9.0])
                .marker(symbols::Marker::HalfBlock)
                .paint(|ctx| {
                    drawhits(ctx, info.opphits);
                });

            f.render_widget(canvasleft, rectleft);
            f.render_widget(canvasright, rectright);
            let msg: Vec<_> = info
                .message
                .iter()
                .rev()
                .cloned()
                .map(text::Line::try_from)
                .filter_map(Result::ok)
                .map(|line| line.style(style::Style::new().gray()))
                .collect();
            f.render_widget(
                widgets::Paragraph::new(msg).wrap(widgets::Wrap { trim: true }),
                rectbottom,
            );
            f.render_widget(widgets::Clear, rectmessage);
            let rectmessage = layout::Rect {
                x: rectmessage.x + 1,
                y: rectmessage.y + 1,
                width: rectmessage.width - 2,
                height: 1,
            };
            f.render_widget(
                widgets::Paragraph::new(MESSAGE).bold().centered().yellow(),
                rectmessage,
            );
        })?;

        Ok(())
    }

    fn displayloss(&mut self, info: client::ClientInfo) -> Result<(), client::UIError<io::Error>> {
        const MESSAGE: &str = "L O S S";

        while let Ok(true) = event::poll(time::Duration::from_secs(0)) {
            event::read()?;
        }

        self.term.draw(|f| {
            let rect = centerrectinrect(
                f.area(),
                layout::Size {
                    width: 23,
                    height: 7,
                },
            );
            let rectleft = layout::Rect {
                x: rect.x,
                y: rect.y,
                width: 11,
                height: rect.height,
            };
            let rectright = layout::Rect {
                x: rectleft.x + rectleft.width,
                y: rect.y,
                width: 12,
                height: rect.height,
            };
            let rectbottom = layout::Rect {
                x: rectleft.x,
                y: rectleft.y + rectleft.height,
                width: rect.width,
                height: f.area().height - rectleft.y - rectleft.height,
            };
            let rectmessage = centerrectinrect(
                rect,
                layout::Size {
                    width: (MESSAGE.len() + 2) as u16,
                    height: 3,
                },
            );

            let blockleft = widgets::Block::bordered()
                .border_type(widgets::BorderType::Thick)
                .borders(widgets::Borders::TOP | widgets::Borders::LEFT | widgets::Borders::BOTTOM);

            let blockrightsymbols = symbols::border::Set {
                top_left: symbols::line::THICK_HORIZONTAL_DOWN,
                bottom_left: symbols::line::THICK_HORIZONTAL_UP,
                ..symbols::border::THICK
            };

            let blockright = widgets::Block::bordered()
                .border_type(widgets::BorderType::Thick)
                .border_set(blockrightsymbols);

            let canvasleft = canvas::Canvas::default()
                .block(blockleft)
                .x_bounds([0.0, 9.0])
                .y_bounds([0.0, 9.0])
                .marker(symbols::Marker::HalfBlock)
                .paint(|ctx| {
                    drawships(ctx, info.ships);
                    drawhits(ctx, info.selfhits);
                });

            let canvasright = canvas::Canvas::default()
                .block(blockright)
                .x_bounds([0.0, 9.0])
                .y_bounds([0.0, 9.0])
                .marker(symbols::Marker::HalfBlock)
                .paint(|ctx| {
                    drawhits(ctx, info.opphits);
                });

            f.render_widget(canvasleft, rectleft);
            f.render_widget(canvasright, rectright);
            let msg: Vec<_> = info
                .message
                .iter()
                .rev()
                .cloned()
                .map(text::Line::try_from)
                .filter_map(Result::ok)
                .map(|line| line.style(style::Style::new().gray()))
                .collect();
            f.render_widget(
                widgets::Paragraph::new(msg).wrap(widgets::Wrap { trim: true }),
                rectbottom,
            );
            f.render_widget(widgets::Clear, rectmessage);
            let rectmessage = layout::Rect {
                x: rectmessage.x + 1,
                y: rectmessage.y + 1,
                width: rectmessage.width - 2,
                height: 1,
            };
            f.render_widget(
                widgets::Paragraph::new(MESSAGE).bold().centered().cyan(),
                rectmessage,
            );
        })?;

        Ok(())
    }
}

fn centerrectinrect(rect: layout::Rect, size: layout::Size) -> layout::Rect {
    layout::Rect {
        x: rect.x + rect.width / 2 - size.width / 2,
        y: rect.y + rect.height / 2 - size.height / 2,
        width: size.width,
        height: size.height,
    }
}

fn drawships(ctx: &mut canvas::Context, ships: &[logic::Ship; 5]) {
    for (ship, color) in Iterator::zip(ships.iter(), SHIPCOLOR.into_iter()) {
        let line = match ship.into() {
            logic::ShipPlan::Horizontal { pos, len } => {
                let (x, y) = pos.coords();
                canvas::Line {
                    x1: x as f64,
                    y1: (9 - y) as f64,
                    x2: (x + len - 1) as f64,
                    y2: (9 - y) as f64,
                    color,
                }
            }
            logic::ShipPlan::Vertical { pos, len } => {
                let (x, y) = pos.coords();
                canvas::Line {
                    x1: x as f64,
                    y1: (9 - y) as f64,
                    x2: x as f64,
                    y2: (9 - (y + len - 1)) as f64,
                    color,
                }
            }
        };
        ctx.draw(&line);
    }
}

fn drawhits(ctx: &mut canvas::Context, hits: &[[Option<logic::AttackInfo>; 10]; 10]) {
    let (hit, missed): (Vec<_>, Vec<_>) = (0..10)
        .flat_map(|x| (0..10).map(move |y| (x, y)))
        .filter_map(|(x, y)| hits[y][x].map(|attackinfo| (attackinfo, x as f64, (9 - y) as f64)))
        .partition_map(|(attackinfo, x, y)| match attackinfo {
            logic::AttackInfo::Hit(_) => itertools::Either::Left((x, y)),
            logic::AttackInfo::Miss => itertools::Either::Right((x, y)),
        });
    ctx.draw(&canvas::Points {
        coords: &hit,
        color: ATTACKHITCOLOR,
    });
    ctx.draw(&canvas::Points {
        coords: &missed,
        color: ATTACKMISSCOLOR,
    });
}

fn moveship(
    term: &mut ratatui::DefaultTerminal,
    x: &mut u8,
    y: &mut u8,
    ships: &mut [logic::Ship; 5],
    idx: usize,
) -> io::Result<()> {
    let (shiplenoff, shiplen, mut horizontal) = match ships[idx].into() {
        logic::ShipPlan::Horizontal { pos, len } => (*x - pos.coords().0, len, true),
        logic::ShipPlan::Vertical { pos, len } => (*y - pos.coords().1, len, false),
    };

    loop {
        let mut checkready = false;
        match event::read()? {
            event::Event::Key(kevent) if kevent.kind == KeyEventKind::Press => match kevent.code {
                KeyCode::Char('a') | KeyCode::Left if *x > 0 => *x -= 1,
                KeyCode::Char('w') | KeyCode::Up if *y > 0 => *y -= 1,
                KeyCode::Char('d') | KeyCode::Right if *x < 9 => *x += 1,
                KeyCode::Char('s') | KeyCode::Down if *y < 9 => *y += 1,
                KeyCode::Char('r') => {
                    horizontal ^= true;
                }
                KeyCode::Char(' ') => checkready = true,
                KeyCode::Char('q') => {
                    return Err(io::Error::new(io::ErrorKind::Other, "player interrupted"))
                }
                _ => {}
            },
            _ => {}
        }

        *x = u8::clamp(
            *x,
            if horizontal { shiplenoff } else { 0 },
            if horizontal {
                10 - shiplen + shiplenoff
            } else {
                9
            },
        );
        *y = u8::clamp(
            *y,
            if horizontal { 0 } else { shiplenoff },
            if horizontal {
                9
            } else {
                10 - shiplen + shiplenoff
            },
        );

        ships[idx] = if horizontal {
            logic::ShipPlan::Horizontal {
                pos: logic::Position::fromcoords(*x - shiplenoff, *y).unwrap(),
                len: shiplen,
            }
        } else {
            logic::ShipPlan::Vertical {
                pos: logic::Position::fromcoords(*x, *y - shiplenoff).unwrap(),
                len: shiplen,
            }
        }
        .try_into()
        .unwrap();

        let valid = logic::validshippos(ships);

        if checkready && valid {
            return Ok(());
        }

        term.draw(|f| {
            let [boardx, boardy] = logic::Position::fromcoords(*x, *y).unwrap().toboard();
            let canvas = canvas::Canvas::default()
                .block(
                    widgets::Block::bordered()
                        .border_style(if valid {
                            style::Style::new().green()
                        } else {
                            style::Style::new().red()
                        })
                        .border_type(widgets::BorderType::Thick)
                        .title_bottom(text::Line::raw(format!("{boardx}{boardy}"))),
                )
                .x_bounds([0.0, 9.0])
                .y_bounds([0.0, 9.0])
                .marker(symbols::Marker::HalfBlock)
                .paint(|ctx| {
                    for (ship, color) in Iterator::zip(ships.iter(), SHIPCOLOR.into_iter())
                        .chain(iter::once((&ships[idx], SHIPCOLOR[idx])))
                    {
                        let line = match ship.into() {
                            logic::ShipPlan::Horizontal { pos, len } => {
                                let (x, y) = pos.coords();
                                canvas::Line {
                                    x1: x as f64,
                                    y1: (9 - y) as f64,
                                    x2: (x + len - 1) as f64,
                                    y2: (9 - y) as f64,
                                    color,
                                }
                            }
                            logic::ShipPlan::Vertical { pos, len } => {
                                let (x, y) = pos.coords();
                                canvas::Line {
                                    x1: x as f64,
                                    y1: (9 - y) as f64,
                                    x2: x as f64,
                                    y2: (9 - (y + len - 1)) as f64,
                                    color,
                                }
                            }
                        };
                        ctx.draw(&line);
                    }
                    ctx.draw(&canvas::Points {
                        coords: &[(*x as f64, (9 - *y) as f64)],
                        color: style::Color::White,
                    });
                });
            f.render_widget(canvas, centerrectinrect(f.area(), layout::Size::new(12, 7)));
        })?;
    }
}
