use std::path::Path;

use ort::{
    execution_providers::CoreMLExecutionProvider,
    session::{builder::GraphOptimizationLevel, Session},
};

trait LayoutParser<I> {
    type ParseResult;
    type Error;

    fn parse_layout(&self, input: I) -> Result<Self::ParseResult, Self::Error>;
}

pub struct ORTLayoutParser {
    session: Session,
}

impl ORTLayoutParser {
    pub fn new<P: AsRef<Path>>(model_path: P) -> anyhow::Result<Self> {
        let session = Session::builder()?
            .with_execution_providers([CoreMLExecutionProvider::default()
                .with_subgraphs()
                .with_ane_only()
                .build()])?
            .with_optimization_level(GraphOptimizationLevel::Level1)?
            .with_intra_threads(8)?
            .commit_from_file(model_path)?;
        Ok(Self { session })
    }
}

impl LayoutParser<ndarray::Array4<f32>> for ORTLayoutParser {
    type ParseResult = ();
    type Error = anyhow::Error;

    fn parse_layout(&self, input: ndarray::Array4<f32>) -> Result<Self::ParseResult, Self::Error> {
        let input = ndarray::Array4::<f32>::ones((1, 3, 1024, 768));

        for _ in 0..32 {
            let _run_result = &self.session.run(ort::inputs!["images"=> input.clone()]?)?;
        }

        Ok(())
    }
}
