use crate::RobustReprC;

pub trait Spread: RobustReprC + core::marker::Sized {
    type Part1: RobustReprC;
    type Part2: RobustReprC;

    /// Consumes the spreadable type, returning its constituents.
    fn into_parts(self) -> (Self::Part1, Self::Part2);

    /// Forms the spreadable type from its constituents.
    fn from_parts(part1: Self::Part1, part2: Self::Part2) -> Self;
}
