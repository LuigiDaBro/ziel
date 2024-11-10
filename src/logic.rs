use core::fmt;
use std::{mem, ops};

#[derive(Debug, Clone, Copy)]
pub enum ShipPlan {
    Horizontal { pos: Position, len: u8 },
    Vertical { pos: Position, len: u8 },
}

#[derive(Debug, Clone, Copy)]
pub struct Ship(ShipPlan);

impl From<Ship> for ShipPlan {
    fn from(value: Ship) -> Self {
        value.0
    }
}

impl From<&Ship> for ShipPlan {
    fn from(value: &Ship) -> Self {
        value.0
    }
}

impl TryFrom<ShipPlan> for Ship {
    type Error = ();

    fn try_from(value: ShipPlan) -> Result<Self, Self::Error> {
        if match value {
            ShipPlan::Horizontal { pos, len } => pos.coords().0 + len <= 10,
            ShipPlan::Vertical { pos, len } => pos.coords().1 + len <= 10,
        } {
            Ok(Ship(value))
        } else {
            Err(())
        }
    }
}

impl IntoIterator for Ship {
    type Item = Position;

    type IntoIter = ShipPositionIter;

    fn into_iter(self) -> Self::IntoIter {
        ShipPositionIter(self.0)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("ship overlap")]
    ShipOverlap,
    #[error("invalid ship lengths")]
    InvalidShipLengths,
    #[error("already occupied target position")]
    OccupiedTargetPosition,
}

#[derive(Clone, Copy, Debug)]
pub struct Ships([Ship; 5]);
impl Ships {
    pub fn asarray(&self) -> &[Ship; 5] {
        &self.0
    }
}

impl IntoIterator for Ships {
    type Item = <[Ship; 5] as IntoIterator>::Item;

    type IntoIter = <[Ship; 5] as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl ops::Index<usize> for Ships {
    type Output = Ship;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl TryFrom<[Ship; 5]> for Ships {
    type Error = Error;

    fn try_from(ships: [Ship; 5]) -> Result<Self, Self::Error> {
        const SHIPLENGTHS: [u8; 5] = [2, 3, 3, 4, 5];

        let mut shipmap = [[false; 10]; 10];
        let mut shiplenmap = [false; SHIPLENGTHS.len()];
        for ship in ships {
            let shiplen = match ship.into() {
                ShipPlan::Horizontal { len, .. } => len,
                ShipPlan::Vertical { len, .. } => len,
            };

            *Iterator::zip(shiplenmap.iter_mut(), SHIPLENGTHS.into_iter())
                .find_map(|(found, len)| {
                    if !*found && len == shiplen {
                        Some(found)
                    } else {
                        None
                    }
                })
                .ok_or(Error::InvalidShipLengths)? = true;

            for pos in ship {
                let (x, y) = pos.coords();
                if mem::replace(&mut shipmap[y as usize][x as usize], true) {
                    return Err(Error::ShipOverlap);
                }
            }
        }

        Ok(Ships(ships))
    }
}

pub struct ShipPositionIter(ShipPlan);

impl Iterator for ShipPositionIter {
    type Item = Position;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 {
            ShipPlan::Horizontal { pos, len } => {
                let len = len.checked_sub(1)?;
                self.0 = ShipPlan::Horizontal { pos, len };
                let (x, y) = pos.coords();
                Some(Position::fromcoords(x + len, y).unwrap())
            }
            ShipPlan::Vertical { pos, len } => {
                let len = len.checked_sub(1)?;
                self.0 = ShipPlan::Vertical { pos, len };
                let (x, y) = pos.coords();
                Some(Position::fromcoords(x, y + len).unwrap())
            }
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Position(u8);

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (x, y) = self.coords();
        write!(f, "Position {{ x: {:?} y: {:?} }}", x, y,)
    }
}

impl Position {
    pub fn frombyte(i: u8) -> Option<Position> {
        let (x, y) = Position::coords(Position(i));
        Position::fromcoords(x, y)
    }

    pub fn byte(self) -> u8 {
        self.0
    }

    pub fn fromcoords(x: u8, y: u8) -> Option<Position> {
        if x < 10 && y < 10 {
            Some(Position(x + (y << 4)))
        } else {
            None
        }
    }

    pub fn coords(self) -> (u8, u8) {
        (self.0 & 0x0f, self.0 >> 4)
    }

    pub fn toboard(self) -> [&'static str; 2] {
        const MAPX: [&str; 10] = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"];
        const MAPY: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];
        let (x, y) = self.coords();
        [MAPX[x as usize], MAPY[y as usize]]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ShipReference(u8);

impl fmt::Debug for ShipReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ShipReference ({:?})", self.inner())
    }
}

impl ShipReference {
    pub fn empty() -> ShipReference {
        ShipReference(u8::MAX)
    }

    pub fn occupied(idx: u8) -> ShipReference {
        ShipReference(idx)
    }

    pub fn inner(self) -> Option<u8> {
        if self.0 == u8::MAX {
            None
        } else {
            Some(self.0)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AttackInfo {
    Hit(bool),
    Miss,
}

#[derive(Debug, Clone)]
pub struct Board {
    ships: Ships,
    shipmap: [[ShipReference; 10]; 10],
    hitmap: [[bool; 10]; 10],
}

pub fn validshippos(ships: &[Ship; 5]) -> bool {
    let mut shipmap = [[false; 10]; 10];
    for ship in ships {
        for pos in *ship {
            let (x, y) = pos.coords();
            if mem::replace(&mut shipmap[y as usize][x as usize], true) {
                return false;
            }
        }
    }
    true
}

impl Board {
    pub fn new(ships: Ships) -> Board {
        let mut shipmap = [[ShipReference::empty(); 10]; 10];
        for (i, ship) in ships.into_iter().enumerate() {
            for pos in ship {
                let (x, y) = pos.coords();
                shipmap[y as usize][x as usize] = ShipReference::occupied(i as u8);
            }
        }

        Board {
            ships,
            shipmap,
            hitmap: [[false; 10]; 10],
        }
    }

    pub fn target(&mut self, pos: Position) -> Option<AttackInfo> {
        let (x, y) = pos.coords();

        // if already hit
        if mem::replace(&mut self.hitmap[y as usize][x as usize], true) {
            return None;
        }

        match self.shipmap[y as usize][x as usize].inner() {
            Some(shipref) => Some(AttackInfo::Hit(
                self.ships[shipref as usize].into_iter().all(|p| {
                    let (x, y) = p.coords();
                    self.hitmap[y as usize][x as usize]
                }),
            )),
            None => Some(AttackInfo::Miss),
        }
    }

    pub fn allsunken(&self) -> bool {
        self.ships.into_iter().all(|ship| {
            ship.into_iter().all(|p| {
                let (x, y) = p.coords();
                self.hitmap[y as usize][x as usize]
            })
        })
    }

    pub fn ships(&self) -> &Ships {
        &self.ships
    }
}
