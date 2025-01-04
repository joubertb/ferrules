pub mod model;

pub struct LayoutResult;

pub trait LayoutParser<I> {
    type Error;

    fn parse_layout(&self, input: I) -> Result<LayoutResult, Self::Error>;
}
