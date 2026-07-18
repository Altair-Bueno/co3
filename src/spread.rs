use crate::ReprC;

pub trait Spread: ReprC + core::marker::Sized {
    type Part1: ReprC;
    type Part2: ReprC;

    /// Consumes the spreadable type, returning its constituents.
    fn into_parts(self) -> (Self::Part1, Self::Part2);

    /// Forms the spreadable type from its constituents.
    fn from_parts(part1: Self::Part1, part2: Self::Part2) -> Self;
}
